use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_IDENTIFIER_LENGTH: usize = 128;

/// Current platform-neutral manifest schema identifier.
pub const BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION: &str = "business-module.manifest.v1";

/// A stable, lower-case identifier for a business module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BusinessModuleId(String);

impl BusinessModuleId {
    /// Creates a module ID using the repository's lower-kebab-case rule.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        validate_module_id(&value)?;
        Ok(Self(value))
    }

    /// Returns the stable string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for BusinessModuleId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for BusinessModuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for BusinessModuleId {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<BusinessModuleId> for String {
    fn from(value: BusinessModuleId) -> Self {
        value.0
    }
}

/// A stable identity scoped to a business module.
///
/// The canonical representation is `<module-id>.<local-id>`. Both segments
/// are contract identifiers; labels, paths, routes and storage names are not
/// part of this value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NamespacedId(String);

impl NamespacedId {
    /// Creates a namespaced identity from its canonical representation.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        validate_namespaced_id(&value)?;
        Ok(Self(value))
    }

    /// Creates a namespaced identity from a module and module-local ID.
    pub fn from_parts(
        module_id: impl AsRef<str>,
        local_id: impl Into<String>,
    ) -> Result<Self, ModuleContractError> {
        let module_id = module_id.as_ref();
        let local_id = local_id.into();
        validate_module_id(module_id)?;
        validate_local_id(&local_id)?;
        Self::new(format!("{module_id}.{local_id}"))
    }

    /// Returns the canonical string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the owning module segment.
    #[must_use]
    pub fn module_id(&self) -> &str {
        self.0
            .split_once('.')
            .map_or("", |(module_id, _)| module_id)
    }

    /// Returns the module-local segment.
    #[must_use]
    pub fn local_id(&self) -> &str {
        self.0.split_once('.').map_or("", |(_, local_id)| local_id)
    }
}

impl AsRef<str> for NamespacedId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for NamespacedId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for NamespacedId {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<NamespacedId> for String {
    fn from(value: NamespacedId) -> Self {
        value.0
    }
}

macro_rules! namespaced_identity {
    ($(#[$meta:meta])* $name:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(NamespacedId);

        impl $name {
            /// Creates an identity from its canonical representation.
            pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
                NamespacedId::new(value)
                    .map(Self)
                    .map_err(|error| with_identity_kind(error, $kind))
            }

            /// Creates an identity from a module and module-local ID.
            pub fn from_parts(
                module_id: impl AsRef<str>,
                local_id: impl Into<String>,
            ) -> Result<Self, ModuleContractError> {
                NamespacedId::from_parts(module_id, local_id)
                    .map(Self)
                    .map_err(|error| with_identity_kind(error, $kind))
            }

            /// Returns the canonical string representation.
            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            /// Returns the owning module segment.
            #[must_use]
            pub fn module_id(&self) -> &str {
                self.0.module_id()
            }

            /// Returns the module-local segment.
            #[must_use]
            pub fn local_id(&self) -> &str {
                self.0.local_id()
            }

            /// Returns the validated generic namespaced identity seam.
            #[must_use]
            pub fn as_namespaced_id(&self) -> &NamespacedId {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl TryFrom<String> for $name {
            type Error = ModuleContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0.into()
            }
        }
    };
}

namespaced_identity!(
    /// A stable identity for a published contribution.
    ContributionId,
    "contribution ID"
);
namespaced_identity!(
    /// A stable identity for a published extension point.
    ExtensionPointId,
    "extension point ID"
);
namespaced_identity!(
    /// A stable identity for a UI contribution.
    UiContributionId,
    "UI contribution ID"
);
namespaced_identity!(
    /// A stable identity for a policy requirement.
    PolicyRequirementId,
    "policy requirement ID"
);
namespaced_identity!(
    /// A stable identity for an agent capability contribution.
    AgentCapabilityId,
    "agent capability ID"
);
namespaced_identity!(
    /// A stable identity for a capability requirement.
    CapabilityRequirementId,
    "capability requirement ID"
);

/// A validated lower-case SHA-256 package digest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackageDigest(String);

impl PackageDigest {
    /// Creates a package digest from 64 lower-case hexadecimal characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ModuleContractError::EmptyValue {
                kind: "package digest",
            });
        }
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ModuleContractError::InvalidCharacters {
                kind: "package digest",
            });
        }
        if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(ModuleContractError::InvalidCharacters {
                kind: "package digest",
            });
        }
        Ok(Self(value))
    }

    /// Returns the canonical digest string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PackageDigest {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PackageDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for PackageDigest {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PackageDigest> for String {
    fn from(value: PackageDigest) -> Self {
        value.0
    }
}

