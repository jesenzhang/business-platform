use business_module_contracts::{
    AgentCapabilityId, ContributionId, ExtensionPointId, ModuleContractError, NamespacedId,
    PackageDigest, PolicyRequirementId, UiContributionId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn namespaced_ids_round_trip_and_expose_parts() -> TestResult {
    let identity = NamespacedId::from_parts("module-a", "detail-section")?;

    assert_eq!(identity.as_str(), "module-a.detail-section");
    assert_eq!(identity.module_id(), "module-a");
    assert_eq!(identity.local_id(), "detail-section");
    assert_eq!(identity.to_string(), "module-a.detail-section");

    let encoded = serde_json::to_string(&identity)?;
    assert_eq!(encoded, r#""module-a.detail-section""#);
    let decoded: NamespacedId = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, identity);
    Ok(())
}

#[test]
fn typed_identities_share_namespace_rules_but_not_identity_types() -> TestResult {
    let contribution = ContributionId::from_parts("module-a", "panel")?;
    let extension_point = ExtensionPointId::from_parts("module-a", "panel")?;
    let ui = UiContributionId::from_parts("module-a", "panel")?;
    let policy = PolicyRequirementId::from_parts("module-a", "panel")?;
    let agent = AgentCapabilityId::from_parts("module-a", "panel")?;

    assert_eq!(contribution.as_str(), extension_point.as_str());
    assert_eq!(contribution.as_str(), ui.as_str());
    assert_eq!(contribution.as_str(), policy.as_str());
    assert_eq!(contribution.as_str(), agent.as_str());
    assert_eq!(
        contribution.as_namespaced_id(),
        extension_point.as_namespaced_id()
    );
    Ok(())
}

#[test]
fn labels_and_paths_do_not_change_stable_identity() -> TestResult {
    let original = ContributionId::from_parts("module-a", "contract-summary")?;
    let renamed_label = ContributionId::from_parts("module-a", "contract-summary")?;
    let different_local_identity = ContributionId::from_parts("module-a", "contract-detail")?;

    assert_eq!(original, renamed_label);
    assert_ne!(original, different_local_identity);
    Ok(())
}

#[test]
fn malformed_namespaced_id_is_rejected() {
    for value in [
        "",
        "module-a",
        ".local",
        "module-a.",
        "module_a.local",
        "Module-a.local",
        "module-a.local.part",
        "module-a/local",
        "module-a.table_name",
        "module-a.route/:id",
        "module-a.local-",
        "module-a..local",
    ] {
        assert!(
            NamespacedId::new(value).is_err(),
            "accepted invalid ID: {value}"
        );
    }
}

#[test]
fn invalid_id_errors_identify_the_typed_identity() {
    assert_eq!(
        UiContributionId::new("Module-a.panel"),
        Err(ModuleContractError::InvalidCharacters {
            kind: "UI contribution ID"
        })
    );
}

#[test]
fn serde_rejects_invalid_typed_identity() {
    let result = serde_json::from_str::<PolicyRequirementId>(r#""module-a.private/table""#);
    assert!(result.is_err());
}

#[test]
fn package_digest_requires_lowercase_sha256_hex() -> TestResult {
    let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert_eq!(PackageDigest::new(valid)?.as_str(), valid);

    for invalid in [
        "",
        "0123456789abcdef",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEFFF",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg12",
    ] {
        assert!(
            PackageDigest::new(invalid).is_err(),
            "accepted invalid digest: {invalid}"
        );
    }

    let encoded = serde_json::to_string(&PackageDigest::new(valid)?)?;
    assert_eq!(encoded, format!(r#""{valid}""#));
    Ok(())
}
