//! Pure, deterministic compilation of declared business application packages.
//!
//! This crate produces rebuildable evidence only. It does not register, load,
//! persist, execute, or discover business applications.

use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, CompatibilityDescriptor,
    ExtensionContribution, ExtensionPointId, ManifestValidationError, ModuleDataState,
    ModuleDependency, ModuleInstallationState, NamespacedId, PackageDigest,
    PublicContributionCatalog, PublicContributionTarget, PublicTargetKind,
    PublishedDependencyCatalog, PublishedDependencyReference, PublishedExtensionPoint,
    TypedContributionSet,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const BUSINESS_APPLICATION_PACKAGE_SCHEMA_VERSION: &str = "business-application.package.v1";

/// A read-only view of the declarations currently known by a registry.
/// Installation and data retention are deliberately separate state machines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentRegistrySnapshot {
    #[serde(default)]
    pub modules: Vec<CurrentModuleSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentModuleSnapshot {
    pub package: BusinessApplicationPackage,
    pub package_digest: PackageDigest,
    pub installation_state: business_module_contracts::ModuleInstallationState,
    pub data_state: business_module_contracts::ModuleDataState,
}

/// The compiled package set supplied to the pure planner.
pub type IncomingCompiledPackages = CompiledBusinessApplicationManifest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessApplicationPlan {
    pub changes: Vec<PackageChange>,
    pub diagnostics: Vec<PlanDiagnostic>,
    /// False means an unresolved conflict or blocked removal prevents apply.
    pub applicable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageChange {
    AddModule {
        module_id: BusinessModuleId,
        digest: PackageDigest,
    },
    UpgradeModule {
        module_id: BusinessModuleId,
        from: PackageDigest,
        to: PackageDigest,
    },
    DisableModule {
        module_id: BusinessModuleId,
    },
    EnableModule {
        module_id: BusinessModuleId,
    },
    RemoveModule {
        module_id: BusinessModuleId,
        data_retained: bool,
    },
    AddContribution {
        contribution_id: String,
        module_id: BusinessModuleId,
        digest: PackageDigest,
    },
    UpdateContribution {
        contribution_id: String,
        module_id: BusinessModuleId,
        from: PackageDigest,
        to: PackageDigest,
    },
    RemoveContribution {
        contribution_id: String,
        module_id: BusinessModuleId,
    },
    AddExtensionPoint {
        extension_point_id: ExtensionPointId,
        owner_module_id: BusinessModuleId,
        digest: PackageDigest,
    },
    RemoveExtensionPoint {
        extension_point_id: ExtensionPointId,
        owner_module_id: BusinessModuleId,
    },
    DependencyChange {
        module_id: BusinessModuleId,
        from: Vec<ModuleDependency>,
        to: Vec<ModuleDependency>,
    },
    CompatibilityChange {
        module_id: BusinessModuleId,
        from: CompatibilityDescriptor,
        to: CompatibilityDescriptor,
    },
    Conflict {
        module_id: Option<BusinessModuleId>,
        identifier: String,
    },
    BlockedRemoval {
        module_id: Option<BusinessModuleId>,
        identifier: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanDiagnostic {
    Conflict {
        module_id: Option<BusinessModuleId>,
        identifier: String,
        reason: String,
    },
    BlockedRemoval {
        module_id: Option<BusinessModuleId>,
        identifier: String,
        reason: String,
    },
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("incoming compiled package set is not canonical: {0}")]
    IncomingNotCanonical(String),
    #[error("planning declarations could not be compiled: {reason}")]
    PlanningCompilation { reason: String },
    #[error("duplicate module in {location}: '{identifier}'")]
    DuplicateModule {
        location: &'static str,
        identifier: String,
    },
    #[error("duplicate plan component: '{identifier}'")]
    DuplicateComponent { identifier: String },
    #[error("failed to fingerprint plan component: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid package digest")]
    Digest,
}

/// A typed package declaration and its optional published extension contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessApplicationPackage {
    pub manifest: BusinessModuleManifest,
    #[serde(default)]
    pub contributions: TypedContributionSet,
    #[serde(default)]
    pub extension_points: Vec<PublishedExtensionPoint>,
    #[serde(default)]
    pub extension_contributions: Vec<ExtensionContribution>,
}

/// Input to a compilation. Installed versions are evidence supplied by a host;
/// they do not become registry state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusinessApplicationCompilerInput {
    pub platform_version: String,
    pub packages: Vec<BusinessApplicationPackage>,
    #[serde(default)]
    pub installed_versions: BTreeMap<BusinessModuleId, BusinessModuleVersion>,
    /// Desired installation states for incoming modules. An omitted module is
    /// planned for removal; `Uninstalled` is therefore not a valid incoming
    /// desired state.
    #[serde(default)]
    pub desired_installation_states: BTreeMap<BusinessModuleId, ModuleInstallationState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledBusinessApplicationManifest {
    pub schema_version: String,
    pub platform_version: String,
    pub packages: Vec<BusinessApplicationPackage>,
    #[serde(default)]
    pub desired_installation_states: BTreeMap<BusinessModuleId, ModuleInstallationState>,
    /// SHA-256 of `canonical_json`; this field is excluded from that payload.
    pub package_digest: PackageDigest,
    #[serde(skip)]
    canonical_json: Vec<u8>,
}

impl CompiledBusinessApplicationManifest {
    #[must_use]
    /// Canonical digest payload, excluding the self-referential digest field.
    pub fn canonical_json(&self) -> &[u8] {
        &self.canonical_json
    }

    #[must_use]
    pub fn package_digest(&self) -> &PackageDigest {
        &self.package_digest
    }
}

impl<'de> Deserialize<'de> for CompiledBusinessApplicationManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SerializedManifest {
            schema_version: String,
            platform_version: String,
            packages: Vec<BusinessApplicationPackage>,
            #[serde(default)]
            desired_installation_states: BTreeMap<BusinessModuleId, ModuleInstallationState>,
            package_digest: PackageDigest,
        }

        let serialized = SerializedManifest::deserialize(deserializer)?;
        let mut manifest = Self {
            schema_version: serialized.schema_version,
            platform_version: serialized.platform_version,
            packages: serialized.packages,
            desired_installation_states: serialized.desired_installation_states,
            package_digest: serialized.package_digest,
            canonical_json: Vec::new(),
        };
        manifest.canonical_json = canonical_bytes(&manifest).map_err(serde::de::Error::custom)?;
        let expected_digest = format!("{:x}", Sha256::digest(&manifest.canonical_json));
        if manifest.package_digest.as_str() != expected_digest {
            return Err(serde::de::Error::custom(
                "package digest does not match canonical bytes",
            ));
        }

        let compiled = compile(BusinessApplicationCompilerInput {
            platform_version: manifest.platform_version.clone(),
            packages: manifest.packages.clone(),
            installed_versions: BTreeMap::new(),
            desired_installation_states: manifest.desired_installation_states.clone(),
        })
        .map_err(|error| {
            serde::de::Error::custom(format!("compiled manifest validation failed: {error}"))
        })?;
        if manifest != compiled {
            return Err(serde::de::Error::custom(
                "compiled manifest is not canonical compiler output",
            ));
        }
        Ok(manifest)
    }
}

