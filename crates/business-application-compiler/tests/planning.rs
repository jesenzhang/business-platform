#![allow(clippy::default_trait_access, clippy::unwrap_used)]

use std::collections::BTreeMap;

use business_application_compiler::{
    compile, dry_plan, dry_plan_from_declarations, BusinessApplicationCompilerInput,
    BusinessApplicationPackage, CurrentModuleSnapshot, CurrentRegistrySnapshot, PackageChange,
    PlanDiagnostic, PlanError,
};
use business_module_contracts::{
    AgentCapabilityContribution, AgentCapabilityId, BusinessModuleId, BusinessModuleManifest,
    BusinessModuleVersion, CompatibilityDescriptor, ContractDescriptor, ContributionDescriptor,
    DataClassification, ExtensionAuthorizationRequirement, ExtensionContribution,
    ExtensionContributionKind, ExtensionPointId, ExtensionPointLifecycle,
    ExtensionPointRemovalSemantics, ExtensionPointVisibility, ManifestSchemaVersion,
    ModuleDataState, ModuleDependency, ModuleInstallationState, NavigationContribution,
    PolicyRequirementDescriptor, PolicyRequirementId, PublicContributionTarget, PublicTargetKind,
    PublishedDependencyReference, ResourceKindDescriptor, UiContributionId,
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

fn compiler_input(packages: Vec<BusinessApplicationPackage>) -> BusinessApplicationCompilerInput {
    BusinessApplicationCompilerInput {
        platform_version: "1.2.0".to_owned(),
        packages,
        installed_versions: Default::default(),
        available_platform_capabilities: Default::default(),
        desired_installation_states: Default::default(),
    }
}

fn compiled(
    packages: Vec<BusinessApplicationPackage>,
) -> business_application_compiler::IncomingCompiledPackages {
    compile(compiler_input(packages)).unwrap()
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

fn query_target(owner: &BusinessModuleId) -> PublicContributionTarget {
    PublicContributionTarget {
        owner_module_id: owner.clone(),
        target: PublicTargetKind::Query {
            query_id: "read".to_owned(),
        },
        version: "1.0.0".to_owned(),
    }
}

fn capability_provider() -> BusinessApplicationPackage {
    let mut provider = package("module-a", "1.0.0");
    provider
        .manifest
        .agent_tool_contributions
        .push(ContributionDescriptor {
            contribution_id: "module-a.capability".to_owned(),
            version: "1.0.0".to_owned(),
        });
    provider
}

fn capability_consumer(references_capability: bool) -> BusinessApplicationPackage {
    let mut consumer = package("module-b", "1.0.0");
    if references_capability {
        let mut point = point_decl(ExtensionPointId::from_parts("module-b", "slot").unwrap());
        point
            .dependency_ids
            .push(PublishedDependencyReference::PublicCapability {
                owner_module_id: BusinessModuleId::new("module-a").unwrap(),
                capability_id: "capability".to_owned(),
                version: "1.0.0".to_owned(),
            });
        consumer.extension_points.push(point);
    }
    consumer
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
fn desired_disabled_state_emits_disable_module() {
    let module = BusinessModuleId::new("module-a").unwrap();
    let incoming = compile(BusinessApplicationCompilerInput {
        platform_version: "1.2.0".to_owned(),
        packages: vec![package("module-a", "1.0.0")],
        installed_versions: Default::default(),
        available_platform_capabilities: Default::default(),
        desired_installation_states: BTreeMap::from([(
            module.clone(),
            ModuleInstallationState::Disabled,
        )]),
    })
    .unwrap();
    let plan = dry_plan(&snapshot(vec![package("module-a", "1.0.0")]), &incoming).unwrap();
    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::DisableModule { module_id } if module_id == &module
    )));
    assert!(plan.applicable);
}

#[test]
fn adding_dependency_blocks_provider_removal_from_incoming_graph() {
    let owner = package("module-a", "1.0.0");
    let current_consumer = package("module-b", "1.0.0");
    let mut incoming_consumer = current_consumer.clone();
    incoming_consumer
        .manifest
        .dependencies
        .push(ModuleDependency {
            module_id: BusinessModuleId::new("module-a").unwrap(),
            version_requirement: "^1.0.0".to_owned(),
        });

    let plan = dry_plan_from_declarations(
        &snapshot(vec![owner, current_consumer]),
        compiler_input(vec![incoming_consumer]),
    )
    .unwrap();

    assert!(!plan.applicable);
    assert!(plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::BlockedRemoval { module_id: Some(module_id), reason, .. }
            if module_id.as_str() == "module-a" && reason.contains("dependency")
    )));
}