/// A version attached to a module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BusinessModuleVersion(String);

impl BusinessModuleVersion {
    /// Creates a version token accepted by the module contract.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        validate_version_token(&value, "module version")?;
        Ok(Self(value))
    }

    /// Returns the stable string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BusinessModuleVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for BusinessModuleVersion {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<BusinessModuleVersion> for String {
    fn from(value: BusinessModuleVersion) -> Self {
        value.0
    }
}

/// The version of the manifest schema used by a module declaration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ManifestSchemaVersion(String);

impl ManifestSchemaVersion {
    /// Creates a manifest schema version token.
    pub fn new(value: impl Into<String>) -> Result<Self, ModuleContractError> {
        let value = value.into();
        validate_version_token(&value, "manifest schema version")?;
        Ok(Self(value))
    }

    /// Returns the stable string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ManifestSchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for ManifestSchemaVersion {
    type Error = ModuleContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ManifestSchemaVersion> for String {
    fn from(value: ManifestSchemaVersion) -> Self {
        value.0
    }
}

/// Platform-wide data classification used by module and semantic contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    /// Data approved for unrestricted public distribution.
    Public,
    /// Data for authenticated internal users and services.
    Internal,
    /// Data requiring explicit business and tenant authorization.
    Confidential,
    /// Data requiring the strongest policy, audit and redaction controls.
    Restricted,
}

/// Installation state is intentionally separate from data retention state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleInstallationState {
    /// The module package and declaration are installed but not serving traffic.
    Installed,
    /// The module is allowed to publish its declared capabilities.
    Enabled,
    /// The module is intentionally stopped while its data may remain.
    Disabled,
    /// The module package is removed from the active registry.
    Uninstalled,
}

/// Data retention state is independent from module installation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleDataState {
    /// Data is retained according to the owner and retention policy.
    Retained,
    /// Data has been explicitly purged under an authorized operation.
    Purged,
}

/// A required or optional platform capability and the compatible version range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformCapabilityRequirement {
    /// Stable capability identifier.
    pub capability_id: String,
    /// Exact, wildcard, caret or tilde requirement interpreted by the compiler.
    pub version_requirement: String,
}

/// A versioned command, query or event published by a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractDescriptor {
    /// Stable public contract identifier.
    pub contract_id: String,
    /// Contract schema version.
    pub version: String,
}

/// A versioned resource kind exposed by a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceKindDescriptor {
    /// Stable resource kind identifier.
    pub resource_kind: String,
    /// Resource contract version.
    pub version: String,
}

/// A generic UI or agent contribution declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContributionDescriptor {
    /// Stable contribution identifier.
    pub contribution_id: String,
    /// Contribution contract version.
    pub version: String,
}

/// A semantic object declared by a module and expected in its contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticContributionDescriptor {
    /// Module-local semantic identifier.
    pub semantic_id: String,
    /// ADR-0017 semantic kind name, such as `metric` or `dataset`.
    pub semantic_kind: String,
    /// Semantic definition version.
    pub version: String,
}

/// A public target that a contribution may reference. It deliberately has no
/// physical persistence or executable representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicContributionTarget {
    pub owner_module_id: BusinessModuleId,
    pub target: PublicTargetKind,
    pub version: String,
}

/// The three public application surfaces available to typed contributions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicTargetKind {
    Resource { resource_kind: String },
    Query { query_id: String },
    Command { command_id: String },
}

/// A declaration of a policy requirement. Declaration never grants access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRequirementDescriptor {
    pub requirement_id: PolicyRequirementId,
    pub owner_module_id: BusinessModuleId,
    pub schema_version: String,
    pub policy_id: String,
    pub version: String,
}

/// A declaration of a platform capability requirement. Declaration never
/// grants access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequirementDescriptor {
    pub requirement_id: CapabilityRequirementId,
    pub owner_module_id: BusinessModuleId,
    pub schema_version: String,
    pub capability_id: String,
    pub version: String,
}