#[derive(Debug, Error)]
pub enum CompilationError {
    #[error("manifest validation failed: {0}")]
    Manifest(#[from] ManifestValidationError),
    #[error("invalid SemVer in {field}: '{value}'")]
    InvalidVersion { field: &'static str, value: String },
    #[error("invalid SemVer requirement in {field}: '{value}'")]
    InvalidRequirement { field: &'static str, value: String },
    #[error("platform version '{version}' is incompatible with package '{module_id}'")]
    IncompatiblePlatform { module_id: String, version: String },
    #[error("unknown module dependency '{module_id}'")]
    UnknownDependency { module_id: String },
    #[error("module dependency '{module_id}' is incompatible with '{requirement}'")]
    IncompatibleDependency {
        module_id: String,
        requirement: String,
    },
    #[error("module dependency graph contains a cycle")]
    DependencyCycle,
    #[error("package downgrade for '{module_id}': installed {installed}, incoming {incoming}")]
    Downgrade {
        module_id: String,
        installed: String,
        incoming: String,
    },
    #[error("duplicate {kind} '{identifier}'")]
    Duplicate {
        kind: &'static str,
        identifier: String,
    },
    #[error("ownership collision for {kind} '{identifier}'")]
    OwnershipCollision {
        kind: &'static str,
        identifier: String,
    },
    #[error("extension contribution targets unknown extension point '{identifier}'")]
    UnknownExtension { identifier: String },
    #[error("desired lifecycle state references unknown incoming module '{module_id}'")]
    UnknownDesiredLifecycleModule { module_id: String },
    #[error("incoming module '{module_id}' cannot have desired lifecycle state 'uninstalled'")]
    InvalidDesiredLifecycle { module_id: String },
    #[error("canonical manifest serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid package digest")]
    Digest,
}

#[derive(Debug, Clone, Default)]
struct PlanningCompilationContext {
    module_versions: BTreeMap<String, Version>,
    public_catalog: PublicContributionCatalog,
    dependency_catalog: PublishedDependencyCatalog,
    extension_points: BTreeMap<ExtensionPointId, PublishedExtensionPoint>,
}

#[allow(clippy::too_many_lines)]
pub fn compile(
    input: BusinessApplicationCompilerInput,
) -> Result<CompiledBusinessApplicationManifest, CompilationError> {
    compile_with_context(input, &PlanningCompilationContext::default())
}

/// Builds a plan from raw desired declarations while resolving references that
/// exist in the current registry as transition context. This seam is needed
/// for a removal such as "owner omitted, consumer contribution still present":
/// the desired state is intentionally not a standalone installable package set
/// until the blocked transition is resolved. Unknown references that exist in
/// neither input nor current state still fail closed.
pub fn dry_plan_from_declarations(
    current: &CurrentRegistrySnapshot,
    input: BusinessApplicationCompilerInput,
) -> Result<BusinessApplicationPlan, PlanError> {
    let context = planning_compilation_context(current)?;
    let compiled =
        compile_with_context(input, &context).map_err(|error| PlanError::PlanningCompilation {
            reason: error.to_string(),
        })?;
    dry_plan(current, &compiled)
}

#[allow(clippy::too_many_lines)]
fn compile_with_context(
    mut input: BusinessApplicationCompilerInput,
    context: &PlanningCompilationContext,
) -> Result<CompiledBusinessApplicationManifest, CompilationError> {
    let platform = parse_version("platform version", &input.platform_version)?;
    let mut modules = BTreeMap::new();
    let mut available_modules = context.module_versions.clone();
    let mut owned_contexts = BTreeMap::<String, String>::new();
    let mut public_ids = BTreeMap::<String, String>::new();

    for package in &input.packages {
        package.manifest.validate()?;
    }
    let public_catalog = merged_public_catalog(&input.packages, &context.public_catalog)?;
    let dependency_catalog =
        merged_dependency_catalog(&input.packages, &context.dependency_catalog);

    for package in &mut input.packages {
        let module_id = package.manifest.module_id.to_string();
        let module_version =
            parse_version("module version", package.manifest.module_version.as_str())?;
        if modules
            .insert(module_id.clone(), module_version.clone())
            .is_some()
        {
            return Err(CompilationError::Duplicate {
                kind: "module",
                identifier: module_id,
            });
        }
        if let Some(installed) = input.installed_versions.get(&package.manifest.module_id) {
            let installed_version = parse_version("installed module version", installed.as_str())?;
            if module_version < installed_version {
                return Err(CompilationError::Downgrade {
                    module_id,
                    installed: installed.to_string(),
                    incoming: module_version.to_string(),
                });
            }
        }
        available_modules.insert(module_id.clone(), module_version.clone());
        validate_platform(&package.manifest, &platform)?;
        package
            .contributions
            .validate(&package.manifest.module_id, &public_catalog)?;
        validate_typed_contributions(&package.contributions)?;
        validate_manifest_versions(&package.manifest)?;
        register_manifest_ids(package, &mut owned_contexts, &mut public_ids)?;
        register_typed_contribution_ids(package, &mut public_ids)?;
        for point in &package.extension_points {
            if point.owner_module_id != package.manifest.module_id {
                return Err(CompilationError::OwnershipCollision {
                    kind: "extension point",
                    identifier: point.extension_point_id.to_string(),
                });
            }
            register_identity(
                point.extension_point_id.to_string(),
                package.manifest.module_id.as_str(),
                &mut public_ids,
            )?;
            point.validate_against_catalog(&dependency_catalog)?;
            parse_version(
                "extension point contract version",
                point.contract_version.as_str(),
            )?;
            parse_version("extension point schema version", &point.schema_version)?;
        }
        for contribution in &package.extension_contributions {
            if contribution.consumer_module_id != package.manifest.module_id {
                return Err(CompilationError::OwnershipCollision {
                    kind: "extension contribution",
                    identifier: contribution.contribution_id.to_string(),
                });
            }
            parse_version(
                "extension contribution contract version",
                contribution.expected_contract_version.as_str(),
            )?;
            register_identity(
                contribution.contribution_id.to_string(),
                package.manifest.module_id.as_str(),
                &mut public_ids,
            )?;
        }
    }
    for (module_id, state) in &input.desired_installation_states {
        if !modules.contains_key(module_id.as_str()) {
            return Err(CompilationError::UnknownDesiredLifecycleModule {
                module_id: module_id.to_string(),
            });
        }
        if *state == ModuleInstallationState::Uninstalled {
            return Err(CompilationError::InvalidDesiredLifecycle {
                module_id: module_id.to_string(),
            });
        }
    }
    for package in &input.packages {
        for dependency in &package.manifest.dependencies {
            let Some(version) = available_modules.get(dependency.module_id.as_str()) else {
                return Err(CompilationError::UnknownDependency {
                    module_id: dependency.module_id.to_string(),
                });
            };
            let requirement =
                parse_requirement("module dependency", &dependency.version_requirement)?;
            if !requirement.matches(version) {
                return Err(CompilationError::IncompatibleDependency {
                    module_id: dependency.module_id.to_string(),
                    requirement: dependency.version_requirement.clone(),
                });
            }
        }
        for contribution in &package.extension_contributions {
            let point = input
                .packages
                .iter()
                .flat_map(|p| p.extension_points.iter())
                .find(|p| p.extension_point_id == contribution.target_extension_point_id)
                .or_else(|| {
                    context
                        .extension_points
                        .get(&contribution.target_extension_point_id)
                })
                .ok_or_else(|| CompilationError::UnknownExtension {
                    identifier: contribution.target_extension_point_id.to_string(),
                })?;
            if contribution.contribution_id.module_id() != contribution.consumer_module_id.as_str()
            {
                return Err(CompilationError::OwnershipCollision {
                    kind: "extension contribution",
                    identifier: contribution.contribution_id.to_string(),
                });
            }
            contribution.validate_against_catalog(point, &dependency_catalog)?;
        }
    }
    detect_cycles(&input.packages)?;
    normalize(&mut input.packages);
    input
        .packages
        .sort_by_key(|p| p.manifest.module_id.to_string());
    let mut compiled = CompiledBusinessApplicationManifest {
        schema_version: BUSINESS_APPLICATION_PACKAGE_SCHEMA_VERSION.to_owned(),
        platform_version: platform.to_string(),
        packages: input.packages,
        desired_installation_states: input.desired_installation_states,
        package_digest: PackageDigest::new("0".repeat(64)).map_err(|_| CompilationError::Digest)?,
        canonical_json: Vec::new(),
    };
    let bytes = canonical_bytes(&compiled)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    compiled.package_digest = PackageDigest::new(digest).map_err(|_| CompilationError::Digest)?;
    compiled.canonical_json = bytes;
    Ok(compiled)
}

fn planning_compilation_context(
    current: &CurrentRegistrySnapshot,
) -> Result<PlanningCompilationContext, PlanError> {
    let mut context = PlanningCompilationContext::default();
    let mut current_packages = Vec::with_capacity(current.modules.len());
    for snapshot in &current.modules {
        let module_id = snapshot.package.manifest.module_id.clone();
        if context
            .module_versions
            .insert(
                module_id.to_string(),
                Version::parse(snapshot.package.manifest.module_version.as_str()).map_err(
                    |error| PlanError::PlanningCompilation {
                        reason: format!("invalid current module version: {error}"),
                    },
                )?,
            )
            .is_some()
        {
            return Err(PlanError::DuplicateModule {
                location: "current registry snapshot",
                identifier: module_id.to_string(),
            });
        }
        for point in &snapshot.package.extension_points {
            if context
                .extension_points
                .insert(point.extension_point_id.clone(), point.clone())
                .is_some()
            {
                return Err(PlanError::DuplicateComponent {
                    identifier: point.extension_point_id.to_string(),
                });
            }
        }
        current_packages.push(snapshot.package.clone());
    }
    context.public_catalog =
        public_catalog(&current_packages).map_err(|error| PlanError::PlanningCompilation {
            reason: error.to_string(),
        })?;
    context.dependency_catalog = dependency_catalog(&current_packages);
    Ok(context)
}

fn merged_public_catalog(
    packages: &[BusinessApplicationPackage],
    external: &PublicContributionCatalog,
) -> Result<PublicContributionCatalog, CompilationError> {
    let incoming = public_catalog(packages)?;
    let mut public_targets = external.public_targets.clone();
    public_targets.extend(incoming.public_targets);
    public_targets.sort_by_key(|target| serde_json::to_string(target).unwrap_or_default());
    public_targets.dedup();
    Ok(PublicContributionCatalog { public_targets })
}

fn merged_dependency_catalog(
    packages: &[BusinessApplicationPackage],
    external: &PublishedDependencyCatalog,
) -> PublishedDependencyCatalog {
    let incoming = dependency_catalog(packages);
    let mut public_dependencies = external.public_dependencies.clone();
    public_dependencies.extend(incoming.public_dependencies);
    public_dependencies
        .sort_by_key(|dependency| serde_json::to_string(dependency).unwrap_or_default());
    public_dependencies.dedup();
    PublishedDependencyCatalog {
        public_dependencies,
    }
}

/// Builds a deterministic, side-effect-free plan. This function only compares
/// declarations and registry evidence; it never applies registration changes
/// and never emits a data purge operation.
#[allow(clippy::too_many_lines)]
pub fn dry_plan(
    current: &CurrentRegistrySnapshot,
    incoming: &IncomingCompiledPackages,
) -> Result<BusinessApplicationPlan, PlanError> {
    let expected_canonical = canonical_bytes(incoming)?;
    if expected_canonical != incoming.canonical_json() {
        return Err(PlanError::IncomingNotCanonical(
            "canonical bytes do not match compiled fields".to_owned(),
        ));
    }
    let expected_digest = format!("{:x}", Sha256::digest(&expected_canonical));
    if incoming.package_digest().as_str() != expected_digest {
        return Err(PlanError::IncomingNotCanonical(
            "package digest does not match canonical bytes".to_owned(),
        ));
    }

    let mut current_modules = BTreeMap::new();
    for item in &current.modules {
        let id = item.package.manifest.module_id.clone();
        if current_modules.insert(id.clone(), item).is_some() {
            return Err(PlanError::DuplicateModule {
                location: "current registry snapshot",
                identifier: id.to_string(),
            });
        }
    }
    let mut incoming_modules = BTreeMap::new();
    for package in &incoming.packages {
        let id = package.manifest.module_id.clone();
        if incoming_modules.insert(id.clone(), package).is_some() {
            return Err(PlanError::DuplicateModule {
                location: "incoming compiled package set",
                identifier: id.to_string(),
            });
        }
    }

    let current_packages: Vec<_> = current
        .modules
        .iter()
        .map(|snapshot| snapshot.package.clone())
        .collect();
    let current_public_targets = public_catalog(&current_packages)
        .map_err(|error| PlanError::PlanningCompilation {
            reason: error.to_string(),
        })?
        .public_targets;
    let incoming_public_targets = public_catalog(&incoming.packages)
        .map_err(|error| PlanError::PlanningCompilation {
            reason: error.to_string(),
        })?
        .public_targets;
    let current_public_capabilities = public_capability_dependencies(&current_packages);
    let incoming_public_capabilities = public_capability_dependencies(&incoming.packages);
    let removed_public_targets = current_public_targets
        .into_iter()
        .filter(|target| !incoming_public_targets.contains(target))
        .collect::<Vec<_>>();
    let removed_public_capabilities = current_public_capabilities
        .into_iter()
        .filter(|capability| !incoming_public_capabilities.contains(capability))
        .collect::<Vec<_>>();
    let mut diagnostics = Vec::new();
    let mut changes = Vec::new();
    let mut blocked_public_targets = BTreeSet::new();
    let mut blocked_public_capabilities = BTreeSet::new();
    for target in &removed_public_targets {
        if push_public_target_blockers(target, &incoming_modules, &mut changes, &mut diagnostics) {
            blocked_public_targets.insert(public_target_key(target));
        }
    }
    for capability in &removed_public_capabilities {
        if push_public_capability_blockers(
            capability,
            &incoming_modules,
            &mut changes,
            &mut diagnostics,
        ) {
            blocked_public_capabilities.insert(stable_json_key(capability));
        }
    }

    let mut incoming_digests = BTreeMap::new();
    for (id, package) in &incoming_modules {
        incoming_digests.insert(id.clone(), package_digest(package)?);
    }
    for (id, package) in &incoming_modules {
        let digest =
            incoming_digests
                .get(id)
                .cloned()
                .ok_or_else(|| PlanError::DuplicateModule {
                    location: "incoming compiled package set",
                    identifier: id.to_string(),
                })?;
        match current_modules.get(id) {
            None => {
                changes.push(PackageChange::AddModule {
                    module_id: id.clone(),
                    digest,
                });
                if let Some(state) = incoming.desired_installation_states.get(id) {
                    if *state == ModuleInstallationState::Disabled {
                        changes.push(PackageChange::DisableModule {
                            module_id: id.clone(),
                        });
                    }
                }
            }
            Some(existing) => {
                let old_digest = existing.package_digest.clone();
                if old_digest != digest {
                    let old_version =
                        Version::parse(existing.package.manifest.module_version.as_str());
                    let new_version = Version::parse(package.manifest.module_version.as_str());
                    if matches!((old_version, new_version), (Ok(old), Ok(new)) if new < old) {
                        push_conflict(
                            &mut changes,
                            &mut diagnostics,
                            Some(id.clone()),
                            id.to_string(),
                            "module downgrade is not allowed",
                        );
                    } else {
                        changes.push(PackageChange::UpgradeModule {
                            module_id: id.clone(),
                            from: old_digest,
                            to: digest,
                        });
                    }
                }
                if let Some(desired) = incoming.desired_installation_states.get(id) {
                    match (*desired, existing.installation_state) {
                        (
                            ModuleInstallationState::Disabled,
                            ModuleInstallationState::Enabled | ModuleInstallationState::Installed,
                        ) => {
                            changes.push(PackageChange::DisableModule {
                                module_id: id.clone(),
                            });
                        }
                        (
                            ModuleInstallationState::Enabled,
                            ModuleInstallationState::Disabled | ModuleInstallationState::Installed,
                        ) => {
                            changes.push(PackageChange::EnableModule {
                                module_id: id.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    for (id, existing) in &current_modules {
        if !incoming_modules.contains_key(id) {
            let mut blocked = removed_public_targets.iter().any(|target| {
                target.owner_module_id == *id
                    && blocked_public_targets.contains(&public_target_key(target))
            });
            if !blocked {
                blocked = removed_public_capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        PublishedDependencyReference::PublicCapability {
                            owner_module_id, ..
                        } if owner_module_id == id
                    ) && blocked_public_capabilities.contains(&stable_json_key(capability))
                });
            }
            for (dependent_id, dependent) in &incoming_modules {
                if dependent_id != id
                    && dependent
                        .manifest
                        .dependencies
                        .iter()
                        .any(|d| d.module_id == *id)
                {
                    blocked = true;
                    let reason = format!("live dependency from {dependent_id}");
                    push_blocked(
                        &mut changes,
                        &mut diagnostics,
                        Some(id.clone()),
                        id.to_string(),
                        reason,
                    );
                }
            }
            if !blocked {
                let active_consumer =
                    incoming_has_extension_consumer(&incoming_modules, |contribution| {
                        contribution.target_extension_point_id.module_id() == id.as_str()
                    });
                if active_consumer {
                    push_blocked(
                        &mut changes,
                        &mut diagnostics,
                        Some(id.clone()),
                        id.to_string(),
                        "active extension consumer remains".to_owned(),
                    );
                    blocked = true;
                }
            }
            if !blocked {
                changes.push(PackageChange::RemoveModule {
                    module_id: id.clone(),
                    data_retained: existing.data_state != ModuleDataState::Purged,
                });
            }
        }
    }

    let mut incoming_entries = Vec::with_capacity(incoming_modules.len());
    for (id, package) in &incoming_modules {
        let digest = incoming_digests
            .get(id)
            .ok_or_else(|| PlanError::DuplicateModule {
                location: "incoming compiled package set",
                identifier: id.to_string(),
            })?;
        incoming_entries.push((id, *package, digest));
    }

    let current_components = all_components(
        current_modules
            .iter()
            .map(|(id, snapshot)| (id, &snapshot.package)),
    )?;
    let incoming_components =
        all_components(incoming_modules.iter().map(|(id, package)| (id, *package)))?;
    for (key, (module_id, digest)) in &incoming_components {
        match current_components.get(key) {
            None => changes.push(PackageChange::AddContribution {
                contribution_id: key.clone(),
                module_id: module_id.clone(),
                digest: digest.clone(),
            }),
            Some((_, old_digest)) if old_digest != digest => {
                changes.push(PackageChange::UpdateContribution {
                    contribution_id: key.clone(),
                    module_id: module_id.clone(),
                    from: old_digest.clone(),
                    to: digest.clone(),
                });
            }
            _ => {}
        }
    }
    for (key, (module_id, _)) in &current_components {
        if !incoming_components.contains_key(key) && incoming_modules.contains_key(module_id) {
            let public_target_blocked = removed_public_targets.iter().any(|target| {
                target.owner_module_id == *module_id
                    && public_target_component_id(target) == key.as_str()
                    && blocked_public_targets.contains(&public_target_key(target))
            });
            let capability_blocked = removed_public_capabilities.iter().any(|capability| {
                public_capability_id(capability).as_deref() == Some(key.as_str())
                    && blocked_public_capabilities.contains(&stable_json_key(capability))
            });
            let target_blocked = public_target_blocked || capability_blocked;
            if !target_blocked {
                changes.push(PackageChange::RemoveContribution {
                    contribution_id: key.clone(),
                    module_id: module_id.clone(),
                });
            }
        }
    }

    let current_points = extension_points(
        current_modules
            .iter()
            .map(|(id, snapshot)| (id, &snapshot.package, &snapshot.package_digest)),
    );
    let incoming_points = extension_points(incoming_entries);
    for (id, (owner, digest)) in &incoming_points {
        if !current_points.contains_key(id) {
            changes.push(PackageChange::AddExtensionPoint {
                extension_point_id: id.clone(),
                owner_module_id: owner.clone(),
                digest: digest.clone(),
            });
        }
    }
    for (id, (owner, _)) in &current_points {
        if !incoming_points.contains_key(id) && incoming_modules.contains_key(owner) {
            let live = incoming_has_extension_consumer(&incoming_modules, |contribution| {
                &contribution.target_extension_point_id == id
            });
            if live {
                push_blocked(
                    &mut changes,
                    &mut diagnostics,
                    Some(owner.clone()),
                    id.to_string(),
                    "active extension consumer remains".to_owned(),
                );
            } else {
                changes.push(PackageChange::RemoveExtensionPoint {
                    extension_point_id: id.clone(),
                    owner_module_id: owner.clone(),
                });
            }
        }
    }
    for id in incoming_modules
        .keys()
        .chain(current_modules.keys())
        .collect::<BTreeSet<_>>()
    {
        if let (Some(before), Some(after)) = (current_modules.get(id), incoming_modules.get(id)) {
            if before.package.manifest.dependencies != after.manifest.dependencies {
                changes.push(PackageChange::DependencyChange {
                    module_id: (*id).clone(),
                    from: sorted_dependencies(&before.package.manifest.dependencies),
                    to: sorted_dependencies(&after.manifest.dependencies),
                });
            }
            if before.package.manifest.compatibility != after.manifest.compatibility {
                changes.push(PackageChange::CompatibilityChange {
                    module_id: (*id).clone(),
                    from: before.package.manifest.compatibility.clone(),
                    to: after.manifest.compatibility.clone(),
                });
            }
        }
    }
    changes.sort_by_key(stable_change_key);
    diagnostics.sort_by_key(stable_diagnostic_key);
    Ok(BusinessApplicationPlan {
        applicable: diagnostics.is_empty(),
        changes,
        diagnostics,
    })
}

fn incoming_has_extension_consumer(
    incoming_modules: &BTreeMap<BusinessModuleId, &BusinessApplicationPackage>,
    target: impl Fn(&ExtensionContribution) -> bool,
) -> bool {
    incoming_modules.values().any(|package| {
        package.extension_contributions.iter().any(|contribution| {
            contribution.consumer_module_id == package.manifest.module_id && target(contribution)
        })
    })
}

fn public_capability_dependencies(
    packages: &[BusinessApplicationPackage],
) -> Vec<PublishedDependencyReference> {
    dependency_catalog(packages)
        .public_dependencies
        .into_iter()
        .filter(|dependency| {
            matches!(
                dependency,
                PublishedDependencyReference::PublicCapability { .. }
            )
        })
        .collect()
}

fn push_public_capability_blockers(
    capability: &PublishedDependencyReference,
    incoming_modules: &BTreeMap<BusinessModuleId, &BusinessApplicationPackage>,
    changes: &mut Vec<PackageChange>,
    diagnostics: &mut Vec<PlanDiagnostic>,
) -> bool {
    let mut consumers = Vec::new();
    for package in incoming_modules.values() {
        for point in &package.extension_points {
            if point
                .dependency_ids
                .iter()
                .any(|dependency| dependency == capability)
            {
                consumers.push((
                    package.manifest.module_id.clone(),
                    point.extension_point_id.to_string(),
                ));
            }
        }
    }
    consumers.sort();
    let has_consumers = !consumers.is_empty();
    for (consumer_module_id, consumer_id) in consumers {
        push_blocked(
            changes,
            diagnostics,
            Some(consumer_module_id.clone()),
            public_capability_id(capability).unwrap_or_else(|| stable_json_key(capability)),
            format!(
                "retained consumer {consumer_module_id} ({consumer_id}) uses removed public capability"
            ),
        );
    }
    has_consumers
}

fn public_capability_id(dependency: &PublishedDependencyReference) -> Option<String> {
    match dependency {
        PublishedDependencyReference::PublicCapability {
            owner_module_id,
            capability_id,
            ..
        } => Some(format!("{owner_module_id}.{capability_id}")),
        _ => None,
    }
}

fn push_public_target_blockers(
    target: &PublicContributionTarget,
    incoming_modules: &BTreeMap<BusinessModuleId, &BusinessApplicationPackage>,
    changes: &mut Vec<PackageChange>,
    diagnostics: &mut Vec<PlanDiagnostic>,
) -> bool {
    let mut consumers = Vec::new();
    for package in incoming_modules.values() {
        macro_rules! collect_typed_consumers {
            ($items:expr) => {
                for item in $items {
                    if &item.target == target {
                        consumers.push((
                            package.manifest.module_id.clone(),
                            item.contribution_id.to_string(),
                        ));
                    }
                }
            };
        }
        collect_typed_consumers!(&package.contributions.navigation);
        collect_typed_consumers!(&package.contributions.list_views);
        collect_typed_consumers!(&package.contributions.detail_sections);
        collect_typed_consumers!(&package.contributions.detail_tabs);
        collect_typed_consumers!(&package.contributions.actions);
        collect_typed_consumers!(&package.contributions.commands);
        collect_typed_consumers!(&package.contributions.agent_capabilities);
        for point in &package.extension_points {
            if point
                .dependency_ids
                .iter()
                .any(|dependency| public_dependency_matches_target(dependency, target))
            {
                consumers.push((
                    package.manifest.module_id.clone(),
                    point.extension_point_id.to_string(),
                ));
            }
        }
    }
    consumers.sort();
    let has_consumers = !consumers.is_empty();
    for (consumer_module_id, consumer_id) in consumers {
        push_blocked(
            changes,
            diagnostics,
            Some(consumer_module_id.clone()),
            public_target_id(target),
            format!(
                "retained consumer {consumer_module_id} ({consumer_id}) uses removed public target"
            ),
        );
    }
    has_consumers
}

fn public_dependency_matches_target(
    dependency: &PublishedDependencyReference,
    target: &PublicContributionTarget,
) -> bool {
    match (&target.target, dependency) {
        (
            PublicTargetKind::Resource { resource_kind },
            PublishedDependencyReference::PublicResource {
                owner_module_id,
                resource_kind: dependency_id,
                version,
            },
        ) => {
            owner_module_id == &target.owner_module_id
                && dependency_id == resource_kind
                && version == &target.version
        }
        (
            PublicTargetKind::Query { query_id },
            PublishedDependencyReference::PublicQuery {
                owner_module_id,
                query_id: dependency_id,
                version,
            },
        ) => {
            owner_module_id == &target.owner_module_id
                && dependency_id == query_id
                && version == &target.version
        }
        (
            PublicTargetKind::Command { command_id },
            PublishedDependencyReference::PublicCommand {
                owner_module_id,
                command_id: dependency_id,
                version,
            },
        ) => {
            owner_module_id == &target.owner_module_id
                && dependency_id == command_id
                && version == &target.version
        }
        _ => false,
    }
}

fn public_target_id(target: &PublicContributionTarget) -> String {
    format!(
        "{}.{}",
        target.owner_module_id,
        public_target_component_id(target)
    )
}

fn public_target_component_id(target: &PublicContributionTarget) -> &str {
    match &target.target {
        PublicTargetKind::Resource { resource_kind } => resource_kind,
        PublicTargetKind::Query { query_id } => query_id,
        PublicTargetKind::Command { command_id } => command_id,
    }
}

fn public_target_key(target: &PublicContributionTarget) -> String {
    stable_json_key(target)
}

fn push_conflict(
    changes: &mut Vec<PackageChange>,
    diagnostics: &mut Vec<PlanDiagnostic>,
    module_id: Option<BusinessModuleId>,
    identifier: String,
    reason: &str,
) {
    changes.push(PackageChange::Conflict {
        module_id: module_id.clone(),
        identifier: identifier.clone(),
    });
    diagnostics.push(PlanDiagnostic::Conflict {
        module_id,
        identifier,
        reason: reason.to_owned(),
    });
}
fn push_blocked(
    changes: &mut Vec<PackageChange>,
    diagnostics: &mut Vec<PlanDiagnostic>,
    module_id: Option<BusinessModuleId>,
    identifier: String,
    reason: String,
) {
    changes.push(PackageChange::BlockedRemoval {
        module_id: module_id.clone(),
        identifier: identifier.clone(),
        reason: reason.clone(),
    });
    diagnostics.push(PlanDiagnostic::BlockedRemoval {
        module_id,
        identifier,
        reason,
    });
}
fn sorted_dependencies(items: &[ModuleDependency]) -> Vec<ModuleDependency> {
    let mut result = items.to_vec();
    result.sort_by_key(|d| (d.module_id.clone(), d.version_requirement.clone()));
    result
}
fn all_components<'a>(
    modules: impl IntoIterator<Item = (&'a BusinessModuleId, &'a BusinessApplicationPackage)>,
) -> Result<BTreeMap<String, (BusinessModuleId, PackageDigest)>, PlanError> {
    modules
        .into_iter()
        .try_fold(BTreeMap::new(), |mut result, (module_id, item)| {
            let mut normalized = item.clone();
            normalize(std::slice::from_mut(&mut normalized));
            for (key, digest) in component_fingerprints(&normalized)? {
                if result
                    .insert(key.clone(), (module_id.clone(), digest))
                    .is_some()
                {
                    return Err(PlanError::DuplicateComponent { identifier: key });
                }
            }
            Ok(result)
        })
}

fn package_digest(package: &BusinessApplicationPackage) -> Result<PackageDigest, PlanError> {
    digest_json(package)
}

fn digest_json<T: Serialize>(value: &T) -> Result<PackageDigest, PlanError> {
    let bytes = serde_json::to_vec(value)?;
    PackageDigest::new(format!("{:x}", Sha256::digest(bytes))).map_err(|_| PlanError::Digest)
}
fn component_fingerprints(
    package: &BusinessApplicationPackage,
) -> Result<Vec<(String, PackageDigest)>, PlanError> {
    let mut fingerprints = Vec::new();
    macro_rules! add_string_components {
        ($items:expr, $field:ident) => {
            for item in $items {
                fingerprints.push((item.$field.clone(), digest_json(item)?));
            }
        };
    }
    macro_rules! add_display_components {
        ($items:expr, $field:ident) => {
            for item in $items {
                fingerprints.push((item.$field.to_string(), digest_json(item)?));
            }
        };
    }

    add_string_components!(package.manifest.published_commands.iter(), contract_id);
    add_string_components!(package.manifest.published_queries.iter(), contract_id);
    add_string_components!(package.manifest.published_events.iter(), contract_id);
    add_string_components!(package.manifest.resource_kinds.iter(), resource_kind);
    add_string_components!(package.manifest.semantic_contributions.iter(), semantic_id);
    add_string_components!(package.manifest.ui_contributions.iter(), contribution_id);
    add_string_components!(
        package.manifest.agent_tool_contributions.iter(),
        contribution_id
    );
    add_display_components!(package.extension_contributions.iter(), contribution_id);
    add_display_components!(package.contributions.navigation.iter(), contribution_id);
    add_display_components!(package.contributions.list_views.iter(), contribution_id);
    add_display_components!(
        package.contributions.detail_sections.iter(),
        contribution_id
    );
    add_display_components!(package.contributions.detail_tabs.iter(), contribution_id);
    add_display_components!(package.contributions.actions.iter(), contribution_id);
    add_display_components!(package.contributions.commands.iter(), contribution_id);
    add_display_components!(
        package.contributions.agent_capabilities.iter(),
        contribution_id
    );
    add_display_components!(
        package.contributions.policy_requirements.iter(),
        requirement_id
    );
    add_display_components!(
        package.contributions.capability_requirements.iter(),
        requirement_id
    );
    Ok(fingerprints)
}
fn extension_points<'a>(
    modules: impl IntoIterator<
        Item = (
            &'a BusinessModuleId,
            &'a BusinessApplicationPackage,
            &'a PackageDigest,
        ),
    >,
) -> BTreeMap<ExtensionPointId, (BusinessModuleId, PackageDigest)> {
    modules
        .into_iter()
        .flat_map(|(_, package, digest)| {
            package.extension_points.iter().map(move |p| {
                (
                    p.extension_point_id.clone(),
                    (p.owner_module_id.clone(), digest.clone()),
                )
            })
        })
        .collect()
}
fn stable_change_key(change: &PackageChange) -> String {
    serde_json::to_string(change).unwrap_or_default()
}
fn stable_diagnostic_key(diagnostic: &PlanDiagnostic) -> String {
    serde_json::to_string(diagnostic).unwrap_or_default()
}

fn parse_version(field: &'static str, value: &str) -> Result<Version, CompilationError> {
    Version::parse(value).map_err(|_| CompilationError::InvalidVersion {
        field,
        value: value.to_owned(),
    })
}

fn parse_requirement(field: &'static str, value: &str) -> Result<VersionReq, CompilationError> {
    VersionReq::parse(value).map_err(|_| CompilationError::InvalidRequirement {
        field,
        value: value.to_owned(),
    })
}

fn canonical_bytes(
    compiled: &CompiledBusinessApplicationManifest,
) -> Result<Vec<u8>, serde_json::Error> {
    #[derive(Serialize)]
    struct Canonical<'a> {
        schema_version: &'a str,
        platform_version: &'a str,
        packages: &'a [BusinessApplicationPackage],
        desired_installation_states: &'a BTreeMap<BusinessModuleId, ModuleInstallationState>,
    }
    serde_json::to_vec(&Canonical {
        schema_version: &compiled.schema_version,
        platform_version: &compiled.platform_version,
        packages: &compiled.packages,
        desired_installation_states: &compiled.desired_installation_states,
    })
}

fn public_catalog(
    packages: &[BusinessApplicationPackage],
) -> Result<PublicContributionCatalog, CompilationError> {
    let mut targets = Vec::new();
    for package in packages {
        let owner = package.manifest.module_id.clone();
        targets.extend(package.manifest.resource_kinds.iter().map(|item| {
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Resource {
                    resource_kind: item.resource_kind.clone(),
                },
                version: item.version.clone(),
            }
        }));
        targets.extend(package.manifest.published_queries.iter().map(|item| {
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Query {
                    query_id: item.contract_id.clone(),
                },
                version: item.version.clone(),
            }
        }));
        targets.extend(package.manifest.published_commands.iter().map(|item| {
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Command {
                    command_id: item.contract_id.clone(),
                },
                version: item.version.clone(),
            }
        }));
    }
    targets.sort_by_key(|target| serde_json::to_string(target).unwrap_or_else(|_| String::new()));
    if targets.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CompilationError::Duplicate {
            kind: "public target",
            identifier: serde_json::to_string(&targets[0])?,
        });
    }
    Ok(PublicContributionCatalog {
        public_targets: targets,
    })
}

