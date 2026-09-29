//! The checked-in declaration a candidate is built from.
//!
//! One JSON file per pilot module under `tools/module-release-tool/specs/`. The
//! spec is deliberately *not* a second module manifest: it is a source form that
//! this tool compiles into the single authoritative
//! [`business_module_contracts::BusinessModuleManifest`] value, and every
//! declaration carries the authority string it was derived from so a reviewer
//! can trace each field back to an accepted document or to real code.
//!
//! Fields the platform does not yet have an authoritative owner for cannot be
//! expressed here at all — there is no key for semantic, UI or agent
//! contributions, for extension points, or for invented platform capability
//! identifiers. Those gaps are recorded as design proposals in
//! `docs/reviews/2026-09-29-wave5a-implementation-record.md` §12 instead of
//! being filled with plausible-looking values.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, CompatibilityDescriptor,
    ContractDescriptor, DataClassification, ManifestSchemaVersion, ModuleDependency,
    ResourceKindDescriptor, BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

use crate::repo::RepoContext;
use crate::{ToolError, ToolResult};

/// Directory the checked-in release specs live in.
pub const SPEC_DIRECTORY: &str = "tools/module-release-tool/specs";

/// Repository-relative path of one module's release spec.
///
/// Provenance has to prove that *this* file, as committed, is the declaration a
/// candidate was compiled from, so the path is derived in exactly one place
/// instead of being re-typed by every caller that needs it.
#[must_use]
pub fn spec_path(module: &str) -> String {
    format!("{SPEC_DIRECTORY}/{module}.json")
}

/// One published command, query or event declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSpec {
    /// Stable contract identifier (lower kebab).
    pub contract_id: String,
    /// Contract surface version, independent of the module release version.
    pub version: String,
    /// Accepted authority that makes this a published contract.
    pub authority: String,
    /// Real code entry point implementing it.
    pub implemented_by: String,
}

/// One published resource kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceKindSpec {
    /// Resource kind identifier.
    pub resource_kind: String,
    /// Version of that kind.
    pub version: String,
    /// Authority for the publication decision.
    pub authority: String,
}

/// One platform capability requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySpec {
    /// Capability identifier as registered by the platform capability owner.
    pub capability_id: String,
    /// Version requirement.
    pub version_requirement: String,
    /// Whether the module fails closed without it.
    pub required: bool,
    /// Authority for the requirement.
    pub authority: String,
}

/// One module dependency declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencySpec {
    /// Depended-on module id.
    pub module_id: String,
    /// Version requirement.
    pub version_requirement: String,
    /// Authority for the dependency.
    pub authority: String,
}

/// Platform compatibility window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilitySpec {
    /// Minimum platform version.
    pub minimum_platform_version: Option<String>,
    /// Maximum platform version, `None` while no upper bound is declared.
    pub maximum_platform_version: Option<String>,
    /// Authority for the window.
    pub authority: String,
}

/// A crate that belongs to the module and is packed into the artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackedCrateSpec {
    /// Cargo package name.
    pub crate_name: String,
    /// Repository-relative crate directory.
    pub source_dir: String,
}

/// One migration set belonging to the module, as recorded by the formal
/// catalog. Sets are never invented: every file must exist in the referenced
/// catalog and match its recorded checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationSetSpec {
    /// Backend identifier, for example `sqlite` or `postgres`.
    pub backend: String,
    /// Repository-relative catalog directory.
    pub catalog: String,
    /// Repository-relative `MANIFEST.sha256` for that catalog.
    pub catalog_manifest: String,
    /// Ledger table the runtime uses, when the backend records one.
    pub ledger_table: Option<String>,
    /// Whether this module owns the whole catalog (a module-owned catalog must
    /// be claimed in full; a shared catalog is claimed file by file).
    pub module_owned: bool,
    /// Plain-language ownership statement, copied from the catalog's own header
    /// where one exists.
    pub ownership_note: String,
    /// Exact file names this module claims from that catalog.
    pub files: Vec<String>,
}

