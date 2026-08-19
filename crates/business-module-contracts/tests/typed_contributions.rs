#![allow(clippy::panic)]

use business_module_contracts::*;

fn module(value: &str) -> BusinessModuleId {
    match BusinessModuleId::new(value) {
        Ok(value) => value,
        Err(error) => panic!("invalid fixture module: {error}"),
    }
}

fn target(owner: &BusinessModuleId) -> PublicContributionTarget {
    PublicContributionTarget {
        owner_module_id: owner.clone(),
        target: PublicTargetKind::Query {
            query_id: "summary".into(),
        },
        version: "1.0.0".into(),
    }
}

fn navigation(
    owner: &BusinessModuleId,
    target_owner: &BusinessModuleId,
    id: &str,
) -> NavigationContribution {
    NavigationContribution {
        contribution_id: match UiContributionId::from_parts(owner, id) {
            Ok(value) => value,
            Err(error) => panic!("invalid fixture contribution: {error}"),
        },
        owner_module_id: owner.clone(),
        schema_version: "ui.v1".into(),
        version: "1.0.0".into(),
        classification: DataClassification::Internal,
        target: target(target_owner),
        label_key: "module-a.summary".into(),
        ordering: Some(10),
        group: Some("main".into()),
        visibility: "authenticated".into(),
        required_policy: vec![],
        required_capability: vec![],
    }
}

fn catalog(owner: &BusinessModuleId) -> PublicContributionCatalog {
    PublicContributionCatalog {
        public_targets: vec![target(owner)],
    }
}

fn policy_requirement_id(owner: &BusinessModuleId, id: &str) -> PolicyRequirementId {
    match PolicyRequirementId::from_parts(owner, id) {
        Ok(value) => value,
        Err(error) => panic!("invalid fixture policy requirement: {error}"),
    }
}

fn capability_requirement_id(owner: &BusinessModuleId, id: &str) -> CapabilityRequirementId {
    match CapabilityRequirementId::from_parts(owner, id) {
        Ok(value) => value,
        Err(error) => panic!("invalid fixture capability requirement: {error}"),
    }
}

#[test]
fn module_a_and_module_b_can_declare_independent_typed_contributions() {
    let a = module("module-a");
    let b = module("module-b");
    let mut a_set = TypedContributionSet::default();
    a_set.navigation.push(navigation(&a, &a, "home"));
    let mut b_set = TypedContributionSet::default();
    b_set.navigation.push(navigation(&b, &b, "home"));
    assert!(a_set.validate(&a, &catalog(&a)).is_ok());
    assert!(b_set.validate(&b, &catalog(&b)).is_ok());
}

#[test]
fn duplicate_and_wrong_owner_are_rejected() {
    let a = module("module-a");
    let b = module("module-b");
    let mut set = TypedContributionSet::default();
    set.navigation.push(navigation(&a, &a, "home"));
    set.navigation.push(navigation(&a, &a, "home"));
    assert!(matches!(
        set.validate(&a, &catalog(&a)),
        Err(ManifestValidationError::DuplicateIdentifier { .. })
    ));

    let mut wrong = TypedContributionSet::default();
    wrong.navigation.push(navigation(&b, &b, "home"));
    assert!(matches!(
        wrong.validate(&a, &catalog(&b)),
        Err(ManifestValidationError::WrongContributionOwner { .. })
    ));
}

#[test]
fn forged_typed_contribution_namespace_is_rejected() {
    let a = module("module-a");
    let b = module("module-b");
    let mut set = TypedContributionSet::default();
    let mut item = navigation(&a, &a, "home");
    item.contribution_id = match UiContributionId::from_parts(&b, "home") {
        Ok(value) => value,
        Err(error) => panic!("invalid fixture contribution: {error}"),
    };
    set.navigation.push(item);

    assert!(matches!(
        set.validate(&a, &catalog(&a)),
        Err(ManifestValidationError::WrongContributionOwner { expected, actual })
            if expected == "module-a" && actual == "module-b"
    ));
}