fn dependency_catalog(packages: &[BusinessApplicationPackage]) -> PublishedDependencyCatalog {
    let mut public_dependencies = Vec::new();
    for package in packages {
        let owner = package.manifest.module_id.clone();
        public_dependencies.extend(package.manifest.resource_kinds.iter().map(|item| {
            PublishedDependencyReference::PublicResource {
                owner_module_id: owner.clone(),
                resource_kind: item.resource_kind.clone(),
                version: item.version.clone(),
            }
        }));
        public_dependencies.extend(package.manifest.published_queries.iter().map(|item| {
            PublishedDependencyReference::PublicQuery {
                owner_module_id: owner.clone(),
                query_id: item.contract_id.clone(),
                version: item.version.clone(),
            }
        }));
        public_dependencies.extend(package.manifest.published_commands.iter().map(|item| {
            PublishedDependencyReference::PublicCommand {
                owner_module_id: owner.clone(),
                command_id: item.contract_id.clone(),
                version: item.version.clone(),
            }
        }));
        public_dependencies.extend(
            package
                .manifest
                .agent_tool_contributions
                .iter()
                .map(|item| {
                    let capability_id = NamespacedId::new(item.contribution_id.clone())
                        .map_or_else(
                            |_| item.contribution_id.clone(),
                            |identity| identity.local_id().to_owned(),
                        );
                    PublishedDependencyReference::PublicCapability {
                        owner_module_id: owner.clone(),
                        capability_id,
                        version: item.version.clone(),
                    }
                }),
        );
    }
    public_dependencies.sort_by_key(|dependency| {
        serde_json::to_string(dependency).unwrap_or_else(|_| String::new())
    });
    PublishedDependencyCatalog {
        public_dependencies,
    }
}

