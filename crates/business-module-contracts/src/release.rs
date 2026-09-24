//! Canonical module release identity and multi-crate release bundle contracts.
//!
//! A module manifest is a declaration; a release identity pins one auditable
//! build of that declaration: the exact source revision, public contract
//! surface digest, compiled package digest, migration revision and build
//! toolchain. These types are pure value contracts. They do not build
//! packages, register, install, enable, uninstall or purge anything, and they
//! do not replace [`crate::BusinessModuleManifest`] as the module declaration.
//!
//! Canonical release identity fields are fixed by `SAAS_MODULE_STANDARD` §13;
//! the multi-crate mapping fulfils `SAAS_MODULE_STANDARD` §8 module-version to
//! crate-version auditability. A release tool assembles this identity from a
//! compiled package plus build evidence.
//!
//! Two validation layers, deliberately separated:
//!
//! * [`ModuleReleaseIdentity::validate`] (and the `validate` methods here)
//!   check only the *shape* of an assembled identity: supported schema
//!   version, `SemVer` module and crate versions, identifier grammar and length
//!   bounds. They prove nothing about which declaration the identity was
//!   assembled from and must not be described as consistency verification.
//! * Consistency with declaration and build evidence — re-computing the
//!   contract and compiled package digests and comparing them, plus identity/
//!   manifest agreement on module id, version, manifest schema version and
//!   migration namespace — is enforced by the packaging compiler's
//!   `verify_release_identity` entry point (`business-application-compiler`),
//!   which consumes the compiled package the identity claims to pin.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::manifest::{
    is_valid_namespace_characters, validate_sha256_hex, validate_version_token, BusinessModuleId,
    BusinessModuleVersion, ManifestSchemaVersion, ModuleContractError, PackageDigest,
};

/// Current platform-neutral release identity schema identifier.
pub const MODULE_RELEASE_IDENTITY_SCHEMA_VERSION: &str = "module-release.identity.v1";

const MAX_TOOLCHAIN_ID_LENGTH: usize = 64;
const MAX_CRATE_NAME_LENGTH: usize = 64;

/// A pinned source revision: one lower-case SHA-1 (40 hex) or SHA-256
/// (64 hex) git object identifier. Mutable branch or tag names are not a
/// release identity component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SourceRevision(String);

impl SourceRevision {
    /// Creates a source revision from 40 or 64 lower-case hexadecimal
    /// characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ModuleContractError::EmptyValue {
                kind: "source revision",
            });
        }
        if !matches!(value.len(), 40 | 64)
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            || value.bytes().any(|byte| byte.is_ascii_uppercase())
        {
            return Err(ModuleContractError::InvalidCharacters {
                kind: "source revision",
            });
        }
        Ok(Self(value))
    }

    /// Returns the canonical revision string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SourceRevision {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for SourceRevision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for SourceRevision {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SourceRevision> for String {
    fn from(value: SourceRevision) -> Self {
        value.0
    }
}

/// A validated lower-case SHA-256 digest of the module's canonical published
/// contract surface. The surface and its canonicalization are defined by the
/// packaging compiler; this type only pins the result.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContractDigest(String);

impl ContractDigest {
    /// Creates a contract digest from 64 lower-case hexadecimal characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        validate_sha256_hex(&value, "contract digest")?;
        Ok(Self(value))
    }

    /// Returns the canonical digest string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ContractDigest {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ContractDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for ContractDigest {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ContractDigest> for String {
    fn from(value: ContractDigest) -> Self {
        value.0
    }
}

/// A validated lower-case SHA-256 digest over the canonical ordered
/// migration-set manifest (namespace plus per-file digests) that a module
/// release shipped with.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MigrationSetDigest(String);

impl MigrationSetDigest {
    /// Creates a migration-set digest from 64 lower-case hexadecimal
    /// characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        validate_sha256_hex(&value, "migration set digest")?;
        Ok(Self(value))
    }

    /// Returns the canonical digest string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for MigrationSetDigest {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for MigrationSetDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for MigrationSetDigest {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<MigrationSetDigest> for String {
    fn from(value: MigrationSetDigest) -> Self {
        value.0
    }
}

