//! Materialising, describing and verifying a local release candidate.
//!
//! Write order is a correctness property, not a style choice:
//!
//! 1. the `package/` payload is written and frozen;
//! 2. `artifact-manifest.json` describes that payload (and declares which
//!    members it excludes, and why);
//! 3. `release.json` / `release-bundle.json` pin the *declaration* digests and
//!    the source revision;
//! 4. `verification.json` is written last, from checks that re-read the files on
//!    disk.
//!
//! Nothing in step 1 hashes anything from steps 2–4, and no file hashes itself.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use business_application_compiler::{package_fingerprint, verify_release_identity};
use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, ContractDigest,
    CrateVersionBinding, ModuleReleaseBundle, ModuleReleaseIdentity, PackageDigest,
    MODULE_RELEASE_IDENTITY_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

use crate::canonical::{
    artifact_integrity_digest, inventory, normalize_text_bytes, sha256_hex, ArtifactFile,
};
use crate::migrations::{self, MigrationMapping};
use crate::repo::RepoContext;
use crate::spec::ReleaseSpec;
use crate::surface;
use crate::{
    ToolError, ToolResult, ARTIFACT_MANIFEST_FILE, ARTIFACT_MANIFEST_SCHEMA_VERSION,
    CHANGELOG_MEMBER, CONTRACT_SURFACE_MEMBER, MIGRATION_MAPPING_MEMBER, MODULE_MANIFEST_MEMBER,
    PACKAGES_MEMBER, PAYLOAD_DIRECTORY, RELEASE_BUNDLE_FILE, RELEASE_IDENTITY_FILE,
    VERIFICATION_FILE, VERIFICATION_INDEX_SCHEMA_VERSION,
};

/// A member deliberately outside the integrity digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExcludedMember {
    /// Path relative to the artifact root.
    pub path: String,
    /// Why it is outside the payload digest.
    pub reason: String,
}

/// One deterministic transformation applied while packing a crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagingTransform {
    /// Crate the transform applies to.
    pub crate_name: String,
    /// Rule identifier.
    pub rule: String,
    /// What was rewritten.
    pub detail: String,
}

/// `artifact-manifest.json`: the integrity record of the payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactManifest {
    /// Record schema identifier.
    pub schema_version: String,
    /// Module the artifact belongs to.
    pub module_id: String,
    /// Module release version of this candidate.
    pub module_version: String,
    /// Algorithm identifier of the payload digest.
    pub artifact_integrity_algorithm: String,
    /// Digest over the canonicalised payload.
    pub artifact_integrity_digest: String,
    /// Compiler contract digest this candidate pins.
    pub contract_digest: String,
    /// Compiled declaration fingerprint this candidate pins.
    pub package_digest: String,
    /// Payload members in canonical order.
    pub files: Vec<ArtifactFile>,
    /// Members outside the payload, each with its reason.
    pub excluded_members: Vec<ExcludedMember>,
    /// Every rewrite performed on a packed crate manifest.
    pub packaging_transforms: Vec<PackagingTransform>,
}

/// One verification outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationCheck {
    /// Stable check identifier.
    pub id: String,
    /// What the check compares.
    pub description: String,
    /// `PASS` only when the check actually ran and held.
    pub outcome: String,
    /// Observed values.
    pub detail: String,
}

/// `verification.json`: the evidence index of one candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationIndex {
    /// Record schema identifier.
    pub schema_version: String,
    /// Module of the verified artifact.
    pub module_id: String,
    /// Version of the verified artifact.
    pub module_version: String,
    /// Payload digest that was verified.
    pub artifact_integrity_digest: String,
    /// Checks, in execution order.
    pub checks: Vec<VerificationCheck>,
}

/// Result of one successful generation.
#[derive(Debug, Clone)]
pub struct GeneratedArtifact {
    /// Artifact directory.
    pub directory: PathBuf,
    /// Module id.
    pub module_id: String,
    /// Module version.
    pub module_version: String,
    /// Payload digest.
    pub artifact_integrity_digest: String,
    /// Compiler contract digest.
    pub contract_digest: String,
    /// Compiled declaration fingerprint.
    pub package_digest: String,
    /// Number of payload members.
    pub member_count: usize,
}

