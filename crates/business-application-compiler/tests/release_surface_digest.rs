//! Wave 2 R2 packaging acceptance: the canonical contract digest and the
//! module release identity/bundle contracts must be deterministic, cover the
//! published cross-module surface, ignore release metadata, and assemble from
//! compiled evidence. Synthetic `module-a` fixtures only; the production
//! compiler remains business-neutral.
#![allow(clippy::default_trait_access, clippy::unwrap_used)]

use business_application_compiler::{
    compile, contract_digest, dry_plan, BusinessApplicationCompilerInput,
    BusinessApplicationPackage, CurrentRegistrySnapshot, PackageChange,
};
use business_module_contracts::{
    BuildToolchain, BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion,
    CompatibilityDescriptor, ContractDescriptor, CrateVersionBinding, DataClassification,
    ExtensionAuthorizationRequirement, ExtensionContributionKind, ExtensionPointId,
    ExtensionPointLifecycle, ExtensionPointRemovalSemantics, ExtensionPointVisibility,
    ManifestSchemaVersion, MigrationRevision, MigrationSetDigest, ModuleReleaseBundle,
    ModuleReleaseError, ModuleReleaseIdentity, PublishedExtensionPoint, ResourceKindDescriptor,
    SemanticContributionDescriptor, SourceRevision, BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION,
    MODULE_RELEASE_IDENTITY_SCHEMA_VERSION,
};