#[test]
fn dropping_dependency_allows_provider_removal_from_incoming_graph() {
    let owner = package("module-a", "1.0.0");
    let mut current_consumer = package("module-b", "1.0.0");
    current_consumer
        .manifest
        .dependencies
        .push(ModuleDependency {
            module_id: BusinessModuleId::new("module-a").unwrap(),
            version_requirement: "^1.0.0".to_owned(),
        });

    let plan = dry_plan_from_declarations(
        &snapshot(vec![owner, current_consumer.clone()]),
        compiler_input(vec![package("module-b", "1.0.0")]),
    )
    .unwrap();

    assert!(plan.applicable);
    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::RemoveModule { module_id, .. } if module_id.as_str() == "module-a"
    )));
}

#[test]
fn retained_public_target_consumers_block_target_removal() {
    let owner_id = BusinessModuleId::new("module-a").unwrap();
    let mut owner = package("module-a", "1.0.0");
    owner.manifest.published_queries.push(ContractDescriptor {
        contract_id: "read".to_owned(),
        version: "1.0.0".to_owned(),
    });

    let mut ui_consumer = package("module-b", "1.0.0");
    ui_consumer
        .contributions
        .navigation
        .push(NavigationContribution {
            contribution_id: UiContributionId::from_parts("module-b", "read-nav").unwrap(),
            owner_module_id: ui_consumer.manifest.module_id.clone(),
            schema_version: "1.0.0".to_owned(),
            version: "1.0.0".to_owned(),
            classification: DataClassification::Internal,
            target: query_target(&owner_id),
            label_key: "read".to_owned(),
            ordering: None,
            group: None,
            visibility: "always".to_owned(),
            required_policy: Vec::new(),
            required_capability: Vec::new(),
        });

    let mut agent_consumer = package("module-c", "1.0.0");
    agent_consumer
        .contributions
        .agent_capabilities
        .push(AgentCapabilityContribution {
            contribution_id: AgentCapabilityId::from_parts("module-c", "read-tool").unwrap(),
            owner_module_id: agent_consumer.manifest.module_id.clone(),
            schema_version: "1.0.0".to_owned(),
            version: "1.0.0".to_owned(),
            classification: DataClassification::Internal,
            target: query_target(&owner_id),
            label_key: "read".to_owned(),
            required_policy: Vec::new(),
            required_capability: Vec::new(),
        });

    let mut extension_consumer = package("module-extension", "1.0.0");
    let mut point = point_decl(ExtensionPointId::from_parts("module-extension", "slot").unwrap());
    point
        .dependency_ids
        .push(PublishedDependencyReference::PublicQuery {
            owner_module_id: owner_id.clone(),
            query_id: "read".to_owned(),
            version: "1.0.0".to_owned(),
        });
    extension_consumer.extension_points.push(point);

    let plan = dry_plan_from_declarations(
        &snapshot(vec![
            owner,
            ui_consumer.clone(),
            agent_consumer.clone(),
            extension_consumer.clone(),
        ]),
        compiler_input(vec![
            package("module-a", "1.0.0"),
            ui_consumer,
            agent_consumer,
            extension_consumer,
        ]),
    )
    .unwrap();

    let blockers: Vec<_> = plan
        .diagnostics
        .iter()
        .filter_map(|diagnostic| match diagnostic {
            PlanDiagnostic::BlockedRemoval {
                module_id: Some(module_id),
                reason,
                ..
            } if reason.contains("public target") => Some(module_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(blockers.len(), 3);
    assert!(blockers.contains(&"module-b"));
    assert!(blockers.contains(&"module-c"));
    assert!(blockers.contains(&"module-extension"));
}

#[test]
fn retained_public_capability_consumer_blocks_provider_removal() {
    let provider = capability_provider();
    let consumer = capability_consumer(true);
    let plan = dry_plan_from_declarations(
        &snapshot(vec![provider, consumer.clone()]),
        compiler_input(vec![consumer]),
    )
    .unwrap();

    assert!(!plan.applicable);
    assert!(plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::BlockedRemoval {
            module_id: Some(module_id),
            reason,
            ..
        } if module_id.as_str() == "module-b" && reason.contains("public capability")
    )));
}

#[test]
fn dropping_public_capability_reference_allows_provider_removal() {
    let provider = capability_provider();
    let current_consumer = capability_consumer(true);
    let incoming_consumer = capability_consumer(false);
    let plan = dry_plan_from_declarations(
        &snapshot(vec![provider, current_consumer]),
        compiler_input(vec![incoming_consumer]),
    )
    .unwrap();

    assert!(plan.applicable);
    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::RemoveModule { module_id, .. } if module_id.as_str() == "module-a"
    )));
}

