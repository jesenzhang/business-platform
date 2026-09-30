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

use std::collections::{BTreeMap, BTreeSet};
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
    artifact_integrity_digest, inventory, normalize_text_bytes, sha256_hex, validate_member_path,
    validate_sha256_hex, ArtifactFile,
};
use crate::commit::Inputs;
use crate::migrations::{self, MigrationMapping};
use crate::provenance::{ProvenanceInput, CHECK_DERIVED, CHECK_MEMBERS};
use crate::repo::RepoContext;
use crate::spec::{self, ReleaseSpec};
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
    // The candidate pins the commit Git resolves `HEAD` to, so from here every input
    // is read out of *that commit* and not out of this checkout: the release spec, the
    // crate manifests before packing, the workspace manifest, the lock file, the
    // toolchain declaration, the crate sources and the formal migration catalogs.
    // Nothing on disk contributes a byte to a candidate (review F1-A).
    let pinned = Inputs::open_commit(context.root(), context.revision())?;
    let projection = project(&pinned, context.root(), module)?;
    // The checkout still has to agree with the commit it is releasing: a modified,
    // deleted or extra input means the bytes about to be released are not the bytes
    // that revision contains. This runs while no output directory exists yet, so a
    // refused generation leaves no candidate behind (review F1-B).
    assert_checkout_matches_commit(context, &projection)?;
    let spec = &projection.spec;
    let identity = projection.identity.clone();
    let bundle = projection.bundle.clone();
    let contract_digest = projection.contract_digest.clone();
    let package_digest = projection.package_digest.clone();

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

    // The bytes written here are the bytes the projection derived from the pinned
    // commit — and `verify` below recomputes that same projection from the same
    // commit and compares, so writing them is not how they get validated.
    for (member, bytes) in &projection.members {
        write_file(&payload.join(member), bytes)?;
    }

    let files = inventory(&payload)?;
    let artifact_integrity_digest = artifact_integrity_digest(&files);
    let artifact_manifest = ArtifactManifest {
        schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION.to_owned(),
        module_id: spec.module_id.clone(),
        module_version: spec.module_version.clone(),
        artifact_integrity_algorithm: crate::canonical::ARTIFACT_INTEGRITY_ALGORITHM.to_owned(),
        artifact_integrity_digest: artifact_integrity_digest.clone(),
        contract_digest: contract_digest.clone(),
        package_digest: package_digest.clone(),
        files,
        excluded_members: excluded_members(),
        packaging_transforms: projection.transforms.clone(),
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
        module_id: spec.module_id.clone(),
        module_version: spec.module_version.clone(),
        artifact_integrity_digest,
        contract_digest,
        package_digest,
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
    // Every input is read out of the commit this artifact pins — the release spec,
    // the workspace manifest, the lock file, the toolchain declaration, the crate
    // manifests, the crate sources and the formal migration catalogs — before the
    // first check runs. Verifying a candidate therefore never depends on what the
    // current checkout happens to contain, and never falls back to it: a commit that
    // is absent, or that does not contain one of its own inputs, is refused here.
    let pinned = Inputs::open_commit(context.root(), identity.source_revision.as_str())?;
    let projection = project(&pinned, context.root(), identity.module_id.as_str())?;
    let spec = &projection.spec;

    identity.validate()?;
    bundle.validate()?;
    if bundle.identity != identity {
        return Err(ToolError::Verification {
            check: "bundle-identity".to_owned(),
            reason: "release-bundle.json pins a different identity than release.json".to_owned(),
        });
    }

    // 0 — the record must be in a format this tool knows how to read, and every
    // field it uses has to be present and well formed. Nothing below trusts a
    // field this step has not accepted.
    validate_artifact_record(&artifact_manifest)?;

    let mut checks = Vec::new();
    checks.push(VerificationCheck {
        id: "artifact-record-format".to_owned(),
        description: "artifact-manifest.json is in a schema this tool implements, names an \
                      integrity algorithm it can reproduce, lists only valid member paths and \
                      SHA-256 digests, and declares every descriptive record it leaves outside \
                      the digest together with a reason"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "schema={} algorithm={} members={} excluded={}",
            artifact_manifest.schema_version,
            artifact_manifest.artifact_integrity_algorithm,
            artifact_manifest.files.len(),
            artifact_manifest.excluded_members.len()
        ),
    });

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
    // Re-compiled with the platform coordinate the *pinned commit* declares, not the
    // one the current checkout happens to state.
    let compiled = surface::compile_single(package, projection.context.platform_version())?;
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

    // 3b — the descriptive records describe one release, and the bundle describes
    // the crate manifests that are actually in the payload.
    assert_records_agree(&artifact_manifest, &identity, &manifest, &mapping)?;
    checks.push(VerificationCheck {
        id: "record-consistency".to_owned(),
        description: "artifact-manifest.json, release.json, the shipped module.manifest.json and \
                      the shipped migration mapping agree on module_id, module_version, manifest \
                      schema, contract digest, package digest and migration namespace"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "module_id={} module_version={} contract_digest={} package_digest={}",
            identity.module_id,
            identity.module_version,
            identity.contract_digest,
            identity.package_digest
        ),
    });
    let bindings = assert_packed_bindings(&bundle, artifact, &files)?;
    checks.push(VerificationCheck {
        id: "packed-crate-bindings".to_owned(),
        description: "release-bundle.json's crate_versions matches the [package] name and version \
                      of every Cargo.toml in the payload: a missing, extra, duplicated or renamed \
                      crate, or a version that differs from the packed manifest, is refused"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: bindings,
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
    migrations::reverify_at(&pinned, spec, &mapping)?;
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

    // 5 — provenance. Three separate claims, three separate identifiers, because a
    // report that says only "provenance failed" cannot say what was wrong:
    //
    //   * `source-member-set-matches-revision` — the payload holds exactly the
    //     members the pinned commit's tree produces, neither more nor fewer;
    //   * `source-inputs-match-revision` — the members that are copies equal the
    //     blobs at those paths in that commit;
    //   * `derived-members-match-revision` — the members that are *not* copies are
    //     what re-running the derivation on that commit produces, in full.
    //
    // The records beside the payload (`release.json`, `release-bundle.json`) are bound
    // through those members rather than compared with the commit directly: they have
    // to agree with the declaration shipped as `module.manifest.json`, and that member
    // is now proven to be the commit's rather than the artifact's own claim. Binding a
    // record to a re-derivation it was itself used to produce would be the circular
    // proof this whole section replaces.
    //
    // The commit itself was resolved, and every input read out of it, before the
    // first byte of this artifact was judged; `source-revision-exists` records that
    // here so the index says what it actually did.
    checks.push(VerificationCheck {
        id: crate::provenance::CHECK_REVISION_EXISTS.to_owned(),
        description: "release.json's source_revision names a commit object in this repository, \
                      resolved by git rather than by reading .git/HEAD, refs and packed-refs by \
                      hand, so a detached HEAD, a linked worktree and a shallow clone all mean \
                      the same commit here that git means. Every input below is read out of that \
                      commit, never out of the current checkout"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "source_revision={} resolves; HEAD currently resolves to {}",
            identity.source_revision,
            context.revision()
        ),
    });
    let member_set = assert_member_set_matches(&files, &projection)?;
    checks.push(VerificationCheck {
        id: crate::provenance::CHECK_MEMBERS.to_owned(),
        description: "the payload contains exactly the members the pinned commit produces: the \
                      fixed records, one packed Cargo.toml per crate named in the commit's \
                      release spec, and every `.rs` file under each packed crate's `src` as \
                      `git ls-tree -r -z` lists them. A member the commit has and the artifact \
                      lacks, or one the artifact has and the commit does not, is refused even \
                      when the payload matches its own integrity record perfectly"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: member_set,
    });
    let verbatim_inputs: Vec<ProvenanceInput> = projection
        .verbatim
        .iter()
        .map(|(repo_path, member)| {
            let path = payload_path(artifact, member);
            if path.is_file() {
                read_bytes(&path)
                    .map(|bytes| ProvenanceInput::from_member(repo_path.clone(), member, bytes))
            } else {
                // Unreachable after the member-set check above; kept because a
                // silent `unwrap_or_default` here would compare nothing.
                Err(ToolError::Verification {
                    check: crate::provenance::CHECK_MEMBERS.to_owned(),
                    reason: format!("'{member}' is declared as a verbatim member and has no byte"),
                })
            }
        })
        .collect::<ToolResult<Vec<_>>>()?;
    let bound_payload = pinned.compare("payload member", &verbatim_inputs)?;
    checks.push(VerificationCheck {
        id: crate::provenance::CHECK_INPUTS.to_owned(),
        description: "every payload member that copies a repository file verbatim — the changelog \
                      and every source file of every packed crate — equals the blob stored at \
                      that path in the pinned commit, so neither a modified nor an invented \
                      source file can survive into a candidate"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "{bound_payload} payload member(s) compared blob-for-blob with commit '{}'; the {} \
             derived member(s) are compared by the '{}' check below, because they are computed \
             rather than copied",
            identity.source_revision,
            projection.derived.len(),
            crate::provenance::CHECK_DERIVED
        ),
    });
    let derived = assert_derived_members_match(artifact, &projection, &artifact_manifest)?;
    checks.push(VerificationCheck {
        id: crate::provenance::CHECK_DERIVED.to_owned(),
        description: "each derived member — the packed Cargo.toml of every crate, the shipped \
                      module.manifest.json, the contract surface and the migration mapping — is \
                      rebuilt from the pinned commit (its release spec, workspace manifest, \
                      Cargo.lock, crate manifests, rust-toolchain.toml and formal migration \
                      catalogs) and compared in full, together with the recorded packaging \
                      transforms. Re-compiling the declaration the artifact ships is not proof \
                      of where it came from, so this compares against the commit instead"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: derived,
    });
    let declared_toolchain = assert_declared_toolchain(&pinned, &identity)?;
    checks.push(VerificationCheck {
        id: crate::provenance::CHECK_TOOLCHAIN.to_owned(),
        description: "DECLARATION CHECK ONLY — release.json's build_toolchain states the channel \
                      that rust-toolchain.toml declares at the pinned commit. It does not, and \
                      cannot, prove which compiler build produced these bytes; that evidence is \
                      the rustc -vV line recorded in the run log outside the artifact"
            .to_owned(),
        outcome: "PASS".to_owned(),
        detail: format!(
            "declared channel '{declared_toolchain}'; identity toolchain_id={} derives from \
             [workspace.package] rust-version, whose file is covered by the input comparison",
            identity.build_toolchain.toolchain_id
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
    /// Digest of the left payload, recomputed from its bytes.
    pub left_digest: String,
    /// Digest of the right payload, recomputed from its bytes.
    pub right_digest: String,
    /// What was actually compared, so a report can never overstate its reach.
    pub scope: String,
}

/// Reads one side of a comparison and checks it against its own record first.
///
/// A comparison is only meaningful if each side means what it says. So the
/// payload of each side is re-listed and re-hashed from the bytes on disk and
/// verified against *that side's own* manifest before anything is compared: a
/// candidate whose record no longer describes its payload is refused as
/// inconsistent, not reported as "these two builds differ".
fn check_side(side: &str, directory: &Path) -> ToolResult<CheckedSide> {
    let manifest: ArtifactManifest = read_json(&directory.join(ARTIFACT_MANIFEST_FILE))?;
    validate_artifact_record(&manifest).map_err(|error| ToolError::Verification {
        check: "compare-payload-integrity".to_owned(),
        reason: format!("the {side} artifact fails its own record check: {error}"),
    })?;
    let files =
        inventory(&directory.join(PAYLOAD_DIRECTORY)).map_err(|error| ToolError::Verification {
            check: "compare-payload-integrity".to_owned(),
            reason: format!("the {side} artifact's payload cannot be read: {error}"),
        })?;
    let digest = artifact_integrity_digest(&files);
    if digest != manifest.artifact_integrity_digest {
        return Err(ToolError::Verification {
            check: "compare-payload-integrity".to_owned(),
            reason: format!(
                "the {side} artifact's payload bytes are not what its own record claims: {}",
                describe_inventory_difference(
                    &manifest.files,
                    &files,
                    &digest,
                    &manifest.artifact_integrity_digest,
                )
            ),
        });
    }
    if files != manifest.files {
        return Err(ToolError::Verification {
            check: "compare-payload-integrity".to_owned(),
            reason: format!(
                "the {side} artifact's payload inventory differs from the listing in its own \
                 artifact-manifest.json, even though the aggregate digest matched"
            ),
        });
    }
    Ok(CheckedSide {
        directory: directory.to_path_buf(),
        manifest,
        files,
        digest,
    })
}

/// One side of a comparison, re-read from disk and self-verified.
struct CheckedSide {
    /// Where it was read from.
    directory: PathBuf,
    /// The descriptive record it ships.
    manifest: ArtifactManifest,
    /// Its payload inventory, recomputed from the bytes on disk.
    files: Vec<ArtifactFile>,
    /// Its integrity digest, recomputed from the bytes on disk.
    digest: String,
}

/// Compares two independently generated candidates.
///
/// Both sides' payload bytes are read and verified against their own records
/// first; only then are the recomputed digests, the recomputed inventories and
/// the descriptive records compared. The scope of the comparison is stated in
/// [`Comparison::scope`]: it covers the packed bytes and the declaration digests,
/// and it deliberately does not compare `verification.json` (an evidence index
/// written after the fact) or the `source_revision` claim (a provenance question
/// that [`verify`] answers against the commit itself).
pub fn compare(left: &Path, right: &Path) -> ToolResult<Comparison> {
    let left = check_side("left", left)?;
    let right = check_side("right", right)?;
    let mut differences = Vec::new();
    for (field, before, after) in [
        (
            "artifact_integrity_digest (recomputed from payload bytes)",
            left.digest.clone(),
            right.digest.clone(),
        ),
        (
            "contract_digest",
            left.manifest.contract_digest.clone(),
            right.manifest.contract_digest.clone(),
        ),
        (
            "package_digest",
            left.manifest.package_digest.clone(),
            right.manifest.package_digest.clone(),
        ),
        (
            "module_id",
            left.manifest.module_id.clone(),
            right.manifest.module_id.clone(),
        ),
        (
            "module_version",
            left.manifest.module_version.clone(),
            right.manifest.module_version.clone(),
        ),
    ] {
        if before != after {
            differences.push(format!("{field}: '{before}' != '{after}'"));
        }
    }
    for entry in diff_entries(&left.files, &right.files) {
        differences.push(format!("payload bytes — {entry}"));
    }
    if left.manifest.packaging_transforms != right.manifest.packaging_transforms {
        differences.push("packaging_transforms differ".to_owned());
    }
    if left.manifest.excluded_members != right.manifest.excluded_members {
        differences.push("excluded_members differ".to_owned());
    }
    Ok(Comparison {
        left: left.directory,
        right: right.directory,
        identical: differences.is_empty(),
        differences,
        left_digest: left.digest,
        right_digest: right.digest,
        scope: format!(
            "{} payload member(s) re-read from both sides and re-hashed, plus the contract, \
             package and integrity digests and the packaging transforms each side records; \
             verification.json and the source_revision claim are out of scope",
            left.files.len()
        ),
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

/// The packed `Cargo.toml` of one crate, rewritten from the manifest as one set of
/// inputs stores it — for a candidate, from the blob the pinned commit holds.
fn pack_crate_manifest(
    inputs: &Inputs,
    context: &RepoContext,
    crate_spec: &crate::spec::PackedCrateSpec,
    module_version: &str,
    transforms: &mut Vec<PackagingTransform>,
) -> ToolResult<Vec<u8>> {
    let path = format!("{}/Cargo.toml", crate_spec.source_dir.trim_end_matches('/'));
    let raw = inputs.text(&path)?;
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

/// Whether a repository path names a Rust source file.
///
/// Case-sensitive on purpose, as the file-name rule always was: the payload's member
/// set has to be the set `rustc` and Cargo treat as module files, and `Ports.RS` is
/// not one of those on any platform.
fn is_rust_source(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "rs")
}

/// Every `.rs` file of one packed crate, as paths relative to its `src` directory,
/// plus the bytes that ship for it.
///
/// The member set comes from the inputs — for a candidate, from
/// `git ls-tree -r -z` on the pinned commit — and never from a listing of whatever
/// the checkout happens to contain: enumerating the disk is how Repair 1 lost a
/// tracked source file from both sides of the comparison at once (review F1-B).
/// `lib.rs` has to be among them, or there is no published crate to pack.
fn crate_sources_at(
    inputs: &Inputs,
    crate_spec: &crate::spec::PackedCrateSpec,
) -> ToolResult<Vec<(String, Vec<u8>)>> {
    let directory = crate_spec.source_dir.trim_end_matches('/');
    let prefix = format!("{directory}/src/");
    let mut sources: Vec<(String, Vec<u8>)> = inputs
        .paths_under(&format!("{directory}/src"))?
        .into_iter()
        .filter(|path| is_rust_source(path))
        .map(|path| {
            let relative = path
                .strip_prefix(&prefix)
                .unwrap_or(path.as_str())
                .to_owned();
            Ok((relative, inputs.bytes(&path)?))
        })
        .collect::<ToolResult<Vec<_>>>()?;
    sources.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    if !sources
        .iter()
        .any(|(relative, _)| relative == "lib.rs" || relative.ends_with("/lib.rs"))
    {
        return Err(ToolError::Packaging {
            crate_name: crate_spec.crate_name.clone(),
            reason: format!(
                "'{directory}/src' contains no lib.rs in {}, so there is no published crate to \
                 pack",
                inputs.label()
            ),
        });
    }
    Ok(sources)
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

// ---------------------------------------------------- the projection ---------

/// Everything one candidate is made of, derived from one fixed set of inputs.
///
/// The point of the type is that there is exactly one way to produce a candidate:
/// generation writes these bytes, and `verify` recomputes them from the commit the
/// candidate pins and compares them with what is on disk. Repair 1 had two paths —
/// one that built the payload out of the checkout and one that re-compiled the
/// payload's own declaration — so a candidate could agree perfectly with the second
/// while owing nothing to the first (review F1-A). There is no second path here.
struct Projection {
    /// The commit every input was read out of.
    revision: String,
    /// Repository facts, read from those same inputs.
    context: RepoContext,
    /// The release spec that declares this module at that commit.
    spec: ReleaseSpec,
    /// The migration mapping that commit's formal catalogs produce.
    mapping: MigrationMapping,
    /// Payload member → the canonical bytes that member must contain.
    members: BTreeMap<String, Vec<u8>>,
    /// Members that are *computed* rather than copied, and are therefore compared
    /// against a recomputation instead of against a repository blob.
    derived: BTreeSet<String>,
    /// `(repository path, payload member)` for each verbatim copy.
    verbatim: Vec<(String, String)>,
    /// Repository paths that only feed a derived record.
    feeding: Vec<String>,
    /// Every rewrite performed on a packed crate manifest.
    transforms: Vec<PackagingTransform>,
    /// `release.json`.
    identity: ModuleReleaseIdentity,
    /// `release-bundle.json`.
    bundle: ModuleReleaseBundle,
    /// Compiler contract digest.
    contract_digest: String,
    /// Compiled declaration fingerprint.
    package_digest: String,
}

impl Projection {
    /// How the inputs this projection came from are named in a refusal.
    fn label(&self) -> String {
        format!("commit '{}'", self.revision)
    }
}

/// Derives a complete candidate from one set of inputs.
///
/// Long by construction, like the two callers it serves: the steps are one linear
/// transaction (spec → declaration → compile → digests → catalogs → identity →
/// members), and splitting them up would scatter the ordering rules that keep the
/// payload frozen before any descriptive record is written next to it.
#[allow(clippy::too_many_lines)]
fn project(inputs: &Inputs, root: &Path, module: &str) -> ToolResult<Projection> {
    let revision = inputs.revision().ok_or_else(|| {
        ToolError::Usage(
            "a candidate may only be derived from a commit: the working tree is not a release \
             input, it is at most evidence that the commit's bytes are also on disk"
                .to_owned(),
        )
    })?;
    let context = inputs.repo_context(root)?;
    let spec = inputs.release_spec(module)?;
    let declaration = spec.manifest()?;
    declaration.validate()?;
    let package = business_application_compiler::BusinessApplicationPackage {
        manifest: declaration,
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
    let contract_digest =
        business_application_compiler::contract_digest(compiled_package)?.to_string();
    let package_digest = package_fingerprint(compiled_package)?.to_string();
    let mapping = migrations::build_mapping_at(inputs, &spec)?;
    let identity = ModuleReleaseIdentity {
        schema_version: MODULE_RELEASE_IDENTITY_SCHEMA_VERSION.to_owned(),
        module_id: BusinessModuleId::new(spec.module_id.clone())?,
        module_version: BusinessModuleVersion::new(spec.module_version.clone())?,
        manifest_schema_version: compiled_package.manifest.manifest_schema_version.clone(),
        source_revision: context.source_revision()?,
        contract_digest: ContractDigest::new(contract_digest.clone())?,
        package_digest: PackageDigest::new(package_digest.clone())?,
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

    // Members, in three groups: the derived records, the packed crate manifest of
    // each crate, and the crate sources copied verbatim. `feeding` is everything
    // that influenced a derived member without shipping — it is what the generation
    // pre-flight compares against the commit, and what the derived check names in
    // its detail so a reader can see what a derived member was a function of.
    let mut transforms = Vec::new();
    let mut verbatim: Vec<(String, String)> =
        vec![(spec.changelog_file.clone(), CHANGELOG_MEMBER.to_owned())];
    let mut feeding: Vec<String> = vec![
        "Cargo.toml".to_owned(),
        "Cargo.lock".to_owned(),
        "rust-toolchain.toml".to_owned(),
        spec::spec_path(&spec.module_id),
    ];
    let mut derived: BTreeSet<String> = [
        MODULE_MANIFEST_MEMBER,
        CONTRACT_SURFACE_MEMBER,
        MIGRATION_MAPPING_MEMBER,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut built: Vec<(String, Vec<u8>)> = vec![
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
            spec.changelog_at(inputs)?.into_bytes(),
        ),
    ];
    for crate_spec in &spec.packed_crates {
        let directory = crate_spec.source_dir.trim_end_matches('/');
        let prefix = format!("{PACKAGES_MEMBER}/{}", crate_spec.crate_name);
        feeding.push(format!("{directory}/Cargo.toml"));
        let member = format!("{prefix}/Cargo.toml");
        built.push((
            member.clone(),
            pack_crate_manifest(
                inputs,
                &context,
                crate_spec,
                &spec.module_version,
                &mut transforms,
            )?,
        ));
        let _ = derived.insert(member);
        for (relative, bytes) in crate_sources_at(inputs, crate_spec)? {
            built.push((format!("{prefix}/src/{relative}"), bytes));
            verbatim.push((
                format!("{directory}/src/{relative}"),
                format!("{prefix}/src/{relative}"),
            ));
        }
    }
    for file in &mapping.files {
        feeding.push(format!("{}/{}", file.catalog, file.file_name));
        feeding.push(file.catalog_manifest.clone());
    }
    verbatim.sort();
    verbatim.dedup();
    feeding.sort();
    feeding.dedup();

    // Payload members are written in normalised form, so the bytes on disk are
    // exactly the bytes the integrity digest covers: two checkouts that differ only
    // in line-ending convention produce the same artifact, and a third party can
    // reproduce every member hash with a plain SHA-256 tool. This is also where a
    // non-UTF-8 payload member is refused.
    //
    // One member is exempt: `contract/contract-surface.json` *is* the canonical
    // contract pre-image, so it ships byte-for-byte as the compiler produced it.
    // Normalising it — appending the trailing newline, for instance — would stop
    // `sha256` of that file from equalling the pinned `contract_digest`, which is
    // the only reason the file is in the artifact at all.
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (member, bytes) in built {
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
        if members.insert(member.clone(), canonical_bytes).is_some() {
            return Err(ToolError::Packaging {
                crate_name: spec.module_id.clone(),
                reason: format!("'{member}' would be built twice, so the payload is undefined"),
            });
        }
    }
    Ok(Projection {
        revision: revision.to_owned(),
        context,
        spec,
        mapping,
        members,
        derived,
        verbatim,
        feeding,
        transforms,
        identity,
        bundle,
        contract_digest,
        package_digest,
    })
}

/// Proves the checkout agrees with the commit the candidate is about to pin.
///
/// This is the only place the construction of a candidate looks at the working tree,
/// and it looks only in order to refuse: an input that was modified, deleted or
/// added since the commit means the bytes about to be released are not the bytes that
/// revision contains. Which inputs count is not a guess — it is exactly the set the
/// projection read: every source the commit lists under each packed crate, the
/// changelog, the crate manifests, the release spec, the workspace manifest, the lock
/// file, the toolchain declaration and the migration catalog files.
///
/// It runs before the output directory is created, and it is deliberately *not* a
/// whole-repository "is the tree clean" test: an unrelated dirty file elsewhere in
/// the repository says nothing about this candidate, while a deleted source file says
/// everything (review §5.4.4).
fn assert_checkout_matches_commit(
    context: &RepoContext,
    projection: &Projection,
) -> ToolResult<()> {
    let commit = Inputs::open_commit(context.root(), &projection.revision)?;
    let disk = Inputs::working_tree(context.root());
    for crate_spec in &projection.spec.packed_crates {
        let directory = crate_spec.source_dir.trim_end_matches('/');
        let committed: BTreeSet<String> = commit
            .paths_under(&format!("{directory}/src"))?
            .into_iter()
            .filter(|path| is_rust_source(path))
            .collect();
        let checkout: BTreeSet<String> = disk
            .paths_under(&format!("{directory}/src"))?
            .into_iter()
            .filter(|path| is_rust_source(path))
            .collect();
        let missing: Vec<&str> = committed
            .difference(&checkout)
            .map(String::as_str)
            .collect();
        let extra: Vec<&str> = checkout
            .difference(&committed)
            .map(String::as_str)
            .collect();
        if missing.is_empty() && extra.is_empty() {
            continue;
        }
        let mut reason = format!(
            "'{directory}/src' is not the source tree of commit '{}': ",
            projection.revision
        );
        if !missing.is_empty() {
            let _ = write!(
                reason,
                "committed file(s) absent from the checkout [{}]; ",
                missing.join(", ")
            );
        }
        if !extra.is_empty() {
            let _ = write!(
                reason,
                "file(s) the commit does not contain [{}]; ",
                extra.join(", ")
            );
        }
        let _ = write!(
            reason,
            "a candidate packs the sources of the commit it pins, so the checkout may neither \
             drop one nor add one"
        );
        return Err(ToolError::Verification {
            check: CHECK_MEMBERS.to_owned(),
            reason,
        });
    }
    let mut given = Vec::with_capacity(projection.verbatim.len() + projection.feeding.len());
    for (repo_path, _) in &projection.verbatim {
        given.push(ProvenanceInput::from_working_tree(
            repo_path.clone(),
            disk.bytes(repo_path)?,
        ));
    }
    for repo_path in &projection.feeding {
        given.push(ProvenanceInput::from_working_tree(
            repo_path.clone(),
            disk.bytes(repo_path)?,
        ));
    }
    commit.compare("input file", &given)?;
    Ok(())
}

/// Compares the identity's toolchain field with the declaration committed at the
/// pinned revision.
///
/// This is a *declaration* check in both directions: it proves `release.json`
/// states the pin `rust-toolchain.toml` carries at that commit (`toolchain_id`
/// itself derives from the pinned workspace manifest, which the input comparison
/// covers). It never claims to know which compiler build produced the bytes —
/// that evidence is the `rustc -vV` line in the run log, outside the artifact.
fn assert_declared_toolchain(
    inputs: &Inputs,
    identity: &ModuleReleaseIdentity,
) -> ToolResult<String> {
    let declared = inputs.declared_toolchain_channel()?;
    if declared != identity.build_toolchain.version {
        return Err(ToolError::Verification {
            check: crate::provenance::CHECK_TOOLCHAIN.to_owned(),
            reason: format!(
                "release.json declares build toolchain '{}' while 'rust-toolchain.toml' at {} \
                 declares '{declared}'; the identity must state the declared pin",
                identity.build_toolchain.version,
                inputs.label(),
            ),
        });
    }
    Ok(declared)
}

// --------------------------------------------------- record consistency ------

/// Rejects a descriptive record whose format or algorithm this tool does not
/// know, instead of verifying it by its own rules.
///
/// A reader that silently accepts `artifact_integrity_algorithm:
/// "unknown-algorithm.v9"` reports a digest it did not compute and cannot
/// reproduce, so an unrecognised identifier is a refusal, not a label.
fn validate_artifact_record(manifest: &ArtifactManifest) -> ToolResult<()> {
    let refused = |reason: String| ToolError::Verification {
        check: "artifact-record-format".to_owned(),
        reason,
    };
    if manifest.schema_version != ARTIFACT_MANIFEST_SCHEMA_VERSION {
        return Err(refused(format!(
            "artifact-manifest.json declares schema_version '{}' which this tool does not \
             understand; it only writes '{ARTIFACT_MANIFEST_SCHEMA_VERSION}'",
            manifest.schema_version
        )));
    }
    if manifest.artifact_integrity_algorithm != crate::canonical::ARTIFACT_INTEGRITY_ALGORITHM {
        return Err(refused(format!(
            "artifact-manifest.json declares artifact_integrity_algorithm '{}' which this tool \
             does not implement; the digest it records cannot be reproduced",
            manifest.artifact_integrity_algorithm
        )));
    }
    for file in &manifest.files {
        validate_member_path(&file.path).map_err(|error| {
            refused(format!(
            "artifact-manifest.json lists member '{}' which is not a valid payload path: {error}",
            file.path
        ))
        })?;
        validate_sha256_hex(&file.sha256, "artifact-manifest.json member digest").map_err(
            |error| {
                refused(format!(
                    "artifact-manifest.json member '{}' does not carry a SHA-256 digest: {error}",
                    file.path
                ))
            },
        )?;
    }
    let known = [
        ARTIFACT_MANIFEST_FILE,
        RELEASE_IDENTITY_FILE,
        RELEASE_BUNDLE_FILE,
        VERIFICATION_FILE,
    ];
    let declared: Vec<&str> = manifest
        .excluded_members
        .iter()
        .map(|member| member.path.as_str())
        .collect();
    for path in &declared {
        if !known.contains(path) {
            return Err(refused(format!(
                "artifact-manifest.json excludes '{path}', which is not one of the descriptive \
                 records this tool writes ({})",
                known.join(", ")
            )));
        }
        let reason = manifest
            .excluded_members
            .iter()
            .find(|member| member.path == *path)
            .map_or("", |member| member.reason.as_str());
        if reason.trim().is_empty() {
            return Err(refused(format!(
                "artifact-manifest.json excludes '{path}' without a reason; an exclusion has to \
                 say why the member is outside the digest"
            )));
        }
    }
    if declared.len() != known.len() {
        let missing: Vec<&str> = known
            .iter()
            .copied()
            .filter(|p| !declared.contains(p))
            .collect();
        return Err(refused(format!(
            "artifact-manifest.json declares {} excluded member(s) but this artifact writes {}; \
             no descriptive record may be silently treated as payload ({})",
            declared.len(),
            known.len(),
            missing.join(", ")
        )));
    }
    Ok(())
}

/// The four descriptive records must describe one and the same release.
///
/// Each field is named on failure, together with the two records that disagree,
/// so a reader can locate the tampering without re-deriving anything.
#[allow(clippy::too_many_lines)]
fn assert_records_agree(
    artifact_manifest: &ArtifactManifest,
    identity: &ModuleReleaseIdentity,
    shipped: &BusinessModuleManifest,
    mapping: &MigrationMapping,
) -> ToolResult<()> {
    let disagreed = |field: &str, in_record: &str, in_identity: &str| ToolError::Verification {
        check: "record-consistency".to_owned(),
        reason: format!(
            "artifact-manifest.json says {field} = '{in_record}' while release.json says \
                 '{in_identity}'; one artifact may not describe two releases"
        ),
    };
    if artifact_manifest.module_id != identity.module_id.as_str() {
        return Err(disagreed(
            "module_id",
            &artifact_manifest.module_id,
            identity.module_id.as_str(),
        ));
    }
    if artifact_manifest.module_version != identity.module_version.as_str() {
        return Err(disagreed(
            "module_version",
            &artifact_manifest.module_version,
            identity.module_version.as_str(),
        ));
    }
    if artifact_manifest.contract_digest != identity.contract_digest.as_str() {
        return Err(disagreed(
            "contract_digest",
            &artifact_manifest.contract_digest,
            identity.contract_digest.as_str(),
        ));
    }
    if artifact_manifest.package_digest != identity.package_digest.as_str() {
        return Err(disagreed(
            "package_digest",
            &artifact_manifest.package_digest,
            identity.package_digest.as_str(),
        ));
    }
    if shipped.module_id != identity.module_id {
        return Err(ToolError::Verification {
            check: "record-consistency".to_owned(),
            reason: format!(
                "the shipped payload manifest declares module_id '{}' while release.json \
                 declares '{}'",
                shipped.module_id, identity.module_id
            ),
        });
    }
    if shipped.module_version != identity.module_version {
        return Err(ToolError::Verification {
            check: "record-consistency".to_owned(),
            reason: format!(
                "the shipped payload manifest declares module_version '{}' while release.json \
                 declares '{}'",
                shipped.module_version, identity.module_version
            ),
        });
    }
    if shipped.manifest_schema_version != identity.manifest_schema_version {
        return Err(ToolError::Verification {
            check: "record-consistency".to_owned(),
            reason: format!(
                "the shipped payload manifest declares manifest_schema_version '{}' while \
                 release.json declares '{}'",
                shipped.manifest_schema_version, identity.manifest_schema_version
            ),
        });
    }
    if let Some(revision) = &identity.migration_revision {
        if mapping.migration_namespace != revision.migration_namespace {
            return Err(ToolError::Verification {
                check: "record-consistency".to_owned(),
                reason: format!(
                    "the shipped migration mapping declares namespace '{}' while release.json \
                     pins '{}'",
                    mapping.migration_namespace, revision.migration_namespace
                ),
            });
        }
    }
    Ok(())
}

/// `release-bundle.json` must describe the crate manifests actually shipped.
///
/// The bundle is the record that says which crates a module release consists of,
/// so it is cross-checked against the packed `Cargo.toml` files themselves:
/// missing, extra, duplicated or differently-named crates and any version that
/// differs from the packed manifest is a refusal. A bundle that names crates the
/// artifact does not contain is worse than no bundle at all.
fn assert_packed_bindings(
    bundle: &ModuleReleaseBundle,
    artifact: &Path,
    files: &[ArtifactFile],
) -> ToolResult<String> {
    let prefix = format!("{PACKAGES_MEMBER}/");
    let mut shipped: Vec<(String, String, String)> = Vec::new();
    for file in files {
        let Some(rest) = file.path.strip_prefix(&prefix) else {
            continue;
        };
        if rest.strip_suffix("/Cargo.toml").is_none() {
            continue;
        }
        let directory = rest.trim_end_matches("/Cargo.toml");
        let bytes = read_bytes(&payload_path(artifact, &file.path))?;
        let (name, version) = package_identity(&bytes, &file.path)?;
        if name != directory {
            return Err(ToolError::Verification {
                check: "packed-crate-bindings".to_owned(),
                reason: format!(
                    "payload member '{}' sits in the directory '{directory}' but its [package] \
                     name is '{name}'",
                    file.path
                ),
            });
        }
        if shipped.iter().any(|(shipped, _, _)| shipped == &name) {
            return Err(ToolError::Verification {
                check: "packed-crate-bindings".to_owned(),
                reason: format!("the payload ships the crate '{name}' more than once"),
            });
        }
        shipped.push((name, version, file.path.clone()));
    }

    let mut bindings = Vec::new();
    for binding in &bundle.crate_versions {
        if bindings.contains(&binding.crate_name) {
            return Err(ToolError::Verification {
                check: "packed-crate-bindings".to_owned(),
                reason: format!(
                    "release-bundle.json lists crate '{}' twice; the module-to-crate mapping must \
                     be one entry per crate",
                    binding.crate_name
                ),
            });
        }
        bindings.push(binding.crate_name.clone());
        let shipped = shipped
            .iter()
            .find(|(name, _, _)| *name == binding.crate_name)
            .ok_or_else(|| ToolError::Verification {
                check: "packed-crate-bindings".to_owned(),
                reason: format!(
                    "release-bundle.json binds crate '{}' at version '{}' but the payload ships no \
                     such crate manifest",
                    binding.crate_name, binding.crate_version
                ),
            })?;
        if shipped.1 != binding.crate_version {
            return Err(ToolError::Verification {
                check: "packed-crate-bindings".to_owned(),
                reason: format!(
                    "release-bundle.json binds crate '{}' at version '{}' but '{}' declares '{}'",
                    binding.crate_name, binding.crate_version, shipped.2, shipped.1
                ),
            });
        }
    }
    if let Some(extra) = shipped.iter().find(|(name, _, _)| !bindings.contains(name)) {
        return Err(ToolError::Verification {
            check: "packed-crate-bindings".to_owned(),
            reason: format!(
                "the payload ships crate '{}' ({}) which release-bundle.json does not bind",
                extra.0, extra.2
            ),
        });
    }
    Ok(shipped
        .iter()
        .map(|(name, version, _)| format!("{name}={version}"))
        .collect::<Vec<_>>()
        .join(","))
}

/// Reads the `[package]` name and version out of a packed Cargo manifest.
fn package_identity(bytes: &[u8], where_: &str) -> ToolResult<(String, String)> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| ToolError::NotUtf8 {
        path: where_.to_owned(),
    })?;
    let section =
        crate::repo::parse_section(&text, "package").map_err(|error| ToolError::Verification {
            check: "packed-crate-bindings".to_owned(),
            reason: format!("'{where_}' has no readable [package] section: {error}"),
        })?;
    let name = crate::repo::required(&section, "name", where_).map_err(|error| {
        ToolError::Verification {
            check: "packed-crate-bindings".to_owned(),
            reason: format!("'{where_}' declares no [package] name: {error}"),
        }
    })?;
    let version = crate::repo::required(&section, "version", where_).map_err(|error| {
        ToolError::Verification {
            check: "packed-crate-bindings".to_owned(),
            reason: format!("'{where_}' declares no [package] version: {error}"),
        }
    })?;
    Ok((name, version))
}

/// The payload's members must be exactly the members the pinned commit produces.
///
/// Comparing bytes answers "are these the commit's bytes". This answers the other
/// question — "is this every member the commit produces, and only those" — in both
/// directions: a candidate generated from a checkout whose tracked file was deleted
/// (review F1-B) is byte-correct about everything it does contain and still ships a
/// crate that cannot compile, and a candidate with an invented member ships a file
/// no commit ever contained.
fn assert_member_set_matches(
    files: &[ArtifactFile],
    projection: &Projection,
) -> ToolResult<String> {
    let shipped: BTreeSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
    let expected: BTreeSet<&str> = projection.members.keys().map(String::as_str).collect();
    let missing: Vec<&str> = expected.difference(&shipped).copied().collect();
    let extra: Vec<&str> = shipped.difference(&expected).copied().collect();
    if !missing.is_empty() || !extra.is_empty() {
        let mut reason = format!(
            "the payload is not the member set of {}: ",
            projection.label()
        );
        if !missing.is_empty() {
            let _ = write!(
                reason,
                "absent from the artifact [{}]; ",
                missing.join(", ")
            );
        }
        if !extra.is_empty() {
            let _ = write!(reason, "not in the commit [{}]; ", extra.join(", "));
        }
        return Err(ToolError::Verification {
            check: CHECK_MEMBERS.to_owned(),
            reason,
        });
    }
    // Every member must be covered by one of the two provenance comparisons. A
    // member that fell between them would be in the artifact and verified by nothing.
    for member in projection.members.keys() {
        if !projection.derived.contains(member)
            && !projection.verbatim.iter().any(|(_, m)| m == member)
        {
            return Err(ToolError::Verification {
                check: CHECK_MEMBERS.to_owned(),
                reason: format!(
                    "'{member}' is produced by {} but is neither re-derived nor compared against \
                     a repository file, so no provenance check would ever look at it",
                    projection.label()
                ),
            });
        }
    }
    Ok(format!(
        "{} payload member(s) are exactly the member set of {} ({} re-derived, {} compared \
         against their repository file)",
        files.len(),
        projection.label(),
        projection.derived.len(),
        projection.verbatim.len()
    ))
}

/// The members a candidate *derives* must be what re-running the derivation on the
/// pinned commit produces, compared as whole members rather than field by field.
///
/// This is the claim Repair 1 could not make. Its `verify` re-compiled the
/// declaration the artifact already shipped and re-hashed the mapping it already
/// had, so rewriting a packed `Cargo.toml` dependency version — or shipping a
/// declaration compiled from a different spec, with the digests beside it rewritten
/// to match — verified, because the artifact was only ever asked to agree with
/// itself. Here the packed crate manifests, the shipped module manifest, the
/// contract surface and the migration mapping are rebuilt from the commit's own
/// release spec, workspace manifest, lock file, crate manifests and migration
/// catalogs, and each derived member is compared in full.
fn assert_derived_members_match(
    artifact: &Path,
    projection: &Projection,
    record: &ArtifactManifest,
) -> ToolResult<String> {
    let mut compared = 0;
    for member in &projection.derived {
        let expected = &projection.members[member];
        let path = payload_path(artifact, member);
        if !path.is_file() {
            return Err(ToolError::Verification {
                check: CHECK_DERIVED.to_owned(),
                reason: format!(
                    "'{member}' is derived from {} but the artifact does not contain it",
                    projection.label()
                ),
            });
        }
        let observed = read_bytes(&path)?;
        // The contract surface is the canonical compiler pre-image and ships exactly
        // as the compiler produced it; every other member ships normalised.
        let observed = if member == CONTRACT_SURFACE_MEMBER {
            observed
        } else {
            normalize_text_bytes(&observed, member)?
        };
        if observed != *expected {
            return Err(ToolError::Verification {
                check: CHECK_DERIVED.to_owned(),
                reason: format!(
                    "package/{member} is not what {} produces (re-derived {want}, found {got}); \
                     a derived member has to be re-derived from the pinned inputs rather than \
                     trusted from the artifact it describes",
                    projection.label(),
                    want = sha256_hex(expected),
                    got = sha256_hex(&observed),
                ),
            });
        }
        compared += 1;
    }
    if record.packaging_transforms != projection.transforms {
        return Err(ToolError::Verification {
            check: CHECK_DERIVED.to_owned(),
            reason: format!(
                "artifact-manifest.json records {} packaging transform(s) but packing the \
                 pinned {} crate manifests produces {}; the packed manifests are not the ones \
                 the record describes",
                record.packaging_transforms.len(),
                projection.label(),
                projection.transforms.len()
            ),
        });
    }
    Ok(format!(
        "{compared} derived member(s) re-derived from {} and compared in full, together with {} \
         packaging transform(s); derivation inputs: {}, {} crate manifest(s), {} migration \
         catalog file(s), workspace manifest, Cargo.lock and rust-toolchain.toml at that commit",
        projection.label(),
        projection.transforms.len(),
        spec::spec_path(&projection.spec.module_id),
        projection.spec.packed_crates.len(),
        projection.mapping.files.len(),
    ))
}