/// Errors raised while validating an assembled release identity or bundle.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModuleReleaseError {
    /// A required value was empty.
    #[error("{kind} must not be empty")]
    EmptyValue { kind: &'static str },
    /// A value contains a character outside its contract alphabet.
    #[error("{kind} contains an invalid character")]
    InvalidCharacters { kind: &'static str },
    /// A value exceeds the bounded length.
    #[error("{kind} exceeds {max} characters")]
    TooLong { kind: &'static str, max: usize },
    /// The release identity schema is not understood by this contract.
    #[error("release identity schema version '{actual}' is unsupported; expected '{expected}'")]
    UnsupportedSchemaVersion { expected: String, actual: String },
    /// The release bundle mapped no crates.
    #[error("release bundle must map at least one crate")]
    EmptyCrateSet,
    /// The same crate appears twice in the bundle mapping.
    #[error("duplicate crate '{crate_name}' in release bundle")]
    DuplicateCrate { crate_name: String },
    /// A release-layer version token is not valid `SemVer`. Release identity
    /// module versions and bundle crate versions use the strict `SemVer`
    /// grammar required by `SAAS_MODULE_STANDARD` §8/§13/§14 (R2); the
    /// broader manifest-level version token grammar is unchanged.
    #[error("{kind} '{value}' is not valid SemVer")]
    NotSemVer { kind: &'static str, value: String },
}

/// The build toolchain that produced a module package release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildToolchain {
    /// Stable toolchain identifier, such as a Rust target triple or a
    /// documented build-image identifier. Lower-case; bounded length.
    pub toolchain_id: String,
    /// Toolchain version token, such as a pinned compiler version.
    pub version: String,
}

impl BuildToolchain {
    /// Validates the toolchain declaration shape.
    pub fn validate(&self) -> Result<(), ModuleReleaseError> {
        if self.toolchain_id.is_empty() {
            return Err(ModuleReleaseError::EmptyValue {
                kind: "build toolchain ID",
            });
        }
        if self.toolchain_id.len() > MAX_TOOLCHAIN_ID_LENGTH {
            return Err(ModuleReleaseError::TooLong {
                kind: "build toolchain ID",
                max: MAX_TOOLCHAIN_ID_LENGTH,
            });
        }
        let first = self.toolchain_id.as_bytes()[0];
        let last = *self.toolchain_id.as_bytes().last().unwrap_or(&first);
        let is_segment_edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        let valid_edge = is_segment_edge(first) && is_segment_edge(last);
        let valid_inner = self.toolchain_id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || ".-_".contains(char::from(byte))
        });
        if !valid_edge || !valid_inner {
            return Err(ModuleReleaseError::InvalidCharacters {
                kind: "build toolchain ID with valid characters",
            });
        }
        validate_version_token(&self.version, "build toolchain version").map_err(|_| {
            ModuleReleaseError::InvalidCharacters {
                kind: "build toolchain version",
            }
        })?;
        Ok(())
    }
}

/// The exact migration set a persistent module release shipped with: the
/// owner migration namespace plus a digest over the canonical ordered
/// migration-set manifest. Stateless modules pin `None` and their release
/// identity carries no migration revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationRevision {
    /// Owner-scoped migration namespace; same grammar as the module manifest.
    pub migration_namespace: String,
    /// SHA-256 over the canonical ordered migration-set manifest
    /// (namespace, ordered file names and per-file digests).
    pub set_digest: MigrationSetDigest,
}

impl MigrationRevision {
    /// Validates the revision pin.
    pub fn validate(&self) -> Result<(), ModuleReleaseError> {
        if self.migration_namespace.is_empty() {
            return Err(ModuleReleaseError::EmptyValue {
                kind: "migration namespace",
            });
        }
        if !is_valid_namespace_characters(&self.migration_namespace) {
            return Err(ModuleReleaseError::InvalidCharacters {
                kind: "migration namespace with valid characters",
            });
        }
        Ok(())
    }
}

/// Canonical release identity of one built module package. Deployment,
/// upgrade, rollback and audit reference this identity, never a mutable tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleReleaseIdentity {
    /// Release identity schema version.
    pub schema_version: String,
    /// Stable module identity.
    pub module_id: BusinessModuleId,
    /// Independently versioned module release version. The release layer
    /// enforces strict `SemVer` ([`Self::validate`]); the manifest-level
    /// version token grammar is a separate, broader rule.
    pub module_version: BusinessModuleVersion,
    /// Manifest schema version of the declaration this release was built from.
    pub manifest_schema_version: ManifestSchemaVersion,
    /// Pinned source revision the package was built from.
    pub source_revision: SourceRevision,
    /// Canonical digest of the published cross-module contract surface,
    /// produced by the compiler's `contract_digest` seam.
    pub contract_digest: ContractDigest,
    /// Canonical digest of the compiled module declaration — the normalized
    /// declaration fingerprint the compiler planner reports per module. It
    /// pins declarations, not packed artifact bytes; a physical artifact
    /// integrity digest is a separate, still-unimplemented concern.
    pub package_digest: PackageDigest,
    /// Pinned migration set, or `None` for modules with no persistent facts.
    #[serde(default)]
    pub migration_revision: Option<MigrationRevision>,
    /// Toolchain that produced the package.
    pub build_toolchain: BuildToolchain,
}