fn validate_platform(
    manifest: &BusinessModuleManifest,
    platform: &Version,
) -> Result<(), CompilationError> {
    let min = manifest
        .compatibility
        .minimum_platform_version
        .as_deref()
        .map(|v| parse_version("minimum platform version", v))
        .transpose()?;
    let max = manifest
        .compatibility
        .maximum_platform_version
        .as_deref()
        .map(|v| parse_version("maximum platform version", v))
        .transpose()?;
    if min.as_ref().is_some_and(|v| platform < v)
        || max.as_ref().is_some_and(|v| platform > v)
        || min.as_ref().zip(max.as_ref()).is_some_and(|(a, b)| a > b)
    {
        return Err(CompilationError::IncompatiblePlatform {
            module_id: manifest.module_id.to_string(),
            version: platform.to_string(),
        });
    }
    for capability in manifest
        .required_platform_capabilities
        .iter()
        .chain(&manifest.optional_platform_capabilities)
    {
        parse_requirement("platform capability", &capability.version_requirement)?;
    }
    Ok(())
}

fn validate_manifest_versions(manifest: &BusinessModuleManifest) -> Result<(), CompilationError> {
    for version in manifest
        .published_commands
        .iter()
        .map(|item| &item.version)
        .chain(manifest.published_queries.iter().map(|item| &item.version))
        .chain(manifest.published_events.iter().map(|item| &item.version))
        .chain(manifest.resource_kinds.iter().map(|item| &item.version))
        .chain(
            manifest
                .semantic_contributions
                .iter()
                .map(|item| &item.version),
        )
        .chain(manifest.ui_contributions.iter().map(|item| &item.version))
        .chain(
            manifest
                .agent_tool_contributions
                .iter()
                .map(|item| &item.version),
        )
    {
        parse_version("contract version", version)?;
    }
    Ok(())
}

