#![allow(clippy::default_trait_access, clippy::unwrap_used)]

use business_application_compiler::{
    compile, dry_plan, BusinessApplicationCompilerInput, BusinessApplicationPackage,
    CurrentModuleSnapshot, CurrentRegistrySnapshot, PackageChange, PlanDiagnostic, PlanError,
};
use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, CompatibilityDescriptor,
    DataClassification, ExtensionAuthorizationRequirement, ExtensionContribution,
    ExtensionContributionKind, ExtensionPointId, ExtensionPointLifecycle,
    ExtensionPointRemovalSemantics, ExtensionPointVisibility, ManifestSchemaVersion,
    ModuleDataState, ModuleDependency, ModuleInstallationState, ResourceKindDescriptor,
};

fn package(id: &str, version: &str) -> BusinessApplicationPackage {
    let module = BusinessModuleId::new(id.to_owned()).unwrap();
    BusinessApplicationPackage {
        manifest: BusinessModuleManifest {
            module_id: module,
            module_version: BusinessModuleVersion::new(version.to_owned()).unwrap(),
            manifest_schema_version: ManifestSchemaVersion::new("business-module.manifest.v1")
                .unwrap(),
            owned_bounded_contexts: vec![format!("{id}-context")],
            required_platform_capabilities: vec![],
            optional_platform_capabilities: vec![],
            published_commands: vec![],
            published_queries: vec![],
            published_events: vec![],
            resource_kinds: vec![],
            data_classification: vec![DataClassification::Internal],
            migration_namespace: id.to_owned(),
            semantic_contributions: vec![],
            ui_contributions: vec![],
            agent_tool_contributions: vec![],
            dependencies: vec![],
            compatibility: CompatibilityDescriptor::default(),
        },
        contributions: Default::default(),
        extension_points: vec![],
        extension_contributions: vec![],
    }
}

