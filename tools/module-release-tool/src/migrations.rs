//! Read-only migration mapping against the formal migration catalogs.
//!
//! The rule this module enforces is simple and is the reason it exists: a
//! candidate artifact may describe migrations, it may never invent them.
//!
//! * every claimed file must exist in the referenced catalog and must match the
//!   checksum recorded in that catalog's own `MANIFEST.sha256`, re-verified
//!   against the bytes on disk;
//! * a module-owned catalog must be claimed in full, so a module cannot quietly
//!   stop accounting for a migration it owns;
//! * a shared catalog is claimed file by file and every shared entry is pinned
//!   with `module_owned = false` plus the ownership note from the catalog header;
//! * historical migrations, checksums and SQL are never modified, and no pilot
//!   copy of any SQL is produced;
//! * what the mapping *cannot* prove — independent installation or upgrade of a
//!   single module on a given backend — is written down as a blocker inside the
//!   artifact instead of being implied by the presence of a digest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use business_module_contracts::{MigrationRevision, MigrationSetDigest};
use serde::{Deserialize, Serialize};

use crate::canonical::{
    fold_line_endings, migration_set_digest, sha256_hex, validate_sha256_hex, MigrationSetEntry,
};
use crate::repo::RepoContext;
use crate::spec::{MigrationSetSpec, ReleaseSpec};
use crate::{ToolError, ToolResult};

/// Schema identifier of `package/migrations/migration-mapping.json`.
pub const MIGRATION_MAPPING_SCHEMA_VERSION: &str = "plan-0014.wave5a.migration-mapping.v1";

/// One migration file as pinned by the formal catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationFileRecord {
    /// Backend identifier.
    pub backend: String,
    /// Catalog directory, repository relative.
    pub catalog: String,
    /// Checksum manifest the record was read from.
    pub catalog_manifest: String,
    /// File name inside the catalog.
    pub file_name: String,
    /// Numeric revision token from the file name, e.g. `001`.
    pub revision: String,
    /// Recorded SHA-256.
    pub sha256: String,
    /// Whether the recorded checksum matches the raw bytes or the
    /// line-ending-folded bytes (the repository stores files with
    /// `core.autocrlf=true`, so both forms are legitimately present).
    pub digest_form: String,
    /// Whether the module owns this file.
    pub module_owned: bool,
    /// Ledger table used by the runtime for this catalog, when it has one.
    pub ledger_table: Option<String>,
    /// Ownership statement copied from the catalog.
    pub ownership_note: String,
}

/// Per-backend assessment of independent install/upgrade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendAssessment {
    /// Backend identifier.
    pub backend: String,
    /// `BLOCKED` or `MAPPED`; never a claim that an upgrade was executed.
    pub status: String,
    /// Why, in terms a reviewer can check.
    pub reason: String,
}

/// The mapping written into the artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationMapping {
    /// Mapping schema identifier.
    pub schema_version: String,
    /// Owner-scoped migration namespace taken from the manifest declaration.
    pub migration_namespace: String,
    /// Whether the module owns persistent facts.
    pub persistent: bool,
    /// Pinned files, ordered by `(backend, file_name)`.
    pub files: Vec<MigrationFileRecord>,
    /// Canonical digest over `(namespace, backend, file, sha256)`.
    pub set_digest: String,
    /// What the mapping does not prove.
    pub assessments: Vec<BackendAssessment>,
    /// Authorities this mapping reads (files, not opinions).
    pub authority: Vec<String>,
}

impl MigrationMapping {
    /// The release-layer revision, or `None` for a stateless module.
    pub fn revision(&self) -> ToolResult<Option<MigrationRevision>> {
        if !self.persistent {
            return Ok(None);
        }
        Ok(Some(MigrationRevision {
            migration_namespace: self.migration_namespace.clone(),
            set_digest: MigrationSetDigest::new(self.set_digest.clone())?,
        }))
    }
}