fn validate_typed_contributions(
    contributions: &TypedContributionSet,
) -> Result<(), CompilationError> {
    macro_rules! validate_items {
        ($items:expr) => {
            for item in $items {
                parse_version("typed contribution schema version", &item.schema_version)?;
                parse_version("typed contribution version", &item.version)?;
                parse_version("public contribution target version", &item.target.version)?;
            }
        };
    }
    validate_items!(&contributions.navigation);
    validate_items!(&contributions.list_views);
    validate_items!(&contributions.detail_sections);
    validate_items!(&contributions.detail_tabs);
    validate_items!(&contributions.actions);
    validate_items!(&contributions.commands);
    for item in &contributions.agent_capabilities {
        parse_version("agent contribution schema version", &item.schema_version)?;
        parse_version("agent contribution version", &item.version)?;
        parse_version("public contribution target version", &item.target.version)?;
    }
    for item in &contributions.policy_requirements {
        parse_version("policy requirement schema version", &item.schema_version)?;
        parse_version("policy requirement version", &item.version)?;
    }
    for item in &contributions.capability_requirements {
        parse_version(
            "capability requirement schema version",
            &item.schema_version,
        )?;
        parse_version("capability requirement version", &item.version)?;
    }
    Ok(())
}

fn register_manifest_ids(
    package: &BusinessApplicationPackage,
    contexts: &mut BTreeMap<String, String>,
    public_ids: &mut BTreeMap<String, String>,
) -> Result<(), CompilationError> {
    let owner = package.manifest.module_id.to_string();
    for context in &package.manifest.owned_bounded_contexts {
        if let Some(previous) = contexts.insert(context.clone(), owner.clone()) {
            return Err(if previous == owner {
                CompilationError::Duplicate {
                    kind: "bounded context",
                    identifier: context.clone(),
                }
            } else {
                CompilationError::OwnershipCollision {
                    kind: "bounded context",
                    identifier: context.clone(),
                }
            });
        }
    }
    for id in package
        .manifest
        .published_commands
        .iter()
        .map(|x| &x.contract_id)
        .chain(
            package
                .manifest
                .published_queries
                .iter()
                .map(|x| &x.contract_id),
        )
        .chain(
            package
                .manifest
                .published_events
                .iter()
                .map(|x| &x.contract_id),
        )
        .chain(
            package
                .manifest
                .resource_kinds
                .iter()
                .map(|x| &x.resource_kind),
        )
        .chain(
            package
                .manifest
                .ui_contributions
                .iter()
                .map(|x| &x.contribution_id),
        )
        .chain(
            package
                .manifest
                .agent_tool_contributions
                .iter()
                .map(|x| &x.contribution_id),
        )
        .chain(
            package
                .manifest
                .semantic_contributions
                .iter()
                .map(|x| &x.semantic_id),
        )
    {
        if let Some(previous) = public_ids.insert(id.clone(), owner.clone()) {
            return Err(if previous == owner {
                CompilationError::Duplicate {
                    kind: "public identifier",
                    identifier: id.clone(),
                }
            } else {
                CompilationError::OwnershipCollision {
                    kind: "public identifier",
                    identifier: id.clone(),
                }
            });
        }
    }
    Ok(())
}