macro_rules! ui_contribution {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub contribution_id: UiContributionId,
            pub owner_module_id: BusinessModuleId,
            pub schema_version: String,
            pub version: String,
            pub target: PublicContributionTarget,
            pub label_key: String,
            #[serde(default)]
            pub ordering: Option<i32>,
            #[serde(default)]
            pub group: Option<String>,
            pub visibility: String,
            #[serde(default)]
            pub required_policy: Vec<PolicyRequirementId>,
            #[serde(default)]
            pub required_capability: Vec<CapabilityRequirementId>,
        }
    };
}

ui_contribution!(NavigationContribution);
ui_contribution!(ListViewContribution);
ui_contribution!(DetailSectionContribution);
ui_contribution!(DetailTabContribution);
ui_contribution!(ActionContribution);
ui_contribution!(CommandContribution);

/// A typed, declarative agent capability contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCapabilityContribution {
    pub contribution_id: AgentCapabilityId,
    pub owner_module_id: BusinessModuleId,
    pub schema_version: String,
    pub version: String,
    pub target: PublicContributionTarget,
    pub label_key: String,
    #[serde(default)]
    pub required_policy: Vec<PolicyRequirementId>,
    #[serde(default)]
    pub required_capability: Vec<CapabilityRequirementId>,
}

/// The Stage 4 contribution set. This is a pure declaration and validation
/// seam; it is not a registry and performs no authorization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedContributionSet {
    #[serde(default)]
    pub navigation: Vec<NavigationContribution>,
    #[serde(default)]
    pub list_views: Vec<ListViewContribution>,
    #[serde(default)]
    pub detail_sections: Vec<DetailSectionContribution>,
    #[serde(default)]
    pub detail_tabs: Vec<DetailTabContribution>,
    #[serde(default)]
    pub actions: Vec<ActionContribution>,
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub agent_capabilities: Vec<AgentCapabilityContribution>,
    #[serde(default)]
    pub policy_requirements: Vec<PolicyRequirementDescriptor>,
    #[serde(default)]
    pub capability_requirements: Vec<CapabilityRequirementDescriptor>,
}

/// Public catalog used to validate that contribution targets are published.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicContributionCatalog {
    pub public_targets: Vec<PublicContributionTarget>,
}

impl TypedContributionSet {
    pub fn validate(
        &self,
        owner_module_id: &BusinessModuleId,
        catalog: &PublicContributionCatalog,
    ) -> Result<(), ManifestValidationError> {
        let mut ids = BTreeSet::new();
        let mut check = |id: &str, owner: &BusinessModuleId, target: &PublicContributionTarget| {
            if owner != owner_module_id {
                return Err(ManifestValidationError::WrongContributionOwner {
                    expected: owner_module_id.to_string(),
                    actual: owner.to_string(),
                });
            }
            if !ids.insert(id.to_owned()) {
                return Err(ManifestValidationError::DuplicateIdentifier {
                    kind: "typed contribution",
                    identifier: id.to_owned(),
                });
            }
            validate_public_target(target, catalog)
        };
        macro_rules! check_ui {
            ($items:expr) => {
                for item in &$items {
                    check(
                        item.contribution_id.as_str(),
                        &item.owner_module_id,
                        &item.target,
                    )?;
                }
            };
        }
        check_ui!(self.navigation);
        check_ui!(self.list_views);
        check_ui!(self.detail_sections);
        check_ui!(self.detail_tabs);
        check_ui!(self.actions);
        check_ui!(self.commands);
        for item in &self.agent_capabilities {
            check(
                item.contribution_id.as_str(),
                &item.owner_module_id,
                &item.target,
            )?;
        }
        for item in &self.policy_requirements {
            if item.owner_module_id != *owner_module_id {
                return Err(ManifestValidationError::WrongContributionOwner {
                    expected: owner_module_id.to_string(),
                    actual: item.owner_module_id.to_string(),
                });
            }
        }
        for item in &self.capability_requirements {
            if item.owner_module_id != *owner_module_id {
                return Err(ManifestValidationError::WrongContributionOwner {
                    expected: owner_module_id.to_string(),
                    actual: item.owner_module_id.to_string(),
                });
            }
        }
        Ok(())
    }
}

fn validate_public_target(
    target: &PublicContributionTarget,
    catalog: &PublicContributionCatalog,
) -> Result<(), ManifestValidationError> {
    validate_non_empty(&target.version, "public target version")?;
    let found = catalog
        .public_targets
        .iter()
        .any(|candidate| candidate == target);
    if !found {
        return Err(ManifestValidationError::UnknownPublicTarget);
    }
    Ok(())
}