/// Builds the mapping from the spec and the repository's formal catalogs.
pub fn build_mapping(context: &RepoContext, spec: &ReleaseSpec) -> ToolResult<MigrationMapping> {
    let mut files = Vec::new();
    let mut assessments = Vec::new();
    let mut authority = vec![
        "migrations/MANIFEST.sha256 (workspace PostgreSQL catalog)".to_owned(),
        "crates/identity-sqlite/migrations/MANIFEST.sha256".to_owned(),
        "crates/policy-sqlite/migrations/MANIFEST.sha256".to_owned(),
        "docs/reviews/2026-09-24-saas-align-module-packaging.md §8 G-203 / §16 D-02".to_owned(),
    ];
    authority.extend(
        spec.declaration_authority
            .iter()
            .map(|(field, source)| format!("{field}: {source}")),
    );

    for set in &spec.migration_sets {
        files.extend(verify_catalog(context.root(), set)?);
        assessments.push(assess(set));
    }

    files.sort_by(|left, right| {
        (left.backend.as_bytes(), left.file_name.as_bytes())
            .cmp(&(right.backend.as_bytes(), right.file_name.as_bytes()))
    });
    let entries: Vec<MigrationSetEntry> = files
        .iter()
        .map(|file| MigrationSetEntry {
            backend: file.backend.clone(),
            file_name: file.file_name.clone(),
            sha256: file.sha256.clone(),
        })
        .collect();
    let set_digest = migration_set_digest(&spec.migration_namespace, &entries);
    Ok(MigrationMapping {
        schema_version: MIGRATION_MAPPING_SCHEMA_VERSION.to_owned(),
        migration_namespace: spec.migration_namespace.clone(),
        persistent: spec.persistent,
        files,
        set_digest,
        assessments,
        authority,
    })
}

/// Checks one declared migration set against the formal catalog found under
/// `root` and returns the records to pin.
///
/// Every claimed file must appear in that catalog's own `MANIFEST.sha256` **and**
/// hash to the value it records — raw bytes, or the same bytes with LF line
/// endings, because the repository's two catalog checkouts differ only there.
/// A catalog declared `module_owned` must be claimed in full, so a module can
/// never quietly release a subset of its own schema; a catalog it does not own
/// may only be referenced, never claimed as the module's own.
pub fn verify_catalog(root: &Path, set: &MigrationSetSpec) -> ToolResult<Vec<MigrationFileRecord>> {
    let catalog_manifest = root.join(&set.catalog_manifest);
    let recorded = read_catalog_manifest(&catalog_manifest)?;
    let catalog_dir = root.join(&set.catalog);
    let mut files: Vec<MigrationFileRecord> = Vec::new();
    for file_name in &set.files {
        let expected = recorded
            .get(file_name)
            .ok_or_else(|| ToolError::MigrationCatalog {
                catalog: set.catalog.clone(),
                reason: format!(
                    "'{file_name}' is claimed by the release spec but is absent from '{}'",
                    set.catalog_manifest
                ),
            })?;
        validate_sha256_hex(expected, "migration catalog checksum")?;
        let path: PathBuf = catalog_dir.join(file_name);
        let shown = path.display().to_string();
        let raw = std::fs::read(&path).map_err(|source| ToolError::Io {
            path: shown,
            source,
        })?;
        let raw_digest = sha256_hex(&raw);
        let folded_digest = sha256_hex(&fold_line_endings(&raw));
        let digest_form = if raw_digest == *expected {
            "raw"
        } else if folded_digest == *expected {
            "lf-normalized"
        } else {
            return Err(ToolError::MigrationCatalog {
                catalog: set.catalog.clone(),
                reason: format!(
                    "'{file_name}' hashes to raw '{raw_digest}' / lf-normalized \
                     '{folded_digest}' but '{}' records '{expected}'",
                    set.catalog_manifest
                ),
            });
        };
        files.push(MigrationFileRecord {
            backend: set.backend.clone(),
            catalog: set.catalog.clone(),
            catalog_manifest: set.catalog_manifest.clone(),
            file_name: file_name.clone(),
            revision: revision_token(file_name, &set.catalog)?,
            sha256: expected.clone(),
            digest_form: digest_form.to_owned(),
            module_owned: set.module_owned,
            ledger_table: set.ledger_table.clone(),
            ownership_note: set.ownership_note.clone(),
        });
    }
    if set.module_owned {
        let mut unclaimed: Vec<String> = recorded
            .keys()
            .filter(|name| !files.iter().any(|file| &file.file_name == *name))
            .cloned()
            .collect();
        unclaimed.sort();
        if !unclaimed.is_empty() {
            return Err(ToolError::MigrationCatalog {
                catalog: set.catalog.clone(),
                reason: format!(
                    "catalog is declared module-owned but {} file(s) are not claimed: \
                     {unclaimed:?}",
                    unclaimed.len()
                ),
            });
        }
    }
    assert_revision_order(&set.backend, &files, &set.catalog)?;
    Ok(files)
}

