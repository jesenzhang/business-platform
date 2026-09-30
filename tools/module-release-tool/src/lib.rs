//! Local module release-candidate assembler (PLAN-0014 Wave 5A).
//!
//! This crate is **pilot tooling**, not a publish path. It takes one checked-in
//! module declaration spec, runs it through the existing pure seams
//! ([`business_module_contracts::BusinessModuleManifest`] and the
//! `business-application-compiler` `compile` / `contract_digest` /
//! `package_fingerprint` / `verify_release_identity` entry points), and writes a
//! *local release candidate* that an out-of-workspace consumer can compile
//! against. It never registers, installs, enables, publishes or deploys
//! anything, and it never modifies a module declaration, a migration or the two
//! pure crates.
//!
//! ## The four digests this tool distinguishes (never interchangeable)
//!
//! * **contract digest** — `contract_digest`: SHA-256 over the compiler's
//!   canonical published contract surface. Written to `release.json` and,
//!   byte-identically, to `package/contract/contract-surface.json`.
//! * **package digest** — `package_fingerprint`: the *compiled declaration
//!   fingerprint* (same value the planner reports per module). It pins
//!   declarations, **not** packed artifact bytes; this is the semantics fixed by
//!   `PLAN-0014` Wave 2 review §5 / §16 F-5 and it is deliberately unchanged
//!   here.
//! * **artifact integrity digest** — a *separate* record defined by this tool
//!   (`plan-0014.artifact-integrity.v1`) covering the physical bytes under
//!   `package/`. It exists precisely because the declaration fingerprint does
//!   not cover artifact bytes, and it is deliberately **not** part of
//!   `ModuleReleaseIdentity` (folding it in would change an accepted contract
//!   schema; that is registered as a design proposal).
//! * **migration set digest** — `MigrationRevision::set_digest` over the
//!   canonical migration-set manifest read from the *formal* migration catalog.
//!
//! ## Digest self-reference
//!
//! Ordering is fixed: the `package/` payload is materialised and frozen first,
//! then the descriptive records (`artifact-manifest.json`, `release.json`,
//! `release-bundle.json`) are computed *about* it, and the evidence index
//! (`verification.json`) is written last. The payload digest therefore never
//! hashes a record that describes it, and no record hashes itself. The excluded
//! members and the reason for each exclusion are declared inside
//! `artifact-manifest.json` rather than silently omitted.
//!
//! Nothing here uses a random UUID, the current time or the host name as part
//! of a release identity: a candidate is identified by `module_id`,
//! `module_version`, `source_revision` and the digests above.

pub mod canonical;
pub mod commit;
pub mod git;
pub mod isolation;
pub mod migrations;
pub mod packaging;
pub mod provenance;
pub mod repo;
pub mod spec;
pub mod surface;
pub mod tempdir;

use business_application_compiler::{CompilationError, ReleaseIdentityMismatch};
use business_module_contracts::{ManifestValidationError, ModuleContractError, ModuleReleaseError};
use thiserror::Error;

/// Schema identifier of the checked-in pilot release spec.
pub const RELEASE_SPEC_SCHEMA_VERSION: &str = "plan-0014.wave5a.release-spec.v1";
/// Schema identifier of the written `artifact-manifest.json`.
pub const ARTIFACT_MANIFEST_SCHEMA_VERSION: &str = "plan-0014.wave5a.artifact-manifest.v1";
/// Schema identifier of the written `verification.json`.
pub const VERIFICATION_INDEX_SCHEMA_VERSION: &str = "plan-0014.wave5a.verification.v1";

/// Relative paths inside one candidate artifact directory.
pub const PAYLOAD_DIRECTORY: &str = "package";
/// Path of the module manifest inside the payload.
pub const MODULE_MANIFEST_MEMBER: &str = "module.manifest.json";
/// Path of the canonical contract surface inside the payload.
pub const CONTRACT_SURFACE_MEMBER: &str = "contract/contract-surface.json";
/// Path of the migration mapping inside the payload.
pub const MIGRATION_MAPPING_MEMBER: &str = "migrations/migration-mapping.json";
/// Path of the changelog inside the payload.
pub const CHANGELOG_MEMBER: &str = "CHANGELOG.md";
/// Root of the packed crate sources inside the payload.
pub const PACKAGES_MEMBER: &str = "contract/packages";
/// Name of the artifact integrity record next to the payload.
pub const ARTIFACT_MANIFEST_FILE: &str = "artifact-manifest.json";
/// Name of the release identity record next to the payload.
pub const RELEASE_IDENTITY_FILE: &str = "release.json";
/// Name of the release bundle record next to the payload.
pub const RELEASE_BUNDLE_FILE: &str = "release-bundle.json";
/// Name of the verification index record next to the payload.
pub const VERIFICATION_FILE: &str = "verification.json";