/// A dependency on another published business module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleDependency {
    /// Target business module identifier.
    pub module_id: BusinessModuleId,
    /// Version requirement for the target module.
    pub version_requirement: String,
}

/// Platform compatibility window for a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityDescriptor {
    /// Minimum compatible platform version, if constrained.
    #[serde(default)]
    pub minimum_platform_version: Option<String>,
    /// Maximum compatible platform version, if constrained.
    #[serde(default)]
    pub maximum_platform_version: Option<String>,
}

/// Complete platform-neutral declaration of a business module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessModuleManifest {
    /// Stable module identity.
    pub module_id: BusinessModuleId,
    /// Installed module version.
    pub module_version: BusinessModuleVersion,
    /// Manifest schema version.
    pub manifest_schema_version: ManifestSchemaVersion,
    /// Bounded Contexts whose business meaning is owned by this module.
    pub owned_bounded_contexts: Vec<String>,
    /// Platform capabilities required for the module to operate.
    #[serde(default)]
    pub required_platform_capabilities: Vec<PlatformCapabilityRequirement>,
    /// Platform capabilities used when available but not required.
    #[serde(default)]
    pub optional_platform_capabilities: Vec<PlatformCapabilityRequirement>,
    /// Commands published by the module.
    #[serde(default)]
    pub published_commands: Vec<ContractDescriptor>,
    /// Queries published by the module.
    #[serde(default)]
    pub published_queries: Vec<ContractDescriptor>,
    /// Events published by the module.
    #[serde(default)]
    pub published_events: Vec<ContractDescriptor>,
    /// Business resource kinds published by the module.
    #[serde(default)]
    pub resource_kinds: Vec<ResourceKindDescriptor>,
    /// Classifications that can be emitted by this module.
    #[serde(default)]
    pub data_classification: Vec<DataClassification>,
    /// Owner-scoped migration namespace reserved for future adapters.
    pub migration_namespace: String,
    /// Semantic definitions expected from the module.
    #[serde(default)]
    pub semantic_contributions: Vec<SemanticContributionDescriptor>,
    /// UI declarations; not executable permissions.
    #[serde(default)]
    pub ui_contributions: Vec<ContributionDescriptor>,
    /// Agent tool declarations; not executable permissions.
    #[serde(default)]
    pub agent_tool_contributions: Vec<ContributionDescriptor>,
    /// Dependencies on other published business modules.
    #[serde(default)]
    pub dependencies: Vec<ModuleDependency>,
    /// Platform compatibility window.
    #[serde(default)]
    pub compatibility: CompatibilityDescriptor,
}

impl BusinessModuleManifest {
    /// Validates local manifest invariants before cross-module compilation.
    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        if self.owned_bounded_contexts.is_empty() {
            return Err(ManifestValidationError::MissingOwnedBoundedContext {
                module_id: self.module_id.to_string(),
            });
        }
        if self.manifest_schema_version.as_str() != BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestValidationError::UnsupportedSchemaVersion {
                expected: BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION.to_owned(),
                actual: self.manifest_schema_version.to_string(),
            });
        }
        for context in &self.owned_bounded_contexts {
            validate_non_empty(context, "owned bounded context")?;
        }
        validate_namespace(&self.migration_namespace)?;
        validate_capabilities(
            &self.required_platform_capabilities,
            &self.optional_platform_capabilities,
        )?;
        validate_contracts(&self.published_commands, "command")?;
        validate_contracts(&self.published_queries, "query")?;
        validate_contracts(&self.published_events, "event")?;
        validate_resource_kinds(&self.resource_kinds)?;
        validate_contributions(&self.semantic_contributions, "semantic")?;
        validate_contributions(&self.ui_contributions, "ui")?;
        validate_contributions(&self.agent_tool_contributions, "agent tool")?;

        let module_id = self.module_id.to_string();
        for dependency in &self.dependencies {
            if dependency.module_id == self.module_id {
                return Err(ManifestValidationError::SelfDependency { module_id });
            }
            validate_version_requirement(&dependency.version_requirement)?;
        }
        Ok(())
    }
}