#[test]
fn provider_removal_is_allowed_without_live_public_capability_consumer() {
    let plan = dry_plan(&snapshot(vec![capability_provider()]), &compiled(vec![])).unwrap();

    assert!(plan.applicable);
    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::RemoveModule { module_id, .. } if module_id.as_str() == "module-a"
    )));
}

#[test]
fn public_capability_removal_planning_is_deterministic() {
    let provider = capability_provider();
    let consumer = capability_consumer(true);
    let first = dry_plan_from_declarations(
        &snapshot(vec![provider.clone(), consumer.clone()]),
        compiler_input(vec![consumer.clone()]),
    )
    .unwrap();
    let second = dry_plan_from_declarations(
        &snapshot(vec![consumer.clone(), provider]),
        compiler_input(vec![consumer]),
    )
    .unwrap();

    assert_eq!(first, second);
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
fn package_metadata_changes_do_not_update_untouched_contributions() {
    let mut current = package("module-a", "1.0.0");
    current.manifest.published_queries.push(ContractDescriptor {
        contract_id: "read".to_owned(),
        version: "1.0.0".to_owned(),
    });
    let mut incoming = current.clone();
    incoming.manifest.compatibility = CompatibilityDescriptor {
        minimum_platform_version: Some("1.0.0".to_owned()),
        maximum_platform_version: None,
    };

    let plan = dry_plan(&snapshot(vec![current]), &compiled(vec![incoming])).unwrap();

    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::CompatibilityChange { module_id, .. } if module_id.as_str() == "module-a"
    )));
    assert!(!has_change(&plan, |change| matches!(
        change,
        PackageChange::UpdateContribution { contribution_id, .. } if contribution_id == "read"
    )));
}