/// Assembles a candidate artifact into `out_root`.
///
/// Refuses to overwrite an existing artifact directory: evidence must not be
/// mutable in place.
///
/// Long by construction: the steps form one linear transaction (load spec,
/// validate, compile, digest, write payload, write records, verify), and
/// splitting them up would scatter the ordering rules that keep the hashed
/// payload frozen before any descriptive record is written next to it.
#[allow(clippy::too_many_lines)]
pub fn generate(
    context: &RepoContext,
    module: &str,
    out_root: &Path,
) -> ToolResult<GeneratedArtifact> {
    let spec = ReleaseSpec::load(context, module)?;
    let manifest = spec.manifest()?;
    manifest.validate()?;
    let package = business_application_compiler::BusinessApplicationPackage {
        manifest,
        contributions: business_module_contracts::TypedContributionSet::default(),
        extension_points: Vec::new(),
        extension_contributions: Vec::new(),
    };
    let compiled = surface::compile_single(package, context.platform_version())?;
    let compiled_package = compiled
        .packages
        .iter()
        .find(|candidate| candidate.manifest.module_id.as_str() == spec.module_id)
        .ok_or_else(|| ToolError::Verification {
            check: "compiled-package-present".to_owned(),
            reason: format!("'{}' is absent from the compiled evidence", spec.module_id),
        })?;

    let surface_bytes = surface::assert_surface_matches_digest(compiled_package)?;
    let contract_digest = business_application_compiler::contract_digest(compiled_package)?;
    let package_digest = package_fingerprint(compiled_package)?;
    let mapping = migrations::build_mapping(context, &spec)?;
    let identity = ModuleReleaseIdentity {
        schema_version: MODULE_RELEASE_IDENTITY_SCHEMA_VERSION.to_owned(),
        module_id: BusinessModuleId::new(spec.module_id.clone())?,
        module_version: BusinessModuleVersion::new(spec.module_version.clone())?,
        manifest_schema_version: compiled_package.manifest.manifest_schema_version.clone(),
        source_revision: context.source_revision()?,
        contract_digest: ContractDigest::new(contract_digest.to_string())?,
        package_digest: PackageDigest::new(package_digest.to_string())?,
        migration_revision: mapping.revision()?,
        build_toolchain: context.build_toolchain(),
    };
    identity.validate()?;
    verify_release_identity(&compiled, &identity)?;

    let bundle = ModuleReleaseBundle {
        identity: identity.clone(),
        crate_versions: packed_bindings(&spec),
    };
    bundle.validate()?;

    let directory = out_root.join(format!("{}-{}", spec.module_id, spec.module_version));
    if directory.exists() {
        return Err(ToolError::Usage(format!(
            "refusing to overwrite existing artifact '{}'; evidence is immutable, pick a new \
             output directory",
            directory.display()
        )));
    }
    let payload = directory.join(PAYLOAD_DIRECTORY);
    std::fs::create_dir_all(&payload).map_err(|source| ToolError::Io {
        path: payload.display().to_string(),
        source,
    })?;

    let mut transforms = Vec::new();
    let mut members: Vec<(String, Vec<u8>)> = vec![
        (
            MODULE_MANIFEST_MEMBER.to_owned(),
            serde_json::to_vec(&compiled_package.manifest)?,
        ),
        (CONTRACT_SURFACE_MEMBER.to_owned(), surface_bytes),
        (
            MIGRATION_MAPPING_MEMBER.to_owned(),
            serde_json::to_vec(&mapping)?,
        ),
        (
            CHANGELOG_MEMBER.to_owned(),
            spec.changelog(context)?.into_bytes(),
        ),
    ];
    for crate_spec in &spec.packed_crates {
        let prefix = format!("{PACKAGES_MEMBER}/{}", crate_spec.crate_name);
        members.push((
            format!("{prefix}/Cargo.toml"),
            pack_crate_manifest(context, crate_spec, &spec.module_version, &mut transforms)?,
        ));
        let mut sources = pack_crate_sources(context, crate_spec)?;
        members.append(&mut sources);
    }
    members.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    // Payload members are written in normalised form, so the bytes on disk are
    // exactly the bytes the integrity digest covers: two checkouts that differ
    // only in line-ending convention produce the same artifact, and a third
    // party can reproduce every member hash with a plain SHA-256 tool. This is
    // also where a non-UTF-8 payload member is refused.
    //
    // One member is exempt: `contract/contract-surface.json` *is* the canonical
    // contract pre-image, so it ships byte-for-byte as the compiler produced
    // it. Normalising it — appending the trailing newline, for instance — would
    // stop `sha256` of that file from equaling the pinned `contract_digest`,
    // which is the only reason the file is in the artifact at all.
    let mut canonical: Vec<(String, Vec<u8>)> = Vec::with_capacity(members.len());
    for (member, bytes) in members {
        let canonical_bytes = if member == CONTRACT_SURFACE_MEMBER {
            bytes
        } else {
            normalize_text_bytes(&bytes, &member).map_err(|error| match error {
                ToolError::NotUtf8 { path } => ToolError::Packaging {
                    crate_name: spec.module_id.clone(),
                    reason: format!("payload member '{path}' is not UTF-8 text"),
                },
                other => other,
            })?
        };
        canonical.push((member, canonical_bytes));
    }
    for (relative, bytes) in &canonical {
        write_file(&payload.join(relative), bytes)?;
    }

    let files = inventory(&payload)?;
    let artifact_integrity_digest = artifact_integrity_digest(&files);
    let artifact_manifest = ArtifactManifest {
        schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION.to_owned(),
        module_id: spec.module_id.clone(),
        module_version: spec.module_version.clone(),
        artifact_integrity_algorithm: crate::canonical::ARTIFACT_INTEGRITY_ALGORITHM.to_owned(),
        artifact_integrity_digest: artifact_integrity_digest.clone(),
        contract_digest: contract_digest.to_string(),
        package_digest: package_digest.to_string(),
        files,
        excluded_members: excluded_members(),
        packaging_transforms: transforms,
    };
    write_file(
        &directory.join(ARTIFACT_MANIFEST_FILE),
        &serde_json::to_vec_pretty(&artifact_manifest)?,
    )?;
    write_file(
        &directory.join(RELEASE_IDENTITY_FILE),
        &serde_json::to_vec_pretty(&identity)?,
    )?;
    write_file(
        &directory.join(RELEASE_BUNDLE_FILE),
        &serde_json::to_vec_pretty(&bundle)?,
    )?;

    let index = verify(context, &directory)?;
    write_file(
        &directory.join(VERIFICATION_FILE),
        &serde_json::to_vec_pretty(&index)?,
    )?;

    let member_count = artifact_manifest.files.len();
    Ok(GeneratedArtifact {
        directory,
        module_id: spec.module_id,
        module_version: spec.module_version,
        artifact_integrity_digest,
        contract_digest: contract_digest.to_string(),
        package_digest: package_digest.to_string(),
        member_count,
    })
}