/// Errors raised while constructing strongly typed module contract values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ModuleContractError {
    /// A required value was empty.
    #[error("{kind} must not be empty")]
    EmptyValue { kind: &'static str },
    /// A value contains a character outside its contract alphabet.
    #[error("{kind} contains an invalid character")]
    InvalidCharacters { kind: &'static str },
    /// A value exceeds the bounded identifier length.
    #[error("{kind} exceeds {max} characters")]
    TooLong { kind: &'static str, max: usize },
}

/// Errors raised by local manifest validation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManifestValidationError {
    /// The module has no owned bounded context.
    #[error("module '{module_id}' must own at least one bounded context")]
    MissingOwnedBoundedContext { module_id: String },
    /// The manifest schema is not understood by this compiler.
    #[error("manifest schema version '{actual}' is unsupported; expected '{expected}'")]
    UnsupportedSchemaVersion { expected: String, actual: String },
    /// A generic field was empty.
    #[error("{kind} must not be empty")]
    EmptyField { kind: &'static str },
    /// A generic field contains an invalid value.
    #[error("{kind} is invalid")]
    InvalidField { kind: &'static str },
    /// A manifest list contains a duplicate identifier.
    #[error("duplicate {kind} identifier '{identifier}'")]
    DuplicateIdentifier {
        kind: &'static str,
        identifier: String,
    },
    /// The same platform capability is listed as required and optional.
    #[error("platform capability '{capability_id}' is both required and optional")]
    DuplicatePlatformCapability { capability_id: String },
    /// A module depends on itself.
    #[error("module '{module_id}' cannot depend on itself")]
    SelfDependency { module_id: String },
    /// A contribution is declared by a different module than its manifest.
    #[error("contribution owner '{actual}' does not match expected module '{expected}'")]
    WrongContributionOwner { expected: String, actual: String },
    /// A contribution points at an unpublished public target.
    #[error("contribution target is not a published public contract")]
    UnknownPublicTarget,
    /// A version requirement cannot be interpreted by the compiler.
    #[error("invalid version requirement '{requirement}'")]
    InvalidVersionRequirement { requirement: String },
}

fn validate_module_id(value: &str) -> Result<(), ModuleContractError> {
    validate_lower_kebab_id(value, "module ID")
}

fn validate_local_id(value: &str) -> Result<(), ModuleContractError> {
    validate_lower_kebab_id(value, "local ID")
}

fn validate_lower_kebab_id(value: &str, kind: &'static str) -> Result<(), ModuleContractError> {
    validate_length_and_non_empty(value, kind)?;
    let mut previous_separator = false;
    for (index, character) in value.chars().enumerate() {
        let is_separator = character == '-';
        if (!character.is_ascii_lowercase() && !character.is_ascii_digit() && !is_separator)
            || (index == 0 && is_separator)
            || previous_separator && is_separator
        {
            return Err(ModuleContractError::InvalidCharacters { kind });
        }
        previous_separator = is_separator;
    }
    if value.ends_with('-') {
        return Err(ModuleContractError::InvalidCharacters { kind });
    }
    Ok(())
}

fn validate_namespaced_id(value: &str) -> Result<(), ModuleContractError> {
    validate_length_and_non_empty(value, "namespaced identity")?;
    let mut segments = value.split('.');
    let Some(module_id) = segments.next() else {
        return Err(ModuleContractError::InvalidCharacters {
            kind: "namespaced identity",
        });
    };
    let Some(local_id) = segments.next() else {
        return Err(ModuleContractError::InvalidCharacters {
            kind: "namespaced identity",
        });
    };
    if segments.next().is_some() {
        return Err(ModuleContractError::InvalidCharacters {
            kind: "namespaced identity",
        });
    }
    validate_module_id(module_id)?;
    validate_local_id(local_id)
}

fn with_identity_kind(error: ModuleContractError, kind: &'static str) -> ModuleContractError {
    match error {
        ModuleContractError::EmptyValue { .. } => ModuleContractError::EmptyValue { kind },
        ModuleContractError::InvalidCharacters { .. } => {
            ModuleContractError::InvalidCharacters { kind }
        }
        ModuleContractError::TooLong { max, .. } => ModuleContractError::TooLong { kind, max },
    }
}

fn validate_version_token(value: &str, kind: &'static str) -> Result<(), ModuleContractError> {
    validate_length_and_non_empty(value, kind)?;
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || ".-+".contains(character))
    {
        return Err(ModuleContractError::InvalidCharacters { kind });
    }
    Ok(())
}

fn validate_length_and_non_empty(
    value: &str,
    kind: &'static str,
) -> Result<(), ModuleContractError> {
    if value.is_empty() {
        return Err(ModuleContractError::EmptyValue { kind });
    }
    if value.len() > MAX_IDENTIFIER_LENGTH {
        return Err(ModuleContractError::TooLong {
            kind,
            max: MAX_IDENTIFIER_LENGTH,
        });
    }
    Ok(())
}

fn validate_non_empty(value: &str, kind: &'static str) -> Result<(), ManifestValidationError> {
    if value.trim().is_empty() {
        return Err(ManifestValidationError::EmptyField { kind });
    }
    Ok(())
}

fn validate_namespace(value: &str) -> Result<(), ManifestValidationError> {
    validate_non_empty(value, "migration namespace")?;
    if !value.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || ".-_".contains(character)
    }) || value.starts_with('.')
        || value.starts_with('-')
        || value.starts_with('_')
        || value.ends_with('.')
        || value.ends_with('-')
        || value.ends_with('_')
    {
        return Err(ManifestValidationError::InvalidField {
            kind: "migration namespace with valid characters",
        });
    }
    Ok(())
}