impl ModuleReleaseIdentity {
    /// Validates the *shape* of an assembled identity against the release
    /// contract: supported schema version, `SemVer` module release version
    /// (`SAAS_MODULE_STANDARD` §8/§14), and toolchain/migration grammar and
    /// length bounds. This is a field-shape check only: it does not compare
    /// any field against a module declaration or build evidence. Consistency
    /// verification against the compiled package the identity claims to pin
    /// (re-computed contract and package digests, manifest agreement on
    /// id/version/schema/namespace) is performed by the packaging compiler's
    /// `verify_release_identity` entry point.
    pub fn validate(&self) -> Result<(), ModuleReleaseError> {
        if self.schema_version != MODULE_RELEASE_IDENTITY_SCHEMA_VERSION {
            return Err(ModuleReleaseError::UnsupportedSchemaVersion {
                expected: MODULE_RELEASE_IDENTITY_SCHEMA_VERSION.to_owned(),
                actual: self.schema_version.clone(),
            });
        }
        if semver::Version::parse(self.module_version.as_str()).is_err() {
            return Err(ModuleReleaseError::NotSemVer {
                kind: "release module version",
                value: self.module_version.to_string(),
            });
        }
        self.build_toolchain.validate()?;
        if let Some(revision) = &self.migration_revision {
            revision.validate()?;
        }
        Ok(())
    }
}

/// One module-crate entry in an auditable module-version to crate-version
/// mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrateVersionBinding {
    /// Cargo crate name.
    pub crate_name: String,
    /// Version that crate was released with under this module release. Must be
    /// strict `SemVer` (`SAAS_MODULE_STANDARD` §8), not an arbitrary token.
    pub crate_version: String,
}

impl CrateVersionBinding {
    /// Validates one mapping entry: crate name grammar and bounds, and a
    /// strict `SemVer` crate version.
    pub fn validate(&self) -> Result<(), ModuleReleaseError> {
        if self.crate_name.is_empty() {
            return Err(ModuleReleaseError::EmptyValue { kind: "crate name" });
        }
        if self.crate_name.len() > MAX_CRATE_NAME_LENGTH {
            return Err(ModuleReleaseError::TooLong {
                kind: "crate name",
                max: MAX_CRATE_NAME_LENGTH,
            });
        }
        let bytes = self.crate_name.as_bytes();
        let edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        let valid_inner = bytes
            .iter()
            .all(|byte| edge(*byte) || *byte == b'-' || *byte == b'_');
        if !edge(bytes[0]) || !edge(*bytes.last().unwrap_or(&bytes[0])) || !valid_inner {
            return Err(ModuleReleaseError::InvalidCharacters {
                kind: "crate name with valid characters",
            });
        }
        if semver::Version::parse(&self.crate_version).is_err() {
            return Err(ModuleReleaseError::NotSemVer {
                kind: "crate version",
                value: self.crate_version.clone(),
            });
        }
        Ok(())
    }
}

/// A module release bundle: the canonical release identity plus the complete,
/// auditable mapping from the module release version to the crate versions
/// released together as one unit. A single-crate module maps exactly one
/// crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleReleaseBundle {
    /// Canonical release identity of the module package.
    pub identity: ModuleReleaseIdentity,
    /// Module-version to crate-version mapping for every crate released in
    /// this bundle. Must be non-empty and unique per crate name.
    pub crate_versions: Vec<CrateVersionBinding>,
}