fn compiled(
    packages: Vec<BusinessApplicationPackage>,
) -> business_application_compiler::IncomingCompiledPackages {
    compile(BusinessApplicationCompilerInput {
        platform_version: "1.2.0".to_owned(),
        packages,
        installed_versions: Default::default(),
    })
    .unwrap()
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

fn has_change(
    plan: &business_application_compiler::BusinessApplicationPlan,
    predicate: impl Fn(&PackageChange) -> bool,
) -> bool {
    plan.changes.iter().any(predicate)
}

#[test]
fn add_upgrade_remove_and_retention_are_explicit() {
    let old = package("module-a", "1.0.0");
    let mut upgraded = package("module-a", "2.0.0");
    upgraded
        .manifest
        .resource_kinds
        .push(ResourceKindDescriptor {
            resource_kind: "record".to_owned(),
            version: "1.0.0".to_owned(),
        });
    let plan = dry_plan(
        &snapshot(vec![old]),
        &compiled(vec![package("module-b", "1.0.0"), upgraded]),
    )
    .unwrap();
    assert!(has_change(&plan, |c| matches!(
        c,
        PackageChange::UpgradeModule { .. }
    )));
    assert!(has_change(
        &plan,
        |c| matches!(c, PackageChange::AddModule { module_id, .. } if module_id.as_str() == "module-b")
    ));
    assert!(!plan
        .changes
        .iter()
        .any(|c| serde_json::to_string(c).unwrap().contains("purge")));
    let removed = dry_plan(
        &snapshot(vec![package("module-a", "1.0.0")]),
        &compiled(vec![]),
    )
    .unwrap();
    assert!(has_change(&removed, |c| matches!(
        c,
        PackageChange::RemoveModule {
            data_retained: true,
            ..
        }
    )));
}

#[test]
fn contribution_and_extension_point_changes_are_diffed() {
    let mut old = package("module-a", "1.0.0");
    old.manifest.resource_kinds.push(ResourceKindDescriptor {
        resource_kind: "old-record".to_owned(),
        version: "1.0.0".to_owned(),
    });
    let mut new = package("module-a", "1.0.0");
    new.manifest.resource_kinds.push(ResourceKindDescriptor {
        resource_kind: "new-record".to_owned(),
        version: "1.0.0".to_owned(),
    });
    let point = ExtensionPointId::from_parts("module-a", "slot").unwrap();
    old.extension_points.push(point_decl(point.clone()));
    let added = dry_plan(&snapshot(vec![old]), &compiled(vec![new])).unwrap();
    assert!(has_change(&added, |c| matches!(
        c,
        PackageChange::AddContribution { .. }
    )));
    assert!(has_change(&added, |c| matches!(
        c,
        PackageChange::RemoveContribution { .. }
    )));
    assert!(has_change(
        &added,
        |c| matches!(c, PackageChange::RemoveExtensionPoint { extension_point_id, .. } if extension_point_id == &point)
    ));
    let with_point = dry_plan(
        &snapshot(vec![package("module-a", "1.0.0")]),
        &compiled(vec![{
            let mut p = package("module-a", "1.0.0");
            p.extension_points.push(point_decl(point));
            p
        }]),
    )
    .unwrap();
    assert!(has_change(&with_point, |c| matches!(
        c,
        PackageChange::AddExtensionPoint { .. }
    )));
}

#[test]
fn live_dependency_and_active_extension_consumer_block_removal() {
    let mut owner = package("module-a", "1.0.0");
    let mut live_consumer = package("module-b", "1.0.0");
    live_consumer.manifest.dependencies.push(ModuleDependency {
        module_id: BusinessModuleId::new("module-a").unwrap(),
        version_requirement: "^1.0.0".to_owned(),
    });
    let blocked = dry_plan(
        &snapshot(vec![owner.clone(), live_consumer]),
        &compiled(vec![package("module-b", "1.0.0")]),
    )
    .unwrap();
    assert!(!blocked.applicable);
    assert!(blocked.diagnostics.iter().any(|d| matches!(d, PlanDiagnostic::BlockedRemoval { reason, .. } if reason.contains("dependency"))));
    let point = ExtensionPointId::from_parts("module-a", "slot").unwrap();
    owner.extension_points.push(point_decl(point.clone()));
    let mut extension_consumer = package("module-extension", "1.0.0");
    extension_consumer
        .extension_contributions
        .push(ExtensionContribution {
            contribution_id: business_module_contracts::ContributionId::from_parts(
                "module-extension",
                "use-slot",
            )
            .unwrap(),
            consumer_module_id: extension_consumer.manifest.module_id.clone(),
            target_extension_point_id: point,
            expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
            classification: DataClassification::Internal,
            kind: ExtensionContributionKind::DetailUi,
        });
    let blocked = dry_plan(
        &snapshot(vec![owner, extension_consumer]),
        &compiled(vec![
            package("module-a", "1.0.0"),
            package("module-extension", "1.0.0"),
        ]),
    )
    .unwrap();
    assert!(blocked.diagnostics.iter().any(|d| matches!(d, PlanDiagnostic::BlockedRemoval { reason, .. } if reason.contains("consumer"))));
}

#[test]
fn permutations_produce_identical_plan_and_diagnostics() {
    let a = package("module-a", "1.0.0");
    let b = package("module-b", "1.0.0");
    let first = dry_plan(
        &snapshot(vec![a.clone(), b.clone()]),
        &compiled(vec![b.clone()]),
    )
    .unwrap();
    let second = dry_plan(
        &snapshot(vec![b, a]),
        &compiled(vec![package("module-b", "1.0.0")]),
    )
    .unwrap();
    assert_eq!(first, second);
}

#[test]
fn tampered_incoming_manifest_is_rejected_before_planning() {
    let mut incoming = compiled(vec![package("module-a", "1.0.0")]);
    incoming.package_digest =
        business_module_contracts::PackageDigest::new("b".repeat(64)).unwrap();
    assert!(matches!(
        dry_plan(&CurrentRegistrySnapshot::default(), &incoming),
        Err(PlanError::IncomingNotCanonical(_))
    ));
}

#[test]
fn module_adds_use_independent_package_digests() {
    let plan = dry_plan(
        &CurrentRegistrySnapshot::default(),
        &compiled(vec![
            package("module-a", "1.0.0"),
            package("module-b", "1.0.0"),
        ]),
    )
    .unwrap();
    let digests: Vec<_> = plan
        .changes
        .iter()
        .filter_map(|change| match change {
            PackageChange::AddModule { digest, .. } => Some(digest),
            _ => None,
        })
        .collect();
    assert_eq!(digests.len(), 2);
    assert_ne!(digests[0], digests[1]);
}

#[test]
fn duplicate_current_module_is_rejected() {
    let package = package("module-a", "1.0.0");
    let snapshot = snapshot(vec![package.clone(), package]);
    assert!(matches!(
        dry_plan(&snapshot, &compiled(vec![])),
        Err(PlanError::DuplicateModule { .. })
    ));
}

fn point_decl(id: ExtensionPointId) -> business_module_contracts::PublishedExtensionPoint {
    business_module_contracts::PublishedExtensionPoint {
        extension_point_id: id,
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
        dependency_ids: vec![],
        removal_semantics: ExtensionPointRemovalSemantics::BlockedRemoval,
        visibility: ExtensionPointVisibility::Public,
    }
}