/// Errors raised while assembling or verifying a local release candidate.
///
/// Every variant names the concrete path or check that failed: a release
/// candidate must never fail with an unlocatable message.
#[derive(Debug, Error)]
pub enum ToolError {
    /// A filesystem operation failed.
    #[error("I/O failure at '{path}': {source}")]
    Io {
        /// Path the operation addressed.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A payload member is not valid UTF-8 text.
    #[error("payload member '{path}' is not valid UTF-8; artifact members must be text")]
    NotUtf8 {
        /// Offending member.
        path: String,
    },
    /// A payload member is not a regular file (for example a symbolic link).
    #[error(
        "'{path}' is not a regular file; packaging refuses symbolic links and special entries"
    )]
    UnsupportedEntry {
        /// Offending entry.
        path: String,
    },
    /// An inventory path is not a clean repository-relative POSIX path.
    #[error("invalid artifact member path '{path}'; members must be relative POSIX paths without '.', '..', drive letters or backslashes")]
    InvalidMemberPath {
        /// Offending path.
        path: String,
    },
    /// The same member path appears twice.
    #[error("duplicate artifact member path '{path}'")]
    DuplicateMember {
        /// Duplicated path.
        path: String,
    },
    /// The checked-in release spec is missing, unknown or internally invalid.
    #[error("release spec '{path}' is invalid: {reason}")]
    Spec {
        /// Spec path.
        path: String,
        /// What is wrong.
        reason: String,
    },
    /// A crate cannot be packed without guessing.
    #[error("cannot pack crate '{crate_name}': {reason}")]
    Packaging {
        /// Crate being packed.
        crate_name: String,
        /// Why packing stopped.
        reason: String,
    },
    /// The formal migration catalog does not agree with the declared mapping.
    #[error("migration catalog check failed for '{catalog}': {reason}")]
    MigrationCatalog {
        /// Catalog that disagreed.
        catalog: String,
        /// What disagreed.
        reason: String,
    },
    /// A verification check did not hold.
    ///
    /// Wording is deliberately neutral about the command: the same check refuses a
    /// candidate that has already been written and, at generation time, a checkout
    /// whose bytes are not the ones the pinned commit contains — where there is no
    /// artifact yet for the message to point at.
    #[error("refused by the [{check}] check: {reason}")]
    Verification {
        /// Failed check identifier.
        check: String,
        /// Observed versus expected.
        reason: String,
    },
    /// The reproduced contract surface no longer matches the compiler digest.
    #[error("contract surface drift: reproduced surface hashes to '{actual}' but the compiler digest is '{expected}'")]
    SurfaceDrift {
        /// Digest reported by the compiler seam.
        expected: String,
        /// Digest of the reproduced surface bytes.
        actual: String,
    },
    /// A declaration value violates the pure contract grammar.
    /// The manifest the artifact ships failed its own validation.
    #[error(transparent)]
    Manifest(#[from] ManifestValidationError),

    #[error(transparent)]
    Contract(#[from] ModuleContractError),
    /// The declaration does not compile.
    #[error(transparent)]
    Compilation(#[from] CompilationError),
    /// The assembled release identity violates its own contract.
    #[error(transparent)]
    ReleaseContract(#[from] ModuleReleaseError),
    /// A canonical JSON step failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The release identity is inconsistent with the compiled evidence.
    #[error(transparent)]
    IdentityMismatch(#[from] ReleaseIdentityMismatch),
    /// A read-only Git command could not be run or returned something
    /// unexpected. Provenance is answered by Git, so this is a verification
    /// failure rather than something to guess around.
    #[error("git could not answer (`git {command}`): {reason}")]
    Git {
        /// The Git command that was run.
        command: String,
        /// Why it failed.
        reason: String,
    },
    /// The command line or the requested module is not usable.
    #[error("usage error: {0}")]
    Usage(String),
}

/// Result alias used across the tool.
pub type ToolResult<T> = Result<T, ToolError>;