fn register_typed_contribution_ids(
    package: &BusinessApplicationPackage,
    public_ids: &mut BTreeMap<String, String>,
) -> Result<(), CompilationError> {
    let owner = package.manifest.module_id.to_string();
    let ids = package
        .contributions
        .navigation
        .iter()
        .map(|item| item.contribution_id.to_string())
        .chain(
            package
                .contributions
                .list_views
                .iter()
                .map(|item| item.contribution_id.to_string()),
        )
        .chain(
            package
                .contributions
                .detail_sections
                .iter()
                .map(|item| item.contribution_id.to_string()),
        )
        .chain(
            package
                .contributions
                .detail_tabs
                .iter()
                .map(|item| item.contribution_id.to_string()),
        )
        .chain(
            package
                .contributions
                .actions
                .iter()
                .map(|item| item.contribution_id.to_string()),
        )
        .chain(
            package
                .contributions
                .commands
                .iter()
                .map(|item| item.contribution_id.to_string()),
        )
        .chain(
            package
                .contributions
                .agent_capabilities
                .iter()
                .map(|item| item.contribution_id.to_string()),
        )
        .chain(
            package
                .contributions
                .policy_requirements
                .iter()
                .map(|item| item.requirement_id.to_string()),
        )
        .chain(
            package
                .contributions
                .capability_requirements
                .iter()
                .map(|item| item.requirement_id.to_string()),
        );
    for id in ids {
        if let Some(previous) = public_ids.insert(id.clone(), owner.clone()) {
            return Err(if previous == owner {
                CompilationError::Duplicate {
                    kind: "public identifier",
                    identifier: id,
                }
            } else {
                CompilationError::OwnershipCollision {
                    kind: "public identifier",
                    identifier: id,
                }
            });
        }
    }
    Ok(())
}

