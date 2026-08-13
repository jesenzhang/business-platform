//! Platform-neutral contracts for declaring an installable business module.
//!
//! The types in this crate describe ownership and published boundaries. They do
//! not load configuration, inspect a database, execute a plugin, or own any
//! business fact.

mod manifest;

pub use manifest::{
    AgentCapabilityId, BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion,
    CompatibilityDescriptor, ContractDescriptor, ContributionDescriptor, ContributionId,
    DataClassification, ExtensionPointId, ManifestSchemaVersion, ManifestValidationError,
    ModuleContractError, ModuleDataState, ModuleDependency, ModuleInstallationState, NamespacedId,
    PackageDigest, PlatformCapabilityRequirement, PolicyRequirementId, ResourceKindDescriptor,
    SemanticContributionDescriptor, UiContributionId, BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION,
};
