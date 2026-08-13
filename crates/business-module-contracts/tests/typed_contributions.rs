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
    set.navigation.push(item);
    assert!(set.validate(&a, &catalog(&a)).is_ok());
    assert_eq!(set.navigation[0].required_policy.len(), 1);
    assert_eq!(set.navigation[0].required_capability.len(), 1);
}

#[test]
fn descriptors_reject_unknown_fields() {
    let json = r#"{"contribution_id":"module-a.home","owner_module_id":"module-a","schema_version":"ui.v1","version":"1.0.0","target":{"owner_module_id":"module-a","target":{"query":{"query_id":"summary"}},"version":"1.0.0"},"label_key":"home","visibility":"authenticated","unexpected":true}"#;
    assert!(serde_json::from_str::<NavigationContribution>(json).is_err());
}