#[test]
fn unknown_public_target_rejected_but_cross_module_public_target_is_allowed() {
    let a = module("module-a");
    let b = module("module-b");
    let mut unknown = TypedContributionSet::default();
    unknown.navigation.push(navigation(&a, &a, "home"));
    assert!(matches!(
        unknown.validate(&a, &PublicContributionCatalog::default()),
        Err(ManifestValidationError::UnknownPublicTarget)
    ));

    let mut cross = TypedContributionSet::default();
    cross.navigation.push(navigation(&b, &a, "home"));
    assert!(cross.validate(&b, &catalog(&a)).is_ok());
}

#[test]
fn malformed_catalog_target_is_rejected_before_matching() {
    let a = module("module-a");
    let mut malformed_catalog = catalog(&a);
    malformed_catalog.public_targets[0].target = PublicTargetKind::Query {
        query_id: "Summary".into(),
    };
    let mut set = TypedContributionSet::default();
    set.navigation.push(navigation(&a, &a, "home"));

    assert!(matches!(
        set.validate(&a, &malformed_catalog),
        Err(ManifestValidationError::InvalidField {
            kind: "public target query ID"
        })
    ));
}

#[test]
fn malformed_incoming_public_target_is_rejected() {
    let a = module("module-a");
    let mut set = TypedContributionSet::default();
    let mut item = navigation(&a, &a, "home");
    item.target.target = PublicTargetKind::Command {
        command_id: "private.command".into(),
    };
    set.navigation.push(item);

    assert!(matches!(
        set.validate(&a, &catalog(&a)),
        Err(ManifestValidationError::InvalidField {
            kind: "public target command ID"
        })
    ));
}

#[test]
fn ui_and_agent_target_surfaces_are_closed_and_capability_targets_are_published() {
    let owner = module("module-a");
    let catalog = PublicContributionCatalog {
        public_targets: vec![
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Resource {
                    resource_kind: "record".into(),
                },
                version: "1.0.0".into(),
            },
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Query {
                    query_id: "summary".into(),
                },
                version: "1.0.0".into(),
            },
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Command {
                    command_id: "update".into(),
                },
                version: "1.0.0".into(),
            },
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Capability {
                    capability_id: "summarize".into(),
                },
                version: "1.0.0".into(),
            },
        ],
    };

    let mut ui_capability = navigation(&owner, &owner, "capability");
    ui_capability.target = catalog.public_targets[3].clone();
    let ui_targeting_command = {
        let mut item = navigation(&owner, &owner, "command");
        item.target = catalog.public_targets[2].clone();
        item
    };
    let ui = TypedContributionSet {
        navigation: vec![ui_capability, ui_targeting_command],
        ..TypedContributionSet::default()
    };
    assert!(matches!(
        ui.validate(&owner, &catalog),
        Err(
            ManifestValidationError::UnsupportedTypedContributionTarget {
                contribution_kind: "UI",
                target_kind: "command"
            }
        )
    ));

    let agent = AgentCapabilityContribution {
        contribution_id: match AgentCapabilityId::from_parts(&owner, "tool") {
            Ok(value) => value,
            Err(error) => panic!("invalid fixture agent contribution: {error}"),
        },
        owner_module_id: owner.clone(),
        schema_version: "agent.v1".into(),
        version: "1.0.0".into(),
        classification: DataClassification::Internal,
        target: catalog.public_targets[0].clone(),
        label_key: "record".into(),
        required_policy: vec![],
        required_capability: vec![],
    };
    let agents = TypedContributionSet {
        agent_capabilities: vec![agent],
        ..TypedContributionSet::default()
    };
    assert!(matches!(
        agents.validate(&owner, &catalog),
        Err(
            ManifestValidationError::UnsupportedTypedContributionTarget {
                contribution_kind: "Agent",
                target_kind: "resource"
            }
        )
    ));
}

#[test]
fn malformed_public_target_version_is_rejected() {
    let a = module("module-a");
    let mut set = TypedContributionSet::default();
    let mut item = navigation(&a, &a, "home");
    item.target.version = "not a version".into();
    set.navigation.push(item);

    assert!(matches!(
        set.validate(&a, &catalog(&a)),
        Err(ManifestValidationError::InvalidField {
            kind: "public target version"
        })
    ));
}

