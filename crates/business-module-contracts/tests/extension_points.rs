#![allow(clippy::panic)]

use business_module_contracts::*;

fn module(value: &str) -> BusinessModuleId {
    match BusinessModuleId::new(value) {
        Ok(value) => value,
        Err(error) => panic!("invalid test module: {error}"),
    }
}

fn extension_point(owner: &BusinessModuleId) -> PublishedExtensionPoint {
    PublishedExtensionPoint {
        extension_point_id: match ExtensionPointId::from_parts(owner, "details") {
            Ok(value) => value,
            Err(error) => panic!("invalid test extension point: {error}"),
        },
        owner_module_id: owner.clone(),
        contract_version: match BusinessModuleVersion::new("1.0.0") {
            Ok(value) => value,
            Err(error) => panic!("invalid test version: {error}"),
        },
        schema_version: "extension.v1".to_owned(),
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

fn contribution(
    consumer: &BusinessModuleId,
    contribution_namespace: &BusinessModuleId,
    point: &PublishedExtensionPoint,
) -> ExtensionContribution {
    ExtensionContribution {
        contribution_id: match ContributionId::from_parts(contribution_namespace, "details") {
            Ok(value) => value,
            Err(error) => panic!("invalid test contribution: {error}"),
        },
        consumer_module_id: consumer.clone(),
        target_extension_point_id: point.extension_point_id.clone(),
        expected_contract_version: point.contract_version.clone(),
        classification: DataClassification::Internal,
        kind: ExtensionContributionKind::DetailUi,
    }
}

#[test]
fn forged_contribution_namespace_is_rejected() {
    let owner = module("module-owner");
    let consumer = module("module-consumer");
    let point = extension_point(&owner);
    let forged = contribution(&consumer, &owner, &point);

    assert!(matches!(
        validate_extension_points(&owner, &[point], &[forged]),
        Err(ManifestValidationError::WrongContributionOwner { expected, actual })
            if expected == "module-consumer" && actual == "module-owner"
    ));
}

#[test]
fn contribution_namespaced_to_consumer_is_valid() {
    let owner = module("module-owner");
    let consumer = module("module-consumer");
    let point = extension_point(&owner);
    let valid = contribution(&consumer, &consumer, &point);

    assert!(validate_extension_points(&owner, &[point], &[valid]).is_ok());
}