/// Re-reads an artifact from disk and re-runs every check independently.
/// Re-checks a candidate in a fresh process, from what is on disk.
///
/// Long for the same reason as [`generate`]: every check must run against the
/// same freshly read state, and reordering them would weaken the guarantees.
#[allow(clippy::too_many_lines)]
pub fn verify(context: &RepoContext, artifact: &Path) -> ToolResult<VerificationIndex> {
    let artifact_manifest: ArtifactManifest = read_json(&artifact.join(ARTIFACT_MANIFEST_FILE))?;
    let identity: ModuleReleaseIdentity = read_json(&artifact.join(RELEASE_IDENTITY_FILE))?;
    let bundle: ModuleReleaseBundle = read_json(&artifact.join(RELEASE_BUNDLE_FILE))?;
    let manifest: BusinessModuleManifest =
        read_json(&payload_path(artifact, MODULE_MANIFEST_MEMBER))?;
    let mapping: MigrationMapping = read_json(&payload_path(artifact, MIGRATION_MAPPING_MEMBER))?;
    let spec = ReleaseSpec::load(context, identity.module_id.as_str())?;

    identity.validate()?;
    bundle.validate()?;
    if bundle.identity != identity {
        return Err(ToolError::Verification {
            check: "bundle-identity".to_owned(),
            reason: "release-bundle.json pins a different identity than release.json".to_owned(),
        });
    }

    let mut checks = Vec::new();

    // 1 — payload bytes.
    let files = inventory(&artifact.join(PAYLOAD_DIRECTORY))?;
    let recomputed = artifact_integrity_digest(&files);
    if recomputed != artifact_manifest.artifact_integrity_digest {
        return Err(ToolError::Verification {
            check: "artifact-integrity-digest".to_owned(),
            reason: describe_inventory_difference(
                &artifact_manifest.files,
                &files,
                &recomputed,
                &artifact_manifest.artifact_integrity_digest,
            ),
        });
    }
    if files != artifact_manifest.files {
        return Err(ToolError::Verification {
            check: "artifact-inventory".to_owned(),
            reason: "the payload inventory differs from the record in \
                     artifact-manifest.json"
                .to_owned(),
        });
    }
    checks.push(VerificationCheck {
        id: "artifact-integrity-digest".to_owned(),
        description: "SHA-256 over the canonicalised members of package/ recomputed from disk"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "{} members, {} = {recomputed}",
            files.len(),
            artifact_manifest.artifact_integrity_algorithm
        ),
    });

    // 2 — contract surface file hashes to the pinned contract digest.
    let surface_disk = read_bytes(&payload_path(artifact, CONTRACT_SURFACE_MEMBER))?;
    let surface_hash = sha256_hex(&surface_disk);
    if surface_hash != identity.contract_digest.to_string() {
        return Err(ToolError::Verification {
            check: "contract-surface-file-digest".to_owned(),
            reason: format!(
                "package/{} hashes to '{surface_hash}' but release.json pins contract_digest \
                 '{contract}'",
                CONTRACT_SURFACE_MEMBER,
                contract = identity.contract_digest
            ),
        });
    }
    checks.push(VerificationCheck {
        id: "contract-surface-file-digest".to_owned(),
        description: "sha256(package/contract/contract-surface.json) equals the pinned contract \
                      digest, so any third-party tool can reproduce contract_digest"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: surface_hash,
    });

    // 3 — the shipped manifest compiles and re-hashes to the pinned digests.
    let package = business_application_compiler::BusinessApplicationPackage {
        manifest: manifest.clone(),
        contributions: business_module_contracts::TypedContributionSet::default(),
        extension_points: Vec::new(),
        extension_contributions: Vec::new(),
    };
    let compiled = surface::compile_single(package, context.platform_version())?;
    verify_release_identity(&compiled, &identity)?;
    let compiled_package = compiled
        .packages
        .iter()
        .find(|candidate| candidate.manifest.module_id == identity.module_id)
        .ok_or_else(|| ToolError::Verification {
            check: "compiled-package-present".to_owned(),
            reason: format!(
                "'{}' is absent from the compiled evidence",
                identity.module_id
            ),
        })?;
    surface::assert_surface_matches_digest(compiled_package)?;
    checks.push(VerificationCheck {
        id: "release-identity-consistency".to_owned(),
        description: "compiler verify_release_identity over the re-compiled shipped manifest \
                      (module id, module version, manifest schema, contract digest, package \
                      digest, migration namespace)"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "contract_digest={} package_digest={} namespace={}",
            identity.contract_digest,
            identity.package_digest,
            identity
                .migration_revision
                .as_ref()
                .map_or("none".to_owned(), |revision| {
                    revision.migration_namespace.clone()
                })
        ),
    });

    // 4 — migration mapping still matches the formal catalog.
    if let Some(revision) = &identity.migration_revision {
        if revision.set_digest.to_string() != mapping.set_digest {
            return Err(ToolError::Verification {
                check: "migration-set-digest".to_owned(),
                reason: format!(
                    "release.json pins migration set digest '{}' but the shipped mapping \
                     computes '{}'",
                    revision.set_digest, mapping.set_digest
                ),
            });
        }
    } else if mapping.persistent {
        return Err(ToolError::Verification {
            check: "migration-set-digest".to_owned(),
            reason: "the shipped mapping declares persistent facts but the identity pins no \
                     migration revision"
                .to_owned(),
        });
    }
    migrations::reverify(context, &spec, &mapping)?;
    checks.push(VerificationCheck {
        id: "migration-mapping-matches-formal-catalog".to_owned(),
        description: "every pinned migration file is re-read from its formal catalog and must \
                      match that catalog's MANIFEST.sha256 and its bytes on disk"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "namespace={} set_digest={} files={} assessments={}",
            mapping.migration_namespace,
            mapping.set_digest,
            mapping
                .files
                .iter()
                .map(|file| format!("{}:{}", file.backend, file.file_name))
                .collect::<Vec<_>>()
                .join(","),
            mapping
                .assessments
                .iter()
                .map(|assessment| format!("{}={}", assessment.backend, assessment.status))
                .collect::<Vec<_>>()
                .join(",")
        ),
    });

    // 5 — provenance.
    checks.push(VerificationCheck {
        id: "source-provenance".to_owned(),
        description: "the artifact pins the source revision and toolchain it was built from"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "source_revision={} toolchain={} {}",
            identity.source_revision,
            identity.build_toolchain.toolchain_id,
            identity.build_toolchain.version
        ),
    });

    Ok(VerificationIndex {
        schema_version: VERIFICATION_INDEX_SCHEMA_VERSION.to_owned(),
        module_id: identity.module_id.to_string(),
        module_version: identity.module_version.to_string(),
        artifact_integrity_digest: artifact_manifest.artifact_integrity_digest,
        checks,
    })
}

