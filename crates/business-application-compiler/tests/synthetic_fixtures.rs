#![allow(clippy::default_trait_access, clippy::unwrap_used)]

use business_application_compiler::{
    compile, dry_plan, dry_plan_from_declarations, BusinessApplicationCompilerInput,
    BusinessApplicationPackage, CurrentModuleSnapshot, CurrentRegistrySnapshot, PackageChange,
    PlanDiagnostic,
};
use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, CompatibilityDescriptor,
    ContributionId, DataClassification, ExtensionAuthorizationRequirement, ExtensionContribution,
    ExtensionContributionKind, ExtensionPointId, ExtensionPointLifecycle,
    ExtensionPointRemovalSemantics, ExtensionPointVisibility, ManifestSchemaVersion,
    ModuleDataState, ModuleDependency, ModuleInstallationState, PublishedExtensionPoint,
    ResourceKindDescriptor,
};

fn package(id: &str, version: &str) -> BusinessApplicationPackage {
    let module_id = BusinessModuleId::new(id).unwrap();
    BusinessApplicationPackage {
        manifest: BusinessModuleManifest {
            module_id: module_id.clone(),
            module_version: BusinessModuleVersion::new(version).unwrap(),
            manifest_schema_version: ManifestSchemaVersion::new("business-module.manifest.v1")
                .unwrap(),
            owned_bounded_contexts: vec![format!("{id}-context")],
            required_platform_capabilities: Vec::new(),
            optional_platform_capabilities: Vec::new(),
            published_commands: Vec::new(),
            published_queries: Vec::new(),
            published_events: Vec::new(),
            resource_kinds: Vec::new(),
            data_classification: vec![DataClassification::Internal],
            migration_namespace: id.to_owned(),
            semantic_contributions: Vec::new(),
            ui_contributions: Vec::new(),
            agent_tool_contributions: Vec::new(),
            dependencies: Vec::new(),
            compatibility: CompatibilityDescriptor::default(),
        },
        contributions: Default::default(),
        extension_points: Vec::new(),
        extension_contributions: Vec::new(),
    }
}

fn input(packages: Vec<BusinessApplicationPackage>) -> BusinessApplicationCompilerInput {
    BusinessApplicationCompilerInput {
        platform_version: "1.2.0".to_owned(),
        packages,
        installed_versions: Default::default(),
        available_platform_capabilities: Default::default(),
        desired_installation_states: Default::default(),
    }
}

fn point() -> PublishedExtensionPoint {
    PublishedExtensionPoint {
        extension_point_id: ExtensionPointId::from_parts("module-a", "slot").unwrap(),
        owner_module_id: BusinessModuleId::new("module-a").unwrap(),
        contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
        schema_version: "1.0.0".to_owned(),
        allowed_contribution_kind: ExtensionContributionKind::DetailUi,
        classification: DataClassification::Internal,
        authorization_requirement: ExtensionAuthorizationRequirement {
            policy_id: None,
            capability_id: None,
        },
        lifecycle: ExtensionPointLifecycle::Published,
        dependency_ids: Vec::new(),
        removal_semantics: ExtensionPointRemovalSemantics::BlockedRemoval,
        visibility: ExtensionPointVisibility::Public,
    }
}

fn extension_package() -> BusinessApplicationPackage {
    let mut package = package("module-extension", "1.0.0");
    package.extension_contributions.push(ExtensionContribution {
        contribution_id: ContributionId::from_parts("module-extension", "use-slot").unwrap(),
        consumer_module_id: BusinessModuleId::new("module-extension").unwrap(),
        target_extension_point_id: ExtensionPointId::from_parts("module-a", "slot").unwrap(),
        expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
        classification: DataClassification::Internal,
        kind: ExtensionContributionKind::DetailUi,
    });
    package
}

fn snapshot(packages: Vec<BusinessApplicationPackage>) -> CurrentRegistrySnapshot {
    CurrentRegistrySnapshot {
        modules: packages
            .into_iter()
            .map(|package| CurrentModuleSnapshot {
                package,
                package_digest: business_module_contracts::PackageDigest::new("a".repeat(64))
                    .unwrap(),
                installation_state: ModuleInstallationState::Enabled,
                data_state: ModuleDataState::Retained,
            })
            .collect(),
    }
}

fn blocked(plan: &business_application_compiler::BusinessApplicationPlan) -> bool {
    !plan.applicable
        && plan
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic, PlanDiagnostic::BlockedRemoval { .. }))
}