#[test]
fn only_changed_contribution_gets_an_update_digest() {
    let mut current = package("module-a", "1.0.0");
    current.manifest.published_queries = vec![
        ContractDescriptor {
            contract_id: "changed".to_owned(),
            version: "1.0.0".to_owned(),
        },
        ContractDescriptor {
            contract_id: "untouched".to_owned(),
            version: "1.0.0".to_owned(),
        },
    ];
    let mut incoming = current.clone();
    incoming.manifest.published_queries[0].version = "2.0.0".to_owned();

    let plan = dry_plan(&snapshot(vec![current]), &compiled(vec![incoming])).unwrap();
    let updates: Vec<_> = plan
        .changes
        .iter()
        .filter_map(|change| match change {
            PackageChange::UpdateContribution {
                contribution_id, ..
            } => Some(contribution_id.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(updates, vec!["changed"]);
}

#[test]
fn typed_requirement_changes_are_planned_as_contribution_updates() {
    let mut current = package("module-a", "1.0.0");
    current.contributions.policy_requirements = vec![PolicyRequirementDescriptor {
        requirement_id: PolicyRequirementId::from_parts("module-a", "policy").unwrap(),
        owner_module_id: BusinessModuleId::new("module-a").unwrap(),
        schema_version: "1.0.0".to_owned(),
        policy_id: "module-a.read".to_owned(),
        version: "1.0.0".to_owned(),
    }];
    let mut incoming = current.clone();
    incoming.contributions.policy_requirements[0].version = "2.0.0".to_owned();

    let plan = dry_plan(&snapshot(vec![current]), &compiled(vec![incoming])).unwrap();

    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::UpdateContribution { contribution_id, .. }
            if contribution_id == "module-a.policy"
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
    let blocked = dry_plan_from_declarations(
        &snapshot(vec![owner.clone(), live_consumer.clone()]),
        compiler_input(vec![live_consumer]),
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
        &snapshot(vec![owner.clone(), extension_consumer.clone()]),
        &compiled(vec![package("module-a", "1.0.0")]),
    )
    .unwrap();
    assert!(!blocked
        .diagnostics
        .iter()
        .any(|d| matches!(d, PlanDiagnostic::BlockedRemoval { reason, .. } if reason.contains("consumer"))));

    let blocked = dry_plan_from_declarations(
        &snapshot(vec![owner, extension_consumer.clone()]),
        compiler_input(vec![extension_consumer]),
    )
    .unwrap();
    assert!(blocked.diagnostics.iter().any(|d| matches!(
        d,
        PlanDiagnostic::BlockedRemoval { reason, .. } if reason.contains("consumer")
    )));
}

#[test]
fn owner_removal_with_active_extension_consumer_is_blocked() {
    let point = ExtensionPointId::from_parts("module-a", "slot").unwrap();
    let mut owner = package("module-a", "1.0.0");
    owner.extension_points.push(point_decl(point.clone()));
    let mut consumer = package("module-extension", "1.0.0");
    consumer
        .extension_contributions
        .push(ExtensionContribution {
            contribution_id: business_module_contracts::ContributionId::from_parts(
                "module-extension",
                "use-slot",
            )
            .unwrap(),
            consumer_module_id: consumer.manifest.module_id.clone(),
            target_extension_point_id: point,
            expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
            classification: DataClassification::Internal,
            kind: ExtensionContributionKind::DetailUi,
        });
    let plan = dry_plan_from_declarations(
        &snapshot(vec![owner, consumer.clone()]),
        compiler_input(vec![consumer]),
    )
    .unwrap();
    assert!(!plan.applicable);
    assert!(plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::BlockedRemoval { module_id: Some(module_id), reason, .. }
            if module_id.as_str() == "module-a" && reason.contains("consumer")
    )));
}

#[test]
fn extension_point_removal_with_active_extension_consumer_is_blocked() {
    let point = ExtensionPointId::from_parts("module-a", "slot").unwrap();
    let mut current_owner = package("module-a", "1.0.0");
    current_owner
        .extension_points
        .push(point_decl(point.clone()));
    let mut consumer = package("module-extension", "1.0.0");
    consumer
        .extension_contributions
        .push(ExtensionContribution {
            contribution_id: business_module_contracts::ContributionId::from_parts(
                "module-extension",
                "use-slot",
            )
            .unwrap(),
            consumer_module_id: consumer.manifest.module_id.clone(),
            target_extension_point_id: point.clone(),
            expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
            classification: DataClassification::Internal,
            kind: ExtensionContributionKind::DetailUi,
        });
    let plan = dry_plan_from_declarations(
        &snapshot(vec![current_owner, consumer.clone()]),
        compiler_input(vec![package("module-a", "1.0.0"), consumer]),
    )
    .unwrap();
    assert!(!plan.applicable);
    assert!(plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::BlockedRemoval { module_id: Some(module_id), identifier, reason }
            if module_id.as_str() == "module-a"
                && identifier == &point.to_string()
                && reason.contains("consumer")
    )));
}

#[test]
fn owner_removal_is_allowed_when_retained_consumer_drops_contribution() {
    let point = ExtensionPointId::from_parts("module-a", "slot").unwrap();
    let mut owner = package("module-a", "1.0.0");
    owner.extension_points.push(point_decl(point.clone()));
    let mut consumer = package("module-extension", "1.0.0");
    consumer
        .extension_contributions
        .push(ExtensionContribution {
            contribution_id: business_module_contracts::ContributionId::from_parts(
                "module-extension",
                "use-slot",
            )
            .unwrap(),
            consumer_module_id: consumer.manifest.module_id.clone(),
            target_extension_point_id: point,
            expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
            classification: DataClassification::Internal,
            kind: ExtensionContributionKind::DetailUi,
        });
    let plan = dry_plan(
        &snapshot(vec![owner, consumer]),
        &compiled(vec![package("module-extension", "1.0.0")]),
    )
    .unwrap();
    assert!(plan.applicable);
    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::RemoveModule { module_id, .. }
            if module_id.as_str() == "module-a"
    )));
    assert!(!plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::BlockedRemoval { reason, .. } if reason.contains("consumer")
    )));
}