/// Difference summary between two recorded inventories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// Left artifact directory.
    pub left: PathBuf,
    /// Right artifact directory.
    pub right: PathBuf,
    /// Whether the two candidates are identical.
    pub identical: bool,
    /// Human-readable per-record differences.
    pub differences: Vec<String>,
    /// Digest of the left payload.
    pub left_digest: String,
    /// Digest of the right payload.
    pub right_digest: String,
}

/// Compares two independently generated candidates.
pub fn compare(left: &Path, right: &Path) -> ToolResult<Comparison> {
    let left_manifest: ArtifactManifest = read_json(&left.join(ARTIFACT_MANIFEST_FILE))?;
    let right_manifest: ArtifactManifest = read_json(&right.join(ARTIFACT_MANIFEST_FILE))?;
    let mut differences = Vec::new();
    for (field, before, after) in [
        (
            "artifact_integrity_digest",
            left_manifest.artifact_integrity_digest.clone(),
            right_manifest.artifact_integrity_digest.clone(),
        ),
        (
            "contract_digest",
            left_manifest.contract_digest.clone(),
            right_manifest.contract_digest.clone(),
        ),
        (
            "package_digest",
            left_manifest.package_digest.clone(),
            right_manifest.package_digest.clone(),
        ),
        (
            "module_id",
            left_manifest.module_id.clone(),
            right_manifest.module_id.clone(),
        ),
        (
            "module_version",
            left_manifest.module_version.clone(),
            right_manifest.module_version.clone(),
        ),
    ] {
        if before != after {
            differences.push(format!("{field}: '{before}' != '{after}'"));
        }
    }
    for entry in diff_entries(&left_manifest.files, &right_manifest.files) {
        differences.push(entry);
    }
    if left_manifest.packaging_transforms != right_manifest.packaging_transforms {
        differences.push("packaging_transforms differ".to_owned());
    }
    Ok(Comparison {
        left: left.to_path_buf(),
        right: right.to_path_buf(),
        identical: differences.is_empty(),
        differences,
        left_digest: left_manifest.artifact_integrity_digest,
        right_digest: right_manifest.artifact_integrity_digest,
    })
}