fn validate_capabilities(
    required: &[PlatformCapabilityRequirement],
    optional: &[PlatformCapabilityRequirement],
) -> Result<(), ManifestValidationError> {
    let mut required_ids = BTreeSet::new();
    for capability in required {
        validate_non_empty(&capability.capability_id, "platform capability")?;
        validate_version_requirement(&capability.version_requirement)?;
        required_ids.insert(capability.capability_id.clone());
    }
    let mut optional_ids = BTreeSet::new();
    for capability in optional {
        validate_non_empty(&capability.capability_id, "platform capability")?;
        validate_version_requirement(&capability.version_requirement)?;
        if required_ids.contains(&capability.capability_id)
            || !optional_ids.insert(capability.capability_id.clone())
        {
            return Err(ManifestValidationError::DuplicatePlatformCapability {
                capability_id: capability.capability_id.clone(),
            });
        }
    }
    if required_ids.len() != required.len() {
        let duplicate = required
            .iter()
            .find(|capability| {
                required
                    .iter()
                    .filter(|other| other.capability_id == capability.capability_id)
                    .count()
                    > 1
            })
            .map_or_else(
                || "unknown".to_owned(),
                |capability| capability.capability_id.clone(),
            );
        return Err(ManifestValidationError::DuplicatePlatformCapability {
            capability_id: duplicate,
        });
    }
    Ok(())
}