/// A whole pilot release spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSpec {
    /// Spec schema identifier.
    pub spec_version: String,
    /// Module identity.
    pub module_id: String,
    /// Independent module release version (strict `SemVer`, pre-release allowed).
    pub module_version: String,
    /// Authoritative data owner of the module's bounded contexts.
    pub data_owner: String,
    /// Bounded contexts the module owns.
    pub owned_bounded_contexts: Vec<String>,
    /// Crates packed into the artifact and mapped in the release bundle.
    pub packed_crates: Vec<PackedCrateSpec>,
    /// Platform capability requirements.
    pub platform_capabilities: Vec<CapabilitySpec>,
    /// Published commands.
    pub published_commands: Vec<ContractSpec>,
    /// Published queries.
    pub published_queries: Vec<ContractSpec>,
    /// Published integration events.
    pub published_events: Vec<ContractSpec>,
    /// Published resource kinds.
    pub resource_kinds: Vec<ResourceKindSpec>,
    /// Data classifications in platform vocabulary.
    pub data_classification: Vec<String>,
    /// Owner-scoped migration namespace.
    pub migration_namespace: String,
    /// Whether the module owns persistent facts.
    pub persistent: bool,
    /// Migration sets claimed from formal catalogs. Must be non-empty exactly
    /// when `persistent` is true.
    pub migration_sets: Vec<MigrationSetSpec>,
    /// Module dependencies.
    pub dependencies: Vec<DependencySpec>,
    /// Platform compatibility window.
    pub compatibility: CompatibilitySpec,
    /// Repository-relative changelog file shipped inside the artifact.
    pub changelog_file: String,
    /// Per-field authority notes, so each declaration is traceable.
    pub declaration_authority: BTreeMap<String, String>,
}

impl ReleaseSpec {
    /// Loads `tools/module-release-tool/specs/<module>.json`.
    pub fn load(context: &RepoContext, module: &str) -> ToolResult<Self> {
        let path: PathBuf = Path::new(SPEC_DIRECTORY).join(format!("{module}.json"));
        let shown = context.root().join(&path).display().to_string();
        let text = std::fs::read_to_string(context.root().join(&path)).map_err(|source| {
            ToolError::Spec {
                path: shown.clone(),
                reason: source.to_string(),
            }
        })?;
        Self::from_json(&text, module, &shown)
    }