#[test]
fn declaration_does_not_grant_authorization() {
    let a = module("module-a");
    let mut set = TypedContributionSet::default();
    let mut item = navigation(&a, &a, "home");
    item.required_policy
        .push(match PolicyRequirementId::from_parts(&a, "read") {
            Ok(value) => value,
            Err(error) => panic!("invalid fixture policy: {error}"),
        });
    item.required_capability
        .push(match CapabilityRequirementId::from_parts(&a, "query") {
            Ok(value) => value,
            Err(error) => panic!("invalid fixture capability: {error}"),
        });
    set.policy_requirements.push(PolicyRequirementDescriptor {
        requirement_id: policy_requirement_id(&a, "read"),
        owner_module_id: a.clone(),
        schema_version: "policy.v1".into(),
        policy_id: "module-a.read".into(),
        version: "1.0.0".into(),
    });
    set.capability_requirements
        .push(CapabilityRequirementDescriptor {
            requirement_id: capability_requirement_id(&a, "query"),
            owner_module_id: a.clone(),
            schema_version: "capability.v1".into(),
            capability_id: "module-a.query".into(),
            version: "1.0.0".into(),
        });
    set.navigation.push(item);
    assert!(set.validate(&a, &catalog(&a)).is_ok());
    assert_eq!(set.navigation[0].required_policy.len(), 1);
    assert_eq!(set.navigation[0].required_capability.len(), 1);
}

#[test]
fn duplicate_requirement_descriptors_are_rejected() {
    let a = module("module-a");
    let policy = PolicyRequirementDescriptor {
        requirement_id: policy_requirement_id(&a, "read"),
        owner_module_id: a.clone(),
        schema_version: "policy.v1".into(),
        policy_id: "module-a.read".into(),
        version: "1.0.0".into(),
    };
    let capability = CapabilityRequirementDescriptor {
        requirement_id: capability_requirement_id(&a, "query"),
        owner_module_id: a.clone(),
        schema_version: "capability.v1".into(),
        capability_id: "module-a.query".into(),
        version: "1.0.0".into(),
    };
    let policies = TypedContributionSet {
        policy_requirements: vec![policy.clone(), policy],
        ..TypedContributionSet::default()
    };
    assert!(matches!(
        policies.validate(&a, &catalog(&a)),
        Err(ManifestValidationError::DuplicateIdentifier {
            kind: "policy requirement",
            ..
        })
    ));

    let capabilities = TypedContributionSet {
        capability_requirements: vec![capability.clone(), capability],
        ..TypedContributionSet::default()
    };
    assert!(matches!(
        capabilities.validate(&a, &catalog(&a)),
        Err(ManifestValidationError::DuplicateIdentifier {
            kind: "capability requirement",
            ..
        })
    ));
}

#[test]
fn dangling_requirement_references_are_rejected() {
    let a = module("module-a");
    let mut set = TypedContributionSet::default();
    let mut item = navigation(&a, &a, "home");
    item.required_policy
        .push(policy_requirement_id(&a, "missing"));
    item.required_capability
        .push(capability_requirement_id(&a, "missing"));
    set.navigation.push(item);

    assert!(matches!(
        set.validate(&a, &catalog(&a)),
        Err(ManifestValidationError::UnknownRequirementReference {
            kind: "policy requirement",
            identifier
        }) if identifier == "module-a.missing"
    ));
}

#[test]
fn descriptors_reject_unknown_fields() {
    let json = r#"{"contribution_id":"module-a.home","owner_module_id":"module-a","schema_version":"ui.v1","version":"1.0.0","target":{"owner_module_id":"module-a","target":{"query":{"query_id":"summary"}},"version":"1.0.0"},"label_key":"home","visibility":"authenticated","unexpected":true}"#;
    assert!(serde_json::from_str::<NavigationContribution>(json).is_err());
}