fn validate_contracts(
    contracts: &[ContractDescriptor],
    kind: &'static str,
) -> Result<(), ManifestValidationError> {
    let mut identifiers = BTreeSet::new();
    for contract in contracts {
        validate_non_empty(&contract.contract_id, kind)?;
        validate_non_empty(&contract.version, "contract version")?;
        if !identifiers.insert(contract.contract_id.clone()) {
            return Err(ManifestValidationError::DuplicateIdentifier {
                kind,
                identifier: contract.contract_id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_resource_kinds(
    resource_kinds: &[ResourceKindDescriptor],
) -> Result<(), ManifestValidationError> {
    let mut identifiers = BTreeSet::new();
    for resource in resource_kinds {
        validate_non_empty(&resource.resource_kind, "resource kind")?;
        validate_non_empty(&resource.version, "resource kind version")?;
        if !identifiers.insert(resource.resource_kind.clone()) {
            return Err(ManifestValidationError::DuplicateIdentifier {
                kind: "resource kind",
                identifier: resource.resource_kind.clone(),
            });
        }
    }
    Ok(())
}

fn validate_contributions<T>(
    contributions: &[T],
    _kind: &'static str,
) -> Result<(), ManifestValidationError>
where
    T: ContributionIdentity,
{
    let mut identifiers = BTreeSet::new();
    for contribution in contributions {
        validate_non_empty(contribution.identifier(), "contribution identifier")?;
        validate_non_empty(contribution.version(), "contribution version")?;
        if !identifiers.insert(contribution.identifier().to_owned()) {
            return Err(ManifestValidationError::DuplicateIdentifier {
                kind: "contribution",
                identifier: contribution.identifier().to_owned(),
            });
        }
    }
    Ok(())
}

trait ContributionIdentity {
    fn identifier(&self) -> &str;
    fn version(&self) -> &str;
}

impl ContributionIdentity for SemanticContributionDescriptor {
    fn identifier(&self) -> &str {
        &self.semantic_id
    }

    fn version(&self) -> &str {
        &self.version
    }
}

impl ContributionIdentity for ContributionDescriptor {
    fn identifier(&self) -> &str {
        &self.contribution_id
    }

    fn version(&self) -> &str {
        &self.version
    }
}

fn validate_version_requirement(requirement: &str) -> Result<(), ManifestValidationError> {
    if requirement == "*" {
        return Ok(());
    }
    let candidate = requirement
        .strip_prefix('^')
        .or_else(|| requirement.strip_prefix('~'));
    let candidate = candidate.unwrap_or(requirement);
    if candidate.is_empty()
        || candidate.split('.').count() > 3
        || candidate
            .chars()
            .any(|character| !(character.is_ascii_digit() || character == '.'))
    {
        return Err(ManifestValidationError::InvalidVersionRequirement {
            requirement: requirement.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_id_rejects_uppercase_and_repeated_separators() {
        assert!(BusinessModuleId::new("Contract".to_owned()).is_err());
        assert!(BusinessModuleId::new("contract--management".to_owned()).is_err());
        assert!(BusinessModuleId::new("contract-management".to_owned()).is_ok());
    }

    #[test]
    fn manifest_rejects_duplicate_required_capabilities() {
        let module_id = BusinessModuleId::new("sales".to_owned());
        assert!(module_id.is_ok());
        let Ok(module_id) = module_id else { return };
        let version = BusinessModuleVersion::new("1.0.0".to_owned());
        assert!(version.is_ok());
        let Ok(version) = version else { return };
        let schema = ManifestSchemaVersion::new("business-module.manifest.v1".to_owned());
        assert!(schema.is_ok());
        let Ok(schema) = schema else { return };
        let manifest = BusinessModuleManifest {
            module_id,
            module_version: version,
            manifest_schema_version: schema,
            owned_bounded_contexts: vec!["sales".to_owned()],
            required_platform_capabilities: vec![
                PlatformCapabilityRequirement {
                    capability_id: "analytics".to_owned(),
                    version_requirement: "1.0.0".to_owned(),
                },
                PlatformCapabilityRequirement {
                    capability_id: "analytics".to_owned(),
                    version_requirement: "1.0.0".to_owned(),
                },
            ],
            optional_platform_capabilities: Vec::new(),
            published_commands: Vec::new(),
            published_queries: Vec::new(),
            published_events: Vec::new(),
            resource_kinds: Vec::new(),
            data_classification: vec![DataClassification::Internal],
            migration_namespace: "sales".to_owned(),
            semantic_contributions: Vec::new(),
            ui_contributions: Vec::new(),
            agent_tool_contributions: Vec::new(),
            dependencies: Vec::new(),
            compatibility: CompatibilityDescriptor::default(),
        };
        assert!(matches!(
            manifest.validate(),
            Err(ManifestValidationError::DuplicatePlatformCapability { .. })
        ));
    }

    #[test]
    fn manifest_rejects_unknown_schema_version() {
        let module_id = BusinessModuleId::new("sales".to_owned());
        let version = BusinessModuleVersion::new("1.0.0".to_owned());
        let schema = ManifestSchemaVersion::new("business-module.manifest.v2".to_owned());
        assert!(module_id.is_ok() && version.is_ok() && schema.is_ok());
        let (Ok(module_id), Ok(version), Ok(schema)) = (module_id, version, schema) else {
            return;
        };
        let manifest = BusinessModuleManifest {
            module_id,
            module_version: version,
            manifest_schema_version: schema,
            owned_bounded_contexts: vec!["sales".to_owned()],
            required_platform_capabilities: Vec::new(),
            optional_platform_capabilities: Vec::new(),
            published_commands: Vec::new(),
            published_queries: Vec::new(),
            published_events: Vec::new(),
            resource_kinds: Vec::new(),
            data_classification: vec![DataClassification::Internal],
            migration_namespace: "sales".to_owned(),
            semantic_contributions: Vec::new(),
            ui_contributions: Vec::new(),
            agent_tool_contributions: Vec::new(),
            dependencies: Vec::new(),
            compatibility: CompatibilityDescriptor::default(),
        };
        assert!(matches!(
            manifest.validate(),
            Err(ManifestValidationError::UnsupportedSchemaVersion { .. })
        ));
    }
}