/// Re-reads the catalogs and fails if anything moved since the artifact was
/// written.
pub fn reverify(
    context: &RepoContext,
    spec: &ReleaseSpec,
    recorded: &MigrationMapping,
) -> ToolResult<()> {
    let rebuilt = build_mapping(context, spec)?;
    if &rebuilt == recorded {
        return Ok(());
    }
    for (index, before) in rebuilt.files.iter().enumerate() {
        match recorded.files.get(index) {
            Some(after) if after == before => {}
            Some(after) => {
                return Err(ToolError::MigrationCatalog {
                    catalog: before.catalog.clone(),
                    reason: format!("artifact pins {after:?} but the catalog now reads {before:?}"),
                })
            }
            None => {
                return Err(ToolError::MigrationCatalog {
                    catalog: before.catalog.clone(),
                    reason: format!("catalog now contains an additional entry {before:?}"),
                })
            }
        }
    }
    Err(ToolError::MigrationCatalog {
        catalog: "migration mapping".to_owned(),
        reason: format!(
            "artifact pins set_digest '{}' but the catalog re-computes to '{}'",
            recorded.set_digest, rebuilt.set_digest
        ),
    })
}

fn assess(set: &MigrationSetSpec) -> BackendAssessment {
    if set.backend == "postgres" && !set.module_owned {
        return BackendAssessment {
            backend: set.backend.clone(),
            status: "BLOCKED".to_owned(),
            reason: format!(
                "'{}' is a shared workspace-level PostgreSQL catalog and '{}' is co-owned \
                 (identity-management, organization and policy in one file, one `public` \
                 schema, embedded wholesale by runtime-migration); independent installation or \
                 upgrade of this module alone is therefore not achievable and not proven here. \
                 R2 remains BLOCKED for this backend (PLAN-0014 Wave 2 review G-203).",
                set.catalog,
                set.files.join(", ")
            ),
        };
    }
    BackendAssessment {
        backend: set.backend.clone(),
        status: "MAPPED".to_owned(),
        reason: format!(
            "catalog '{}' is module-owned with its own MANIFEST.sha256{}; the mapping pins the \
             checksums, but no migration was executed by this tool, so install/upgrade \
             behaviour is unverified here.",
            set.catalog,
            set.ledger_table
                .as_ref()
                .map_or(String::new(), |table| format!(
                    " and ledger table '{table}'"
                ))
        ),
    }
}

fn read_catalog_manifest(path: &Path) -> ToolResult<BTreeMap<String, String>> {
    let shown = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    let mut entries = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, "  ");
        let digest = parts.next().unwrap_or_default();
        let name = parts.next().unwrap_or_default();
        // Catalog entries are lowercase `.sql` by repository convention; a
        // different spelling is not a migration file here, so this comparison is
        // deliberately case-sensitive.
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        if name.is_empty() || !name.ends_with(".sql") {
            return Err(ToolError::MigrationCatalog {
                catalog: shown,
                reason: format!("unparsable manifest line '{line}'"),
            });
        }
        validate_sha256_hex(digest, "migration catalog checksum")?;
        entries.insert(name.to_owned(), digest.to_owned());
    }
    Ok(entries)
}

fn revision_token(file_name: &str, catalog: &str) -> ToolResult<String> {
    let digits: String = file_name.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return Err(ToolError::MigrationCatalog {
            catalog: catalog.to_owned(),
            reason: format!("'{file_name}' has no numeric revision prefix"),
        });
    }
    Ok(digits)
}

fn assert_revision_order(
    backend: &str,
    files: &[MigrationFileRecord],
    catalog: &str,
) -> ToolResult<()> {
    let mut seen: Vec<(String, String)> = Vec::new();
    for file in files.iter().filter(|file| file.backend == backend) {
        if let Some((previous, _)) = seen.iter().find(|(previous, _)| *previous == file.revision) {
            let _ = previous;
            return Err(ToolError::MigrationCatalog {
                catalog: catalog.to_owned(),
                reason: format!(
                    "duplicate revision '{}' for backend '{backend}' in '{catalog}'",
                    file.revision
                ),
            });
        }
        seen.push((file.revision.clone(), file.file_name.clone()));
    }
    let mut ordered: Vec<&MigrationFileRecord> = files
        .iter()
        .filter(|file| file.backend == backend)
        .collect();
    ordered.sort_by(|left, right| left.revision.cmp(&right.revision));
    for pair in ordered.windows(2) {
        if pair[0].file_name > pair[1].file_name {
            return Err(ToolError::MigrationCatalog {
                catalog: catalog.to_owned(),
                reason: format!(
                    "revision order disagrees with file-name order between '{}' and '{}' for \
                     backend '{backend}'",
                    pair[0].file_name, pair[1].file_name
                ),
            });
        }
    }
    Ok(())
}