impl ModuleReleaseBundle {
    /// Validates the bundle identity and its crate mapping.
    pub fn validate(&self) -> Result<(), ModuleReleaseError> {
        self.identity.validate()?;
        if self.crate_versions.is_empty() {
            return Err(ModuleReleaseError::EmptyCrateSet);
        }
        let mut names = std::collections::BTreeSet::new();
        for binding in &self.crate_versions {
            binding.validate()?;
            if !names.insert(binding.crate_name.as_str()) {
                return Err(ModuleReleaseError::DuplicateCrate {
                    crate_name: binding.crate_name.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cloned_ref_to_slice_refs)]
mod tests {
    use super::*;
    use crate::manifest::BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION;

    fn hex64(seed: u8) -> String {
        (0..64)
            .map(|index| {
                let digit = (seed + index) % 16;
                char::from_digit(u32::from(digit), 16).unwrap()
            })
            .collect()
    }

    fn valid_identity() -> ModuleReleaseIdentity {
        ModuleReleaseIdentity {
            schema_version: MODULE_RELEASE_IDENTITY_SCHEMA_VERSION.to_owned(),
            module_id: BusinessModuleId::new("module-core".to_owned()).unwrap(),
            module_version: BusinessModuleVersion::new("1.2.0".to_owned()).unwrap(),
            manifest_schema_version: ManifestSchemaVersion::new(
                BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION.to_owned(),
            )
            .unwrap(),
            source_revision: SourceRevision::new("a".repeat(40)).unwrap(),
            contract_digest: ContractDigest::new(hex64(1)).unwrap(),
            package_digest: PackageDigest::new(hex64(7)).unwrap(),
            migration_revision: Some(MigrationRevision {
                migration_namespace: "module_core".to_owned(),
                set_digest: MigrationSetDigest::new(hex64(12)).unwrap(),
            }),
            build_toolchain: BuildToolchain {
                toolchain_id: "x86_64-pc-windows-msvc".to_owned(),
                version: "1.94.1".to_owned(),
            },
        }
    }

    #[test]
    fn identity_validates_and_round_trips_serde() {
        let identity = valid_identity();
        assert_eq!(identity.validate(), Ok(()));
        let json = serde_json::to_string(&identity).unwrap();
        let restored: ModuleReleaseIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, identity);
    }

    #[test]
    fn stateless_module_identity_pins_no_migration_revision() {
        let mut identity = valid_identity();
        identity.migration_revision = None;
        assert_eq!(identity.validate(), Ok(()));
        let json = serde_json::to_string(&identity).unwrap();
        let restored: ModuleReleaseIdentity = serde_json::from_str(&json).unwrap();
        assert!(restored.migration_revision.is_none());
    }

    #[test]
    fn identity_rejects_unknown_schema_version() {
        let mut identity = valid_identity();
        identity.schema_version = "module-release.identity.v2".to_owned();
        assert!(matches!(
            identity.validate(),
            Err(ModuleReleaseError::UnsupportedSchemaVersion { .. })
        ));
    }

    #[test]
    fn source_revision_requires_full_lower_case_commit_sha() {
        assert!(SourceRevision::new("a".repeat(40)).is_ok());
        assert!(SourceRevision::new("f".repeat(64)).is_ok());
        assert!(SourceRevision::new("a".repeat(39)).is_err());
        assert!(SourceRevision::new("a".repeat(41)).is_err());
        assert!(SourceRevision::new("A".repeat(40)).is_err());
        assert!(SourceRevision::new("z".repeat(40)).is_err());
        assert!(SourceRevision::new(String::new()).is_err());
    }

    #[test]
    fn digests_reject_uppercase_and_wrong_length_hex() {
        assert!(ContractDigest::new(hex64(3)).is_ok());
        assert!(ContractDigest::new(hex64(3).to_uppercase()).is_err());
        assert!(ContractDigest::new("a".repeat(63)).is_err());
        assert!(MigrationSetDigest::new(hex64(4)).is_ok());
        assert!(MigrationSetDigest::new("xyz".to_owned()).is_err());
        assert!(PackageDigest::new(hex64(5)).is_ok());
        assert!(PackageDigest::new(hex64(5).to_uppercase()).is_err());
    }

    #[test]
    fn build_toolchain_rejects_empty_identifier_and_invalid_versions() {
        let mut toolchain = BuildToolchain {
            toolchain_id: String::new(),
            version: "1.94.1".to_owned(),
        };
        assert!(matches!(
            toolchain.validate(),
            Err(ModuleReleaseError::EmptyValue { .. })
        ));
        toolchain.toolchain_id = "rust;rm -rf".to_owned();
        assert!(matches!(
            toolchain.validate(),
            Err(ModuleReleaseError::InvalidCharacters { .. })
        ));
        toolchain.toolchain_id = "-leading".to_owned();
        assert!(matches!(
            toolchain.validate(),
            Err(ModuleReleaseError::InvalidCharacters { .. })
        ));
        toolchain.toolchain_id = "x86_64-pc-windows-msvc".to_owned();
        toolchain.version = "nightly/2026".to_owned();
        assert!(matches!(
            toolchain.validate(),
            Err(ModuleReleaseError::InvalidCharacters { .. })
        ));
        toolchain.version = "1.94.1".to_owned();
        assert_eq!(toolchain.validate(), Ok(()));
    }

    #[test]
    fn migration_revision_pins_manifest_namespace_grammar() {
        let mut revision = MigrationRevision {
            migration_namespace: "Upper.Namespace".to_owned(),
            set_digest: MigrationSetDigest::new(hex64(9)).unwrap(),
        };
        assert!(matches!(
            revision.validate(),
            Err(ModuleReleaseError::InvalidCharacters { .. })
        ));
        revision.migration_namespace = "-leading".to_owned();
        assert!(matches!(
            revision.validate(),
            Err(ModuleReleaseError::InvalidCharacters { .. })
        ));
        revision.migration_namespace = "module-core".to_owned();
        assert_eq!(revision.validate(), Ok(()));
    }

    #[test]
    fn bundle_requires_non_empty_unique_crate_mapping() {
        let bundle = ModuleReleaseBundle {
            identity: valid_identity(),
            crate_versions: Vec::new(),
        };
        assert!(matches!(
            bundle.validate(),
            Err(ModuleReleaseError::EmptyCrateSet)
        ));

        let bundle = ModuleReleaseBundle {
            identity: valid_identity(),
            crate_versions: vec![
                CrateVersionBinding {
                    crate_name: "module-core".to_owned(),
                    crate_version: "1.2.0".to_owned(),
                },
                CrateVersionBinding {
                    crate_name: "module-core-postgres".to_owned(),
                    crate_version: "1.2.0".to_owned(),
                },
            ],
        };
        assert_eq!(bundle.validate(), Ok(()));

        let mut duplicated = bundle.clone();
        duplicated.crate_versions.push(CrateVersionBinding {
            crate_name: "module-core".to_owned(),
            crate_version: "1.1.0".to_owned(),
        });
        assert!(matches!(
            duplicated.validate(),
            Err(ModuleReleaseError::DuplicateCrate { .. })
        ));

        let mut invalid_name = bundle;
        invalid_name.crate_versions = vec![CrateVersionBinding {
            crate_name: "Module Core".to_owned(),
            crate_version: "1.2.0".to_owned(),
        }];
        assert!(matches!(
            invalid_name.validate(),
            Err(ModuleReleaseError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn release_identity_requires_semver_module_version() {
        // Every token below passes the manifest-level version *token*
        // grammar but is not SemVer; the release layer must reject them.
        for rejected in ["1.2", "1", "v1.2.3", "nightly-2026-09", "1.2.3.4"] {
            let mut identity = valid_identity();
            identity.module_version = BusinessModuleVersion::new(rejected.to_owned()).unwrap();
            assert!(
                matches!(
                    identity.validate(),
                    Err(ModuleReleaseError::NotSemVer { .. })
                ),
                "release module version '{rejected}' must be rejected as non-SemVer"
            );
        }
        for accepted in ["1.2.0", "0.0.1", "1.2.0-rc.1"] {
            let mut identity = valid_identity();
            identity.module_version = BusinessModuleVersion::new(accepted.to_owned()).unwrap();
            assert_eq!(
                identity.validate(),
                Ok(()),
                "release module version '{accepted}' must satisfy SemVer"
            );
        }
    }

    #[test]
    fn bundle_requires_semver_crate_versions() {
        let mut bundle = ModuleReleaseBundle {
            identity: valid_identity(),
            crate_versions: vec![CrateVersionBinding {
                crate_name: "module-core".to_owned(),
                crate_version: "1.2.0".to_owned(),
            }],
        };
        assert_eq!(bundle.validate(), Ok(()));
        for rejected in ["1.2", "nightly", "0"] {
            bundle.crate_versions[0].crate_version = rejected.to_owned();
            assert!(
                matches!(bundle.validate(), Err(ModuleReleaseError::NotSemVer { .. })),
                "crate version '{rejected}' must be rejected as non-SemVer"
            );
        }
        bundle.crate_versions[0].crate_version = "1.2.0-rc.1".to_owned();
        assert_eq!(bundle.validate(), Ok(()));
    }

    #[test]
    fn release_contracts_reject_unknown_fields_and_bad_digest_bytes() {
        let mut identity = valid_identity();
        identity.package_digest = PackageDigest::new(hex64(2)).unwrap();
        let json = serde_json::to_string(&identity).unwrap();
        let tampered = json.replace("\"package_digest\":", "\"extra\":true,\"package_digest\":");
        assert!(serde_json::from_str::<ModuleReleaseIdentity>(&tampered).is_err());

        let bad_digest = json.replace(&hex64(2), &hex64(2).to_uppercase());
        assert!(serde_json::from_str::<ModuleReleaseIdentity>(&bad_digest).is_err());
    }
}