fn diff_entries(left: &[ArtifactFile], right: &[ArtifactFile]) -> Vec<String> {
    let mut differences = Vec::new();
    for file in left {
        match right.iter().find(|candidate| candidate.path == file.path) {
            Some(other) if other == file => {}
            Some(other) => differences.push(format!(
                "member '{}': '{}' ({} bytes) != '{}' ({} bytes)",
                file.path, file.sha256, file.bytes, other.sha256, other.bytes
            )),
            None => differences.push(format!(
                "member '{}' is absent from the right artifact",
                file.path
            )),
        }
    }
    for file in right {
        if !left.iter().any(|candidate| candidate.path == file.path) {
            differences.push(format!(
                "member '{}' is absent from the left artifact",
                file.path
            ));
        }
    }
    differences
}

fn describe_inventory_difference(
    recorded: &[ArtifactFile],
    observed: &[ArtifactFile],
    recomputed: &str,
    expected: &str,
) -> String {
    let mut details =
        format!("recomputed '{recomputed}' but artifact-manifest.json pins '{expected}'");
    let added = diff_entries(recorded, observed);
    if !added.is_empty() {
        details.push_str("; member differences: ");
        details.push_str(&added.join(" | "));
    }
    if added.is_empty() {
        if let Some(pair) = recorded
            .iter()
            .zip(observed.iter())
            .find(|(before, after)| before != after)
        {
            let _ = write!(
                details,
                "; first differing member '{}' is recorded as '{}' and observed as '{}'",
                pair.0.path, pair.0.sha256, pair.1.sha256
            );
        }
    }
    details
}