#[test]
fn synthetic_modules_can_compile_independently_and_with_public_dependency() {
    assert!(compile(input(vec![package("module-a", "1.0.0")])).is_ok());
    assert!(compile(input(vec![package("module-b", "1.0.0")])).is_ok());

    let mut owner = package("module-a", "1.0.0");
    owner.manifest.resource_kinds.push(ResourceKindDescriptor {
        resource_kind: "resource-a".to_owned(),
        version: "1.0.0".to_owned(),
    });
    let mut consumer = package("module-b", "1.0.0");
    consumer.manifest.dependencies.push(ModuleDependency {
        module_id: owner.manifest.module_id.clone(),
        version_requirement: "^1.0.0".to_owned(),
    });
    assert!(compile(input(vec![consumer, owner])).is_ok());
}

#[test]
fn extension_fixture_requires_a_published_public_point() {
    let mut owner = package("module-a", "1.0.0");
    owner.extension_points.push(point());
    assert!(compile(input(vec![owner.clone(), extension_package()])).is_ok());

    assert!(matches!(
        compile(input(vec![extension_package()])),
        Err(business_application_compiler::CompilationError::UnknownExtension { .. })
    ));

    let mut private_owner = owner;
    private_owner.extension_points[0].visibility = ExtensionPointVisibility::Private;
    assert!(matches!(
        compile(input(vec![private_owner, extension_package()])),
        Err(business_application_compiler::CompilationError::Manifest(_))
    ));
}

#[test]
fn removing_extension_fixture_does_not_change_owner_package() {
    let mut owner = package("module-a", "1.0.0");
    owner.extension_points.push(point());
    let owner_only = compile(input(vec![owner.clone()])).unwrap();
    let with_extension = compile(input(vec![owner, extension_package()])).unwrap();
    let owner_from_combined = with_extension
        .packages
        .iter()
        .find(|package| package.manifest.module_id.as_str() == "module-a")
        .unwrap();
    assert_eq!(&owner_only.packages[0], owner_from_combined);
}

#[test]
fn module_removals_block_on_live_dependency_or_extension_consumer() {
    let owner = package("module-a", "1.0.0");
    let mut dependent = package("module-b", "1.0.0");
    dependent.manifest.dependencies.push(ModuleDependency {
        module_id: owner.manifest.module_id.clone(),
        version_requirement: "^1.0.0".to_owned(),
    });
    let dependency_plan = dry_plan_from_declarations(
        &snapshot(vec![owner.clone(), dependent.clone()]),
        input(vec![dependent]),
    )
    .unwrap();
    assert!(blocked(&dependency_plan));

    let mut owner_with_point = owner;
    owner_with_point.extension_points.push(point());
    let extension_plan = dry_plan_from_declarations(
        &snapshot(vec![owner_with_point.clone(), extension_package()]),
        input(vec![extension_package()]),
    )
    .unwrap();
    assert!(blocked(&extension_plan));
}

#[test]
fn synthetic_permutations_have_identical_compilation_and_plan() {
    let mut owner = package("module-a", "1.0.0");
    owner.extension_points.push(point());
    let extension = extension_package();
    let first = compile(input(vec![owner.clone(), extension.clone()])).unwrap();
    let second = compile(input(vec![extension, owner])).unwrap();
    assert_eq!(first.canonical_json(), second.canonical_json());
    assert_eq!(first.package_digest(), second.package_digest());

    let current = snapshot(vec![
        package("module-a", "1.0.0"),
        package("module-b", "1.0.0"),
    ]);
    let incoming = compile(input(vec![package("module-b", "1.0.0")])).unwrap();
    let reversed = snapshot(vec![
        package("module-b", "1.0.0"),
        package("module-a", "1.0.0"),
    ]);
    assert_eq!(
        dry_plan(&current, &incoming).unwrap(),
        dry_plan(&reversed, &incoming).unwrap()
    );
}

#[test]
fn removal_plan_never_purges_business_data() {
    let plan = dry_plan(
        &snapshot(vec![package("module-a", "1.0.0")]),
        &compile(input(Vec::new())).unwrap(),
    )
    .unwrap();
    assert!(plan.changes.iter().any(|change| matches!(
        change,
        PackageChange::RemoveModule {
            data_retained: true,
            ..
        }
    )));
    let serialized = serde_json::to_string(&plan).unwrap();
    assert!(!serialized.contains("purge"));
    assert!(!serialized.contains("delete_data"));
    assert!(!serialized.contains("drop_business_facts"));
}
