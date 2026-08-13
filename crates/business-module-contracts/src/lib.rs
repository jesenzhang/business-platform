//! Platform-neutral contracts for declaring an installable business module.
//!
//! The types in this crate describe ownership and published boundaries. They do
//! not load configuration, inspect a database, execute a plugin, or own any
//! business fact.

mod manifest;

pub use manifest::{
    ActionContribution, AgentCapabilityContribution, AgentCapabilityId, BusinessModuleId,
    BusinessModuleManifest, BusinessModuleVersion, CapabilityRequirementDescriptor,
    CapabilityRequirementId, CommandContribution, CompatibilityDescriptor, ContractDescriptor,
    ContributionDescriptor, ContributionId, DataClassification, DetailSectionContribution,
    DetailTabContribution, ExtensionPointId, ListViewContribution, ManifestSchemaVersion,
    ManifestValidationError, ModuleContractError, ModuleDataState, ModuleDependency,
    ModuleInstallationState, NamespacedId, NavigationContribution, PackageDigest,
    PlatformCapabilityRequirement, PolicyRequirementDescriptor, PolicyRequirementId,
    PublicContributionCatalog, PublicContributionTarget, PublicTargetKind, ResourceKindDescriptor,
    SemanticContributionDescriptor, TypedContributionSet, UiContributionId,
    BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION,
};