fn packed_bindings(spec: &ReleaseSpec) -> Vec<CrateVersionBinding> {
    spec.packed_crates
        .iter()
        .map(|crate_spec| CrateVersionBinding {
            crate_name: crate_spec.crate_name.clone(),
            crate_version: spec.module_version.clone(),
        })
        .collect()
}

fn excluded_members() -> Vec<ExcludedMember> {
    vec![
        ExcludedMember {
            path: ARTIFACT_MANIFEST_FILE.to_owned(),
            reason: "descriptive record of the payload; hashing the record that contains the \
                     payload digest would make the digest self-referential"
                .to_owned(),
        },
        ExcludedMember {
            path: RELEASE_IDENTITY_FILE.to_owned(),
            reason: "external release identity record; it pins declaration digests and the \
                     source revision, not payload bytes"
                .to_owned(),
        },
        ExcludedMember {
            path: RELEASE_BUNDLE_FILE.to_owned(),
            reason: "audit mapping from module version to crate versions; derived from the same \
                     declaration digests"
                .to_owned(),
        },
        ExcludedMember {
            path: VERIFICATION_FILE.to_owned(),
            reason: "evidence index written after verification; excluding it keeps the payload \
                     digest stable across re-verification"
                .to_owned(),
        },
    ]
}

fn pack_crate_manifest(
    context: &RepoContext,
    crate_spec: &crate::spec::PackedCrateSpec,
    module_version: &str,
    transforms: &mut Vec<PackagingTransform>,
) -> ToolResult<Vec<u8>> {
    let path = context
        .root()
        .join(&crate_spec.source_dir)
        .join("Cargo.toml");
    let shown = path.display().to_string();
    let raw = std::fs::read_to_string(&path).map_err(|source| ToolError::Io {
        path: shown,
        source,
    })?;
    let packed = rewrite_crate_manifest(context, crate_spec, module_version, &raw, transforms)?;
    Ok(packed.into_bytes())
}

fn rewrite_crate_manifest(
    context: &RepoContext,
    crate_spec: &crate::spec::PackedCrateSpec,
    module_version: &str,
    raw: &str,
    transforms: &mut Vec<PackagingTransform>,
) -> ToolResult<String> {
    let mut output = String::new();
    let mut section = String::new();
    let mut dropped = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            header.clone_into(&mut section);
            if is_dropped_section(header) {
                dropped.push(header.to_owned());
                continue;
            }
        }
        if dropped.iter().any(|dropped| section.eq(dropped)) {
            continue;
        }
        let Some((key, _)) = split_assignment(trimmed) else {
            output.push_str(line);
            output.push('\n');
            continue;
        };
        // `version.workspace = true` declares the same thing as
        // `version = { workspace = true }` written differently, so the base key
        // is what both the rewrite and the recorded transform are keyed on.
        let key = key.trim().trim_matches('"').trim().to_owned();
        let base_key = key
            .strip_suffix(".workspace")
            .unwrap_or(key.as_str())
            .to_owned();
        if is_dependency_section(&section) {
            output.push_str(&rewrite_dependency_line(
                context,
                crate_spec,
                base_key.as_str(),
                trimmed,
                transforms,
            )?);
            output.push('\n');
            continue;
        }
        let replacement = match base_key.as_str() {
            "version" if trimmed.contains("workspace") => {
                Some(format!("version = \"{module_version}\""))
            }
            "edition" if trimmed.contains("workspace") => {
                Some(format!("edition = \"{}\"", context.edition()))
            }
            "rust-version" if trimmed.contains("workspace") => {
                Some(format!("rust-version = \"{}\"", context.rust_version()))
            }
            "license" if trimmed.contains("workspace") => {
                Some(format!("license = \"{}\"", context.license()))
            }
            _ => None,
        };
        if let Some(replacement) = &replacement {
            transforms.push(PackagingTransform {
                crate_name: crate_spec.crate_name.clone(),
                rule: format!("materialise-{base_key}-from-workspace"),
                detail: format!("{trimmed} -> {replacement}"),
            });
            output.push_str(replacement);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    for header in dropped {
        transforms.push(PackagingTransform {
            crate_name: crate_spec.crate_name.clone(),
            rule: "drop-section".to_owned(),
            detail: format!("[{header}] is not part of the published contract surface"),
        });
    }
    if output.contains("workspace = true") || output.contains(".workspace = true") {
        return Err(ToolError::Packaging {
            crate_name: crate_spec.crate_name.clone(),
            reason: "the packed manifest still inherits from the workspace; refusing to guess a \
                     value"
                .to_owned(),
        });
    }
    // The artifact must build outside this workspace; an explicit empty
    // `[workspace]` table detaches it from any enclosing workspace instead of
    // relying on where the artifact happens to be unpacked.
    transforms.push(PackagingTransform {
        crate_name: crate_spec.crate_name.clone(),
        rule: "detach-workspace".to_owned(),
        detail: "append an empty [workspace] table so the packed crate builds identically \
                 inside and outside this repository"
            .to_owned(),
    });
    output.push_str("\n[workspace]\n");
    Ok(output)
}

