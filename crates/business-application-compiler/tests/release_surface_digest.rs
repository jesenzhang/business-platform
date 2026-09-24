//! Wave 2 R2 packaging acceptance: the canonical contract digest and the
//! module release identity/bundle contracts must be deterministic, cover the
//! published cross-module surface, ignore release metadata, and assemble from
//! compiled evidence. Synthetic `module-a` fixtures only; the production
//! compiler remains business-neutral.
#![allow(clippy::default_trait_access, clippy::unwrap_used)]

use business_application_compiler::{
    compile, contract_digest, dry_plan, package_fingerprint, verify_release_identity,
    BusinessApplicationCompilerInput, BusinessApplicationPackage,
    CompiledBusinessApplicationManifest, CurrentRegistrySnapshot, PackageChange,
    ReleaseIdentityMismatch,
};
use business_module_contracts::{
    AgentCapabilityContribution, AgentCapabilityId, BuildToolchain, BusinessModuleId,
    BusinessModuleManifest, BusinessModuleVersion, CompatibilityDescriptor, ContractDescriptor,
    ContributionDescriptor, CrateVersionBinding, DataClassification,
    ExtensionAuthorizationRequirement, ExtensionContributionKind, ExtensionPointId,
    ExtensionPointLifecycle, ExtensionPointRemovalSemantics, ExtensionPointVisibility,
    ManifestSchemaVersion, MigrationRevision, MigrationSetDigest, ModuleReleaseBundle,
    ModuleReleaseError, ModuleReleaseIdentity, NavigationContribution, PolicyRequirementDescriptor,
    PolicyRequirementId, PublicContributionTarget, PublicTargetKind, PublishedExtensionPoint,
    ResourceKindDescriptor, SemanticContributionDescriptor, SourceRevision, UiContributionId,
    BUSINESS_MODULE_MANIFEST_SCHEMA_VERSION, MODULE_RELEASE_IDENTITY_SCHEMA_VERSION,
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
            agent_tool_contributions: vec![
                ContributionDescriptor {
                    contribution_id: "module-a.query-thing".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                ContributionDescriptor {
                    contribution_id: "module-a.run-reconcile".to_owned(),
                    version: "1.0.0".to_owned(),
                },
            ],
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
    second.manifest.agent_tool_contributions.reverse();
    assert_eq!(
        contract_digest(&first).unwrap(),
        contract_digest(&second).unwrap()
    );
}

#[test]
fn contract_digest_changes_when_a_public_agent_capability_id_or_version_changes() {
    // `manifest.agent_tool_contributions` is a published public contract of
    // this module: the compiler registers every entry as a
    // `PublicTargetKind::Capability` target in the public contribution catalog
    // and as a `PublishedDependencyReference::PublicCapability`, so other
    // modules may resolve and depend on exactly these IDs and versions
    // (ADR-0021 §2.5). Renaming or re-versioning a public capability is a
    // cross-module contract change and must move the contract digest.
    let base = contract_digest(&base_package()).unwrap();

    let mut renamed = base_package();
    renamed.manifest.agent_tool_contributions[0].contribution_id =
        "module-a.query-things".to_owned();
    assert_ne!(base, contract_digest(&renamed).unwrap());

    let mut bumped = base_package();
    bumped.manifest.agent_tool_contributions[0].version = "1.1.0".to_owned();
    assert_ne!(base, contract_digest(&bumped).unwrap());

    let mut dropped = base_package();
    dropped.manifest.agent_tool_contributions.remove(0);
    assert_ne!(base, contract_digest(&dropped).unwrap());
}

#[test]
fn contract_digest_ignores_consumer_side_contributions_but_package_digest_covers_them() {
    // Classification of the remaining contribution declarations (documented per
    // the independent-review remediation):
    // * `manifest.ui_contributions` — presentation declarations consumed by the
    //   platform shell; they never enter the public contribution catalog and no
    //   cross-module dependency reference resolves against them, so they are
    //   not a published cross-module contract.
    // * `package.contributions` typed UI/Agent/Policy entries — consumer-side
    //   wiring: they *reference* published targets (compile fails closed if a
    //   target is unpublished) and are themselves never targets
    //   (`PublicTargetKind` only has Resource/Query/Command/Capability), so
    //   they are not a published cross-module contract either.
    // The published public target catalog itself is fully derivable from the
    // included surface (resource kinds, queries, commands, agent capabilities),
    // so no separate catalog digest is required. All of these remain pinned by
    // the compiled package digest and the planner's per-component fingerprints,
    // so "the released artifact changed" is still provable.
    let mut consumer = base_package();
    consumer
        .manifest
        .ui_contributions
        .push(ContributionDescriptor {
            contribution_id: "module-a.thing-detail-panel".to_owned(),
            version: "1.0.0".to_owned(),
        });
    let owner = BusinessModuleId::new("module-a").unwrap();
    consumer
        .contributions
        .navigation
        .push(NavigationContribution {
            contribution_id: UiContributionId::from_parts("module-a", "thing-nav").unwrap(),
            owner_module_id: owner.clone(),
            schema_version: "1.0.0".to_owned(),
            version: "1.0.0".to_owned(),
            classification: DataClassification::Internal,
            target: PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Query {
                    query_id: "get-thing".to_owned(),
                },
                version: "1.0.0".to_owned(),
            },
            label_key: "thing".to_owned(),
            ordering: None,
            group: None,
            visibility: "always".to_owned(),
            required_policy: vec![
                PolicyRequirementId::from_parts("module-a", "review-thing").unwrap()
            ],
            required_capability: Vec::new(),
        });
    consumer
        .contributions
        .agent_capabilities
        .push(AgentCapabilityContribution {
            contribution_id: AgentCapabilityId::from_parts("module-a", "thing-inspector").unwrap(),
            owner_module_id: owner.clone(),
            schema_version: "1.0.0".to_owned(),
            version: "1.0.0".to_owned(),
            classification: DataClassification::Internal,
            target: PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Command {
                    command_id: "create-thing".to_owned(),
                },
                version: "1.0.0".to_owned(),
            },
            label_key: "thing".to_owned(),
            required_policy: Vec::new(),
            required_capability: Vec::new(),
        });
    consumer
        .contributions
        .policy_requirements
        .push(PolicyRequirementDescriptor {
            requirement_id: PolicyRequirementId::from_parts("module-a", "review-thing").unwrap(),
            owner_module_id: owner,
            schema_version: "1.0.0".to_owned(),
            policy_id: "module-a.contract-review".to_owned(),
            version: "1.0.0".to_owned(),
        });
    // The consumer wiring must survive compilation (targets resolve, policy
    // reference resolves) or the fixture proves nothing.
    compiled_with(consumer.clone());

    assert_eq!(
        contract_digest(&base_package()).unwrap(),
        contract_digest(&consumer).unwrap(),
        "UI declarations and consumer-side typed contributions are not the \
         published cross-module contract surface"
    );
    assert_ne!(
        package_digest_of(&base_package()),
        package_digest_of(&consumer),
        "the compiled package digest still pins the presentation and \
         consumer-side declarations"
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

// ---------------------------------------------------------------------------
// Release identity consistency gate (round-2 remediation).
// `ModuleReleaseIdentity::validate` is a field-shape check; the executable
// consistency entry is `verify_release_identity`, which re-computes both
// canonical digests from the compiled package evidence and compares every
// identity field against it. Each single-field mismatch has its own negative
// test below; nothing here relies on hand-filling matching constants.
// ---------------------------------------------------------------------------

fn identity_from_compiled(compiled: &CompiledBusinessApplicationManifest) -> ModuleReleaseIdentity {
    let package = compiled.packages.first().unwrap();
    ModuleReleaseIdentity {
        schema_version: MODULE_RELEASE_IDENTITY_SCHEMA_VERSION.to_owned(),
        module_id: package.manifest.module_id.clone(),
        module_version: package.manifest.module_version.clone(),
        manifest_schema_version: package.manifest.manifest_schema_version.clone(),
        source_revision: SourceRevision::new("5d3d2eb5cfe93aad6f8c5fe4d023d386d0a08b04").unwrap(),
        contract_digest: contract_digest(package).unwrap(),
        package_digest: package_fingerprint(package).unwrap(),
        migration_revision: Some(MigrationRevision {
            migration_namespace: package.manifest.migration_namespace.clone(),
            set_digest: MigrationSetDigest::new("c".repeat(64)).unwrap(),
        }),
        build_toolchain: BuildToolchain {
            toolchain_id: "x86_64-pc-windows-msvc".to_owned(),
            version: "1.94.1".to_owned(),
        },
    }
}

#[test]
fn package_fingerprint_matches_the_planner_per_module_digest() {
    // The public fingerprint seam used by the release gate must report the
    // exact per-module digest the planner already produces, so identity
    // pinning and planning agree on one compiled-package digest.
    let package = base_package();
    assert_eq!(
        package_fingerprint(&package).unwrap(),
        package_digest_of(&package),
        "package_fingerprint must equal the planner's AddModule digest"
    );
}

#[test]
fn verify_release_identity_accepts_identity_recomputed_from_compiled_evidence() {
    let compiled = compiled_with(base_package());
    let identity = identity_from_compiled(&compiled);
    assert_eq!(verify_release_identity(&compiled, &identity), Ok(()));
    // An equivalent, permuted declaration compiles to the same canonical
    // evidence; the identity survives verification because it is defined by
    // the canonical content, not by how the input was ordered.
    let mut permuted = base_package();
    permuted.manifest.published_commands.reverse();
    permuted.manifest.agent_tool_contributions.reverse();
    let recompiled = compiled_with(permuted);
    assert_eq!(verify_release_identity(&recompiled, &identity), Ok(()));
}

#[test]
fn verify_release_identity_accepts_stateless_identity_without_migration_revision() {
    // A module that ships no migration set pins `None`; the manifest-level
    // namespace declaration is then intentionally unconstrained.
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    identity.migration_revision = None;
    assert_eq!(verify_release_identity(&compiled, &identity), Ok(()));
}

#[test]
fn verify_release_identity_rejects_pinned_module_id_not_in_evidence() {
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    identity.module_id = BusinessModuleId::new("module-z").unwrap();
    assert!(matches!(
        verify_release_identity(&compiled, &identity),
        Err(ReleaseIdentityMismatch::UnknownModule { .. })
    ));
}

#[test]
fn verify_release_identity_rejects_pinned_module_version_mismatch() {
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    identity.module_version = BusinessModuleVersion::new("1.0.1").unwrap();
    assert_eq!(
        verify_release_identity(&compiled, &identity),
        Err(ReleaseIdentityMismatch::ModuleVersion {
            pinned: "1.0.1".to_owned(),
            declared: "1.0.0".to_owned(),
        })
    );
}

#[test]
fn verify_release_identity_rejects_pinned_manifest_schema_version_mismatch() {
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    identity.manifest_schema_version =
        ManifestSchemaVersion::new("business-module.manifest.v2".to_owned()).unwrap();
    assert!(matches!(
        verify_release_identity(&compiled, &identity),
        Err(ReleaseIdentityMismatch::ManifestSchemaVersion { .. })
    ));
}

#[test]
fn verify_release_identity_rejects_pinned_contract_digest_mismatch() {
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    // A release tool that pinned a stale or wrong contract surface digest
    // must be caught even though every other field agrees: the digest is
    // re-computed over the compiled declaration.
    identity.contract_digest =
        business_module_contracts::ContractDigest::new("a".repeat(64)).unwrap();
    assert!(matches!(
        verify_release_identity(&compiled, &identity),
        Err(ReleaseIdentityMismatch::ContractDigest { .. })
    ));
}

#[test]
fn verify_release_identity_rejects_pinned_package_digest_mismatch() {
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    identity.package_digest =
        business_module_contracts::PackageDigest::new("b".repeat(64)).unwrap();
    assert!(matches!(
        verify_release_identity(&compiled, &identity),
        Err(ReleaseIdentityMismatch::PackageDigest { .. })
    ));
}

#[test]
fn verify_release_identity_rejects_pinned_migration_namespace_mismatch() {
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    identity
        .migration_revision
        .as_mut()
        .unwrap()
        .migration_namespace = "module-a-archive".to_owned();
    assert_eq!(
        verify_release_identity(&compiled, &identity),
        Err(ReleaseIdentityMismatch::MigrationNamespace {
            pinned: "module-a-archive".to_owned(),
            declared: "module-a".to_owned(),
        })
    );
}

#[test]
fn verify_release_identity_rejects_identity_failing_shape_validation() {
    // Non-SemVer module versions never reach the comparison stage; the
    // shape contract rejects them first.
    let compiled = compiled_with(base_package());
    let mut identity = identity_from_compiled(&compiled);
    identity.module_version = BusinessModuleVersion::new("1.2").unwrap();
    assert!(matches!(
        verify_release_identity(&compiled, &identity),
        Err(ReleaseIdentityMismatch::IdentityShape(
            ModuleReleaseError::NotSemVer { .. }
        ))
    ));
}