fn base_package() -> BusinessApplicationPackage {
    let module_id = BusinessModuleId::new("module-a").unwrap();
    BusinessApplicationPackage {
        manifest: BusinessModuleManifest {
            module_id: module_id.clone(),
            module_version: BusinessModuleVersion::new("1.0.0").unwrap(),
            manifest_schema_version: ManifestSchemaVersion::new(
                BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION.to_owned(),
            )
            .unwrap(),
            owned_bounded_contexts: vec!["module-a-context".to_owned()],
            required_platform_capabilities: Vec::new(),
            optional_platform_capabilities: Vec::new(),
            published_commands: vec![
                ContractDescriptor {
                    contract_id: "create-thing".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                ContractDescriptor {
                    contract_id: "archive-thing".to_owned(),
                    version: "1.0.0".to_owned(),
                },
            ],
            published_queries: vec![ContractDescriptor {
                contract_id: "get-thing".to_owned(),
                version: "1.0.0".to_owned(),
            }],
            published_events: Vec::new(),
            resource_kinds: vec![
                ResourceKindDescriptor {
                    resource_kind: "thing".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                ResourceKindDescriptor {
                    resource_kind: "thing-audit".to_owned(),
                    version: "1.0.0".to_owned(),
                },
            ],
            data_classification: vec![
                DataClassification::Internal,
                DataClassification::Confidential,
            ],
            migration_namespace: "module-a".to_owned(),
            semantic_contributions: vec![SemanticContributionDescriptor {
                semantic_id: "module-a.metric-throughput".to_owned(),
                semantic_kind: "metric".to_owned(),
                version: "1.0.0".to_owned(),
            }],
            ui_contributions: Vec::new(),
            agent_tool_contributions: Vec::new(),
            dependencies: Vec::new(),
            compatibility: CompatibilityDescriptor::default(),
        },
        contributions: Default::default(),
        extension_points: vec![point("slot", "1.0.0")],
        extension_contributions: Vec::new(),
    }
}

fn point(name: &str, contract_version: &str) -> PublishedExtensionPoint {
    PublishedExtensionPoint {
        extension_point_id: ExtensionPointId::from_parts("module-a", name).unwrap(),
        owner_module_id: BusinessModuleId::new("module-a").unwrap(),
        contract_version: BusinessModuleVersion::new(contract_version).unwrap(),
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

fn compiled_with(
    package: BusinessApplicationPackage,
) -> business_application_compiler::CompiledBusinessApplicationManifest {
    compile(BusinessApplicationCompilerInput {
        platform_version: "1.0.0".to_owned(),
        packages: vec![package],
        installed_versions: Default::default(),
        available_platform_capabilities: Default::default(),
        desired_installation_states: Default::default(),
    })
    .unwrap()
}

fn package_digest_of(
    package: &BusinessApplicationPackage,
) -> business_module_contracts::PackageDigest {
    let compiled = compiled_with(package.clone());
    let plan = dry_plan(&CurrentRegistrySnapshot::default(), &compiled).unwrap();
    plan.changes
        .iter()
        .find_map(|change| match change {
            PackageChange::AddModule { module_id, digest } if module_id.as_str() == "module-a" => {
                Some(digest.clone())
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn contract_digest_ignores_input_order_of_the_same_contract_surface() {
    let first = base_package();
    let mut second = base_package();
    second.manifest.published_commands.reverse();
    second.manifest.resource_kinds.reverse();
    second.manifest.data_classification.reverse();
    assert_eq!(
        contract_digest(&first).unwrap(),
        contract_digest(&second).unwrap()
    );
}

#[test]
fn contract_digest_changes_when_a_published_contract_or_extension_version_changes() {
    let base = contract_digest(&base_package()).unwrap();

    let mut query_bumped = base_package();
    query_bumped.manifest.published_queries[0].version = "1.1.0".to_owned();
    assert_ne!(base, contract_digest(&query_bumped).unwrap());

    let mut extension_bumped = base_package();
    extension_bumped.extension_points[0].contract_version =
        BusinessModuleVersion::new("2.0.0").unwrap();
    assert_ne!(base, contract_digest(&extension_bumped).unwrap());

    let mut extended_surface = base_package();
    extended_surface
        .manifest
        .published_events
        .push(ContractDescriptor {
            contract_id: "thing-archived".to_owned(),
            version: "1.0.0".to_owned(),
        });
    assert_ne!(base, contract_digest(&extended_surface).unwrap());

    let mut extra_point = base_package();
    extra_point
        .extension_points
        .push(point("slot-two", "1.0.0"));
    assert_ne!(base, contract_digest(&extra_point).unwrap());

    let mut classification_widened = base_package();
    classification_widened
        .manifest
        .data_classification
        .push(DataClassification::Restricted);
    assert_ne!(base, contract_digest(&classification_widened).unwrap());

    let mut semantic_bumped = base_package();
    semantic_bumped.manifest.semantic_contributions[0].version = "1.1.0".to_owned();
    assert_ne!(base, contract_digest(&semantic_bumped).unwrap());
}

#[test]
fn contract_digest_ignores_release_metadata_but_package_digest_covers_it() {
    let base = base_package();
    let mut release_metadata = base_package();
    release_metadata.manifest.module_version = BusinessModuleVersion::new("1.0.1").unwrap();
    release_metadata.manifest.compatibility = CompatibilityDescriptor {
        minimum_platform_version: Some("1.0.0".to_owned()),
        maximum_platform_version: Some("2.0.0".to_owned()),
    };

    assert_eq!(
        contract_digest(&base).unwrap(),
        contract_digest(&release_metadata).unwrap(),
        "module version and compatibility window are release metadata, not contract surface"
    );

    let base_package_digest = package_digest_of(&base);
    let release_package_digest = package_digest_of(&release_metadata);
    assert_ne!(
        base_package_digest, release_package_digest,
        "the package digest pins packaging metadata that the contract digest ignores"
    );
}

#[test]
fn release_identity_and_bundle_assemble_from_compiled_evidence() {
    let package = base_package();
    let package_digest = package_digest_of(&package);
    let contract = contract_digest(&package).unwrap();

    let identity = ModuleReleaseIdentity {
        schema_version: MODULE_RELEASE_IDENTITY_SCHEMA_VERSION.to_owned(),
        module_id: BusinessModuleId::new("module-a").unwrap(),
        module_version: BusinessModuleVersion::new("1.0.0").unwrap(),
        manifest_schema_version: ManifestSchemaVersion::new(
            BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION.to_owned(),
        )
        .unwrap(),
        source_revision: SourceRevision::new("5d3d2eb5cfe93aad6f8c5fe4d023d386d0a08b04").unwrap(),
        contract_digest: contract.clone(),
        package_digest: package_digest.clone(),
        migration_revision: Some(MigrationRevision {
            migration_namespace: "module-a".to_owned(),
            set_digest: MigrationSetDigest::new("c".repeat(64)).unwrap(),
        }),
        build_toolchain: BuildToolchain {
            toolchain_id: "x86_64-pc-windows-msvc".to_owned(),
            version: "1.94.1".to_owned(),
        },
    };
    assert_eq!(identity.validate(), Ok(()));

    let bundle = ModuleReleaseBundle {
        identity: identity.clone(),
        crate_versions: vec![
            CrateVersionBinding {
                crate_name: "module-a".to_owned(),
                crate_version: "1.0.0".to_owned(),
            },
            CrateVersionBinding {
                crate_name: "module_a_postgres".to_owned(),
                crate_version: "1.0.0".to_owned(),
            },
        ],
    };
    assert_eq!(bundle.validate(), Ok(()));

    let json = serde_json::to_string(&bundle).unwrap();
    let restored: ModuleReleaseBundle = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, bundle);

    let mut duplicated = bundle;
    duplicated.crate_versions.push(CrateVersionBinding {
        crate_name: "module-a".to_owned(),
        crate_version: "0.9.0".to_owned(),
    });
    assert!(matches!(
        duplicated.validate(),
        Err(ModuleReleaseError::DuplicateCrate { .. })
    ));
}

#[test]
fn contract_digest_is_stable_across_repeated_invocations() {
    let package = base_package();
    let first = contract_digest(&package).unwrap();
    let second = contract_digest(&package).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.as_str().len(), 64);
}