/// Rewrites one dependency line of a packed crate manifest.
///
/// A workspace-inherited dependency is **materialised from the workspace
/// entry**, not from the member's `workspace = true` line: the entry may carry
/// a `package` rename, `default-features = false` or features, and the member
/// may add features of its own. Dropping any of that while pinning the version
/// would ship a crate whose dependency is not the one this workspace builds.
/// The version is the only thing rewritten — to the exact `Cargo.lock` version.
/// Dependencies that do not inherit from the workspace are passed through.
fn rewrite_dependency_line(
    context: &RepoContext,
    crate_spec: &crate::spec::PackedCrateSpec,
    key: &str,
    line: &str,
    transforms: &mut Vec<PackagingTransform>,
) -> ToolResult<String> {
    let fails = |reason: String| ToolError::Packaging {
        crate_name: crate_spec.crate_name.clone(),
        reason,
    };
    let (_, right) = split_assignment(line)
        .ok_or_else(|| fails(format!("cannot parse dependency line '{line}'")))?;
    if declares_path(right) {
        return Err(fails(format!(
            "dependency '{key}' is a path dependency; a published artifact must not point back \
             into the source workspace"
        )));
    }
    if !right.contains("workspace") {
        return Ok(line.to_owned());
    }
    let entry = context.workspace_dependency_entry(key)?;
    if declares_path(&entry) {
        return Err(fails(format!(
            "the workspace declares '{key}' as a path dependency, which a released crate cannot \
             inherit"
        )));
    }
    let version = context.locked_version(key)?;
    let mut features = array_value(&entry, "features");
    for feature in array_value(right, "features") {
        if !features.contains(&feature) {
            features.push(feature);
        }
    }
    features.sort();
    let mut parts = vec![format!("version = \"={version}\"")];
    for part in entry_parts(&entry) {
        if matches!(
            part_name(&part).as_str(),
            "version" | "workspace" | "features"
        ) {
            continue;
        }
        parts.push(part);
    }
    if !features.is_empty() {
        let items: Vec<String> = features
            .iter()
            .map(|feature| format!("\"{feature}\""))
            .collect();
        parts.push(format!("features = [{}]", items.join(", ")));
    }
    let rewritten = format!("{key} = {{ {} }}", parts.join(", "));
    transforms.push(PackagingTransform {
        crate_name: crate_spec.crate_name.clone(),
        rule: "materialise-workspace-dependency".to_owned(),
        detail: format!(
            "{line} together with the workspace entry '{key} = {entry}' -> {rewritten}; the \
             version is pinned to Cargo.lock"
        ),
    });
    Ok(rewritten)
}

/// Top-level `key = value` parts of an inline dependency table.
fn entry_parts(entry: &str) -> Vec<String> {
    entry
        .trim()
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .map_or_else(Vec::new, split_top_level)
}

/// The name side of a `name = value` part.
fn part_name(part: &str) -> String {
    part.split_once('=').map_or_else(
        || part.trim().trim_matches('"').to_owned(),
        |(name, _)| name.trim().trim_matches('"').to_owned(),
    )
}