    /// Parses and validates a spec from text without reading it from disk.
    ///
    /// `path` is only used in error messages. Tests and external callers use
    /// this to check a candidate declaration before it is written to the
    /// repository, so an invalid declaration never has to touch the tree.
    pub fn from_json(text: &str, module: &str, path: &str) -> ToolResult<Self> {
        let spec: Self = serde_json::from_str(text).map_err(|error| ToolError::Spec {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
        spec.validate_against(module, path)?;
        Ok(spec)
    }

    fn validate_against(&self, module: &str, path: &str) -> ToolResult<()> {
        if self.spec_version != crate::RELEASE_SPEC_SCHEMA_VERSION {
            return Err(ToolError::Spec {
                path: path.to_owned(),
                reason: format!(
                    "spec_version '{}' is not '{}'",
                    self.spec_version,
                    crate::RELEASE_SPEC_SCHEMA_VERSION
                ),
            });
        }
        if self.module_id != module {
            return Err(ToolError::Spec {
                path: path.to_owned(),
                reason: format!(
                    "spec declares module_id '{}' but '{module}' was requested",
                    self.module_id
                ),
            });
        }
        if self.packed_crates.is_empty() {
            return Err(ToolError::Spec {
                path: path.to_owned(),
                reason: "a module release bundle must map at least one crate".to_owned(),
            });
        }
        if self.owned_bounded_contexts.is_empty() || self.data_owner.trim().is_empty() {
            return Err(ToolError::Spec {
                path: path.to_owned(),
                reason: "owned_bounded_contexts and data_owner are mandatory".to_owned(),
            });
        }
        if self.persistent == self.migration_sets.is_empty() {
            return Err(ToolError::Spec {
                path: path.to_owned(),
                reason: format!(
                    "persistent={} disagrees with {} declared migration set(s); a stateless \
                     module must declare none and must pin no migration revision",
                    self.persistent,
                    self.migration_sets.len()
                ),
            });
        }
        for classification in &self.data_classification {
            classification_of(classification)?;
        }
        for set in &self.migration_sets {
            if set.files.is_empty()
                || set.backend.trim().is_empty()
                || set.catalog.trim().is_empty()
            {
                return Err(ToolError::Spec {
                    path: path.to_owned(),
                    reason: "a migration set needs a backend, a catalog and at least one file"
                        .to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Builds the authoritative module manifest value from the spec.
    pub fn manifest(&self) -> ToolResult<BusinessModuleManifest> {
        let mut classifications = Vec::new();
        for classification in &self.data_classification {
            classifications.push(classification_of(classification)?);
        }
        let mut required = Vec::new();
        let mut optional = Vec::new();
        for capability in &self.platform_capabilities {
            let entry = business_module_contracts::PlatformCapabilityRequirement {
                capability_id: capability.capability_id.clone(),
                version_requirement: capability.version_requirement.clone(),
            };
            if capability.required {
                required.push(entry);
            } else {
                optional.push(entry);
            }
        }
        let mut dependencies = Vec::new();
        for dependency in &self.dependencies {
            dependencies.push(ModuleDependency {
                module_id: BusinessModuleId::new(dependency.module_id.clone())?,
                version_requirement: dependency.version_requirement.clone(),
            });
        }
        Ok(BusinessModuleManifest {
            module_id: BusinessModuleId::new(self.module_id.clone())?,
            module_version: BusinessModuleVersion::new(self.module_version.clone())?,
            manifest_schema_version: ManifestSchemaVersion::new(
                BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION.to_owned(),
            )?,
            owned_bounded_contexts: self.owned_bounded_contexts.clone(),
            required_platform_capabilities: required,
            optional_platform_capabilities: optional,
            published_commands: descriptors(&self.published_commands),
            published_queries: descriptors(&self.published_queries),
            published_events: descriptors(&self.published_events),
            resource_kinds: self
                .resource_kinds
                .iter()
                .map(|kind| ResourceKindDescriptor {
                    resource_kind: kind.resource_kind.clone(),
                    version: kind.version.clone(),
                })
                .collect(),
            data_classification: classifications,
            migration_namespace: self.migration_namespace.clone(),
            semantic_contributions: Vec::new(),
            ui_contributions: Vec::new(),
            agent_tool_contributions: Vec::new(),
            dependencies,
            compatibility: CompatibilityDescriptor {
                minimum_platform_version: self.compatibility.minimum_platform_version.clone(),
                maximum_platform_version: self.compatibility.maximum_platform_version.clone(),
            },
        })
    }

    /// Changeg log text shipped inside the artifact.
    pub fn changelog(&self, context: &RepoContext) -> ToolResult<String> {
        let path = context.root().join(&self.changelog_file);
        std::fs::read_to_string(&path).map_err(|source| ToolError::Spec {
            path: path.display().to_string(),
            reason: source.to_string(),
        })
    }
}

fn descriptors(contracts: &[ContractSpec]) -> Vec<ContractDescriptor> {
    contracts
        .iter()
        .map(|contract| ContractDescriptor {
            contract_id: contract.contract_id.clone(),
            version: contract.version.clone(),
        })
        .collect()
}

fn classification_of(value: &str) -> ToolResult<DataClassification> {
    Ok(match value {
        "public" => DataClassification::Public,
        "internal" => DataClassification::Internal,
        "confidential" => DataClassification::Confidential,
        "restricted" => DataClassification::Restricted,
        other => {
            return Err(ToolError::Spec {
                path: "data_classification".to_owned(),
                reason: format!("'{other}' is not a platform data classification"),
            })
        }
    })
}