fn register_identity(
    identifier: String,
    owner: &str,
    public_ids: &mut BTreeMap<String, String>,
) -> Result<(), CompilationError> {
    if let Some(previous) = public_ids.insert(identifier.clone(), owner.to_owned()) {
        return Err(if previous == owner {
            CompilationError::Duplicate {
                kind: "public identifier",
                identifier,
            }
        } else {
            CompilationError::OwnershipCollision {
                kind: "public identifier",
                identifier,
            }
        });
    }
    Ok(())
}

fn detect_cycles(packages: &[BusinessApplicationPackage]) -> Result<(), CompilationError> {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, Vec<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> bool {
        if active.contains(node) {
            return true;
        }
        if done.contains(node) {
            return false;
        }
        active.insert(node.to_owned());
        let cycle = graph
            .get(node)
            .is_some_and(|deps| deps.iter().any(|dep| visit(dep, graph, active, done)));
        active.remove(node);
        done.insert(node.to_owned());
        cycle
    }
    let graph: BTreeMap<_, _> = packages
        .iter()
        .map(|p| {
            (
                p.manifest.module_id.to_string(),
                p.manifest
                    .dependencies
                    .iter()
                    .map(|d| d.module_id.to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();
    if graph
        .keys()
        .any(|node| visit(node, &graph, &mut active, &mut done))
    {
        Err(CompilationError::DependencyCycle)
    } else {
        Ok(())
    }
}

fn normalize(packages: &mut [BusinessApplicationPackage]) {
    for package in packages {
        let m = &mut package.manifest;
        m.owned_bounded_contexts.sort();
        m.required_platform_capabilities
            .sort_by_key(|x| (x.capability_id.clone(), x.version_requirement.clone()));
        m.optional_platform_capabilities
            .sort_by_key(|x| (x.capability_id.clone(), x.version_requirement.clone()));
        m.published_commands
            .sort_by_key(|x| (x.contract_id.clone(), x.version.clone()));
        m.published_queries
            .sort_by_key(|x| (x.contract_id.clone(), x.version.clone()));
        m.published_events
            .sort_by_key(|x| (x.contract_id.clone(), x.version.clone()));
        m.resource_kinds
            .sort_by_key(|x| (x.resource_kind.clone(), x.version.clone()));
        m.data_classification.sort();
        m.semantic_contributions
            .sort_by_key(|x| (x.semantic_id.clone(), x.version.clone()));
        m.ui_contributions
            .sort_by_key(|x| (x.contribution_id.clone(), x.version.clone()));
        m.agent_tool_contributions
            .sort_by_key(|x| (x.contribution_id.clone(), x.version.clone()));
        m.dependencies
            .sort_by_key(|x| (x.module_id.to_string(), x.version_requirement.clone()));
        package
            .extension_points
            .sort_by_key(|x| x.extension_point_id.clone());
        for point in &mut package.extension_points {
            point.dependency_ids.sort_by_key(stable_json_key);
        }
        package
            .extension_contributions
            .sort_by_key(|x| x.contribution_id.clone());
        normalize_typed_contributions(&mut package.contributions);
    }
}

fn stable_json_key<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::new())
}

fn normalize_typed_contributions(contributions: &mut TypedContributionSet) {
    macro_rules! normalize_items {
        ($items:expr) => {
            for item in $items {
                item.required_policy.sort();
                item.required_capability.sort();
            }
            $items.sort_by_key(|item| item.contribution_id.clone());
        };
    }
    normalize_items!(&mut contributions.navigation);
    normalize_items!(&mut contributions.list_views);
    normalize_items!(&mut contributions.detail_sections);
    normalize_items!(&mut contributions.detail_tabs);
    normalize_items!(&mut contributions.actions);
    normalize_items!(&mut contributions.commands);
    for item in &mut contributions.agent_capabilities {
        item.required_policy.sort();
        item.required_capability.sort();
    }
    contributions
        .agent_capabilities
        .sort_by_key(|item| item.contribution_id.clone());
    contributions
        .policy_requirements
        .sort_by_key(|item| item.requirement_id.clone());
    contributions
        .capability_requirements
        .sort_by_key(|item| item.requirement_id.clone());
}