/// Reads a string array such as `features = ["a", "b"]` out of a table.
fn array_value(entry: &str, wanted: &str) -> Vec<String> {
    for part in entry_parts(entry) {
        if part_name(&part) != wanted {
            continue;
        }
        let Some((_, value)) = split_assignment(&part) else {
            continue;
        };
        let Some(body) = value
            .trim()
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        else {
            continue;
        };
        return split_top_level(body)
            .into_iter()
            .map(|item| item.trim().trim_matches('"').to_owned())
            .filter(|item| !item.is_empty())
            .collect();
    }
    Vec::new()
}

/// Whether a dependency right-hand side declares a local `path`.
fn declares_path(right: &str) -> bool {
    entry_parts(right)
        .iter()
        .any(|part| part_name(part) == "path")
        || right.trim().starts_with("{ path")
}

fn is_dependency_section(section: &str) -> bool {
    section == "dependencies"
        || section == "build-dependencies"
        || section.ends_with(".dependencies")
}

fn is_dropped_section(section: &str) -> bool {
    section == "dev-dependencies"
        || section == "lints"
        || section.ends_with(".dev-dependencies")
        || section == "workspace"
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
        return None;
    }
    line.split_once('=')
}

/// Splits an inline-table body on top-level commas only.
fn split_top_level(body: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut current = String::new();
    for character in body.chars() {
        match character {
            '"' => {
                in_string = !in_string;
                current.push(character);
            }
            '[' | '{' if !in_string => {
                depth += 1;
                current.push(character);
            }
            ']' | '}' if !in_string => {
                depth -= 1;
                current.push(character);
            }
            ',' if !in_string && depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_owned());
                }
                current = String::new();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_owned());
    }
    parts
}

fn pack_crate_sources(
    context: &RepoContext,
    crate_spec: &crate::spec::PackedCrateSpec,
) -> ToolResult<Vec<(String, Vec<u8>)>> {
    let root = context.root().join(&crate_spec.source_dir).join("src");
    let mut members = Vec::new();
    collect_sources(crate_spec, &root, &root, &mut members)?;
    members.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    if members
        .iter()
        .all(|(path, _)| !path.ends_with("/lib.rs") && !path.ends_with("lib.rs"))
    {
        return Err(ToolError::Packaging {
            crate_name: crate_spec.crate_name.clone(),
            reason: format!(
                "'{}' contains no lib.rs, so there is no published crate to pack",
                root.display()
            ),
        });
    }
    Ok(members)
}

fn collect_sources(
    crate_spec: &crate::spec::PackedCrateSpec,
    root: &Path,
    directory: &Path,
    members: &mut Vec<(String, Vec<u8>)>,
) -> ToolResult<()> {
    let shown = directory.display().to_string();
    let read = std::fs::read_dir(directory).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    let mut paths: Vec<PathBuf> = read
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .collect();
    paths.sort();
    for path in paths {
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ToolError::UnsupportedEntry {
                path: path.display().to_string(),
            });
        }
        if metadata.is_dir() {
            collect_sources(crate_spec, root, &path, members)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(ToolError::UnsupportedEntry {
                path: path.display().to_string(),
            });
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ToolError::InvalidMemberPath {
                path: path.display().to_string(),
            })?;
        let relative = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let bytes = std::fs::read(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        members.push((
            // `root` is the crate's `src` directory, and the packed crate has to
            // look like the crate it came from, so the `src/` level is kept.
            format!("{PACKAGES_MEMBER}/{}/src/{relative}", crate_spec.crate_name),
            bytes,
        ));
    }
    Ok(())
}

fn payload_path(artifact: &Path, member: &str) -> PathBuf {
    let mut path = artifact.join(PAYLOAD_DIRECTORY);
    for segment in member.split('/') {
        path = path.join(segment);
    }
    path
}

fn read_bytes(path: &Path) -> ToolResult<Vec<u8>> {
    let shown = path.display().to_string();
    std::fs::read(path).map_err(|source| ToolError::Io {
        path: shown,
        source,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> ToolResult<T> {
    let shown = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ToolError::Verification {
        check: "read-record".to_owned(),
        reason: format!("'{shown}' is not a readable record: {source}"),
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> ToolResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ToolError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    std::fs::write(path, bytes).map_err(|source| ToolError::Io {
        path: path.display().to_string(),
        source,
    })
}