#[test]
fn extension_point_removal_is_allowed_when_retained_consumer_drops_contribution() {
    let point = ExtensionPointId::from_parts("module-a", "slot").unwrap();
    let mut owner = package("module-a", "1.0.0");
    owner.extension_points.push(point_decl(point.clone()));
    let mut consumer = package("module-extension", "1.0.0");
    consumer
        .extension_contributions
        .push(ExtensionContribution {
            contribution_id: business_module_contracts::ContributionId::from_parts(
                "module-extension",
                "use-slot",
            )
            .unwrap(),
            consumer_module_id: consumer.manifest.module_id.clone(),
            target_extension_point_id: point.clone(),
            expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
            classification: DataClassification::Internal,
            kind: ExtensionContributionKind::DetailUi,
        });
    let plan = dry_plan(
        &snapshot(vec![owner, consumer]),
        &compiled(vec![
            package("module-a", "1.0.0"),
            package("module-extension", "1.0.0"),
        ]),
    )
    .unwrap();
    assert!(plan.applicable);
    assert!(has_change(&plan, |change| matches!(
        change,
        PackageChange::RemoveExtensionPoint { extension_point_id, .. }
            if extension_point_id == &point
    )));
    assert!(!plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::BlockedRemoval { reason, .. } if reason.contains("consumer")
    )));
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
fn planning_declarations_reject_unknown_external_extension_point() {
    let mut consumer = package("module-extension", "1.0.0");
    consumer
        .extension_contributions
        .push(ExtensionContribution {
            contribution_id: business_module_contracts::ContributionId::from_parts(
                "module-extension",
                "unknown-slot",
            )
            .unwrap(),
            consumer_module_id: consumer.manifest.module_id.clone(),
            target_extension_point_id: ExtensionPointId::from_parts("module-a", "missing").unwrap(),
            expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
            classification: DataClassification::Internal,
            kind: ExtensionContributionKind::DetailUi,
        });
    let plan = dry_plan_from_declarations(
        &CurrentRegistrySnapshot::default(),
        compiler_input(vec![consumer]),
    )
    .unwrap();
    assert!(!plan.applicable);
    assert!(plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::Conflict { reason, .. } if reason.contains("unknown extension point")
    )));
}

#[test]
fn planning_unknown_dependency_is_a_structured_conflict() {
    let mut dependent = package("module-b", "1.0.0");
    dependent.manifest.dependencies.push(ModuleDependency {
        module_id: BusinessModuleId::new("missing").unwrap(),
        version_requirement: "^1.0.0".into(),
    });

    let plan = dry_plan_from_declarations(
        &CurrentRegistrySnapshot::default(),
        compiler_input(vec![dependent]),
    )
    .unwrap();

    assert!(!plan.applicable);
    assert!(plan.changes.iter().any(|change| matches!(
        change,
        PackageChange::Conflict { identifier, .. } if identifier == "missing"
    )));
    assert!(plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::Conflict { identifier, reason, .. }
            if identifier == "missing" && reason.contains("unknown module dependency")
    )));
}

#[test]
fn planning_incompatible_dependency_is_a_structured_conflict() {
    let mut dependent = package("module-b", "1.0.0");
    dependent.manifest.dependencies.push(ModuleDependency {
        module_id: BusinessModuleId::new("module-a").unwrap(),
        version_requirement: "^2.0.0".into(),
    });

    let plan = dry_plan_from_declarations(
        &CurrentRegistrySnapshot::default(),
        compiler_input(vec![package("module-a", "1.0.0"), dependent]),
    )
    .unwrap();

    assert!(!plan.applicable);
    assert!(plan.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        PlanDiagnostic::Conflict { identifier, reason, .. }
            if identifier == "module-a" && reason.contains("incompatible")
    )));
}

#[test]
fn planning_ownership_collision_is_a_structured_conflict() {
    let mut other = package("module-b", "1.0.0");
    other.manifest.owned_bounded_contexts = vec!["module-a-context".into()];

    let plan = dry_plan_from_declarations(
        &CurrentRegistrySnapshot::default(),
        compiler_input(vec![package("module-a", "1.0.0"), other]),
    )
    .unwrap();

    assert!(!plan.applicable);
    assert!(plan.changes.iter().any(|change| matches!(
        change,
        PackageChange::Conflict { identifier, .. }
            if identifier == "module-a-context"
    )));
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
        owner_module_id: BusinessModuleId::new(id.module_id()).unwrap(),
        extension_point_id: id,
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
