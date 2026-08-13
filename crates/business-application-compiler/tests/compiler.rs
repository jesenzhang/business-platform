use business_application_compiler::{
    compile, BusinessApplicationCompilerInput, BusinessApplicationPackage, CompilationError,
};
use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, CompatibilityDescriptor,
    ContributionId, DataClassification, ExtensionAuthorizationRequirement, ExtensionContribution,
    ExtensionContributionKind, ExtensionPointId, ExtensionPointLifecycle,
    ExtensionPointRemovalSemantics, ExtensionPointVisibility, ManifestSchemaVersion,
    PublicContributionTarget, PublicTargetKind, PublishedExtensionPoint, ResourceKindDescriptor,
};
use sha2::Digest;

#[allow(clippy::unwrap_used, clippy::default_trait_access)]
mod tests {
    use super::*;

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
            desired_installation_states: Default::default(),
        }
    }

    #[test]
    fn accepts_standard_semver_ranges_and_rejects_invalid_versions() {
        let mut a = package("module-a", "1.2.3");
        a.manifest.compatibility.minimum_platform_version = Some("^1.0.0".to_owned());
        assert!(matches!(
            compile(input(vec![a])),
            Err(CompilationError::InvalidVersion {
                field: "minimum platform version",
                ..
            })
        ));
        let mut a = package("module-a", "1.2.3");
        a.manifest.compatibility.minimum_platform_version = Some("1.0.0".to_owned());
        a.manifest.compatibility.maximum_platform_version = Some("2.0.0".to_owned());
        assert!(compile(input(vec![a])).is_ok());
    }

    #[test]
    fn rejects_unknown_incompatible_cyclic_and_downgrade_dependencies() {
        let mut a = package("module-a", "1.0.0");
        a.manifest
            .dependencies
            .push(business_module_contracts::ModuleDependency {
                module_id: BusinessModuleId::new("missing").unwrap(),
                version_requirement: "^1.0.0".to_owned(),
            });
        assert!(matches!(
            compile(input(vec![a])),
            Err(CompilationError::UnknownDependency { .. })
        ));

        let mut a = package("module-a", "1.0.0");
        let b = package("module-b", "2.0.0");
        a.manifest
            .dependencies
            .push(business_module_contracts::ModuleDependency {
                module_id: b.manifest.module_id.clone(),
                version_requirement: "^1.0.0".to_owned(),
            });
        assert!(matches!(
            compile(input(vec![a, b])),
            Err(CompilationError::IncompatibleDependency { .. })
        ));

        let mut a = package("module-a", "1.0.0");
        let mut b = package("module-b", "1.0.0");
        a.manifest
            .dependencies
            .push(business_module_contracts::ModuleDependency {
                module_id: b.manifest.module_id.clone(),
                version_requirement: "^1.0.0".to_owned(),
            });
        b.manifest
            .dependencies
            .push(business_module_contracts::ModuleDependency {
                module_id: a.manifest.module_id.clone(),
                version_requirement: "^1.0.0".to_owned(),
            });
        assert!(matches!(
            compile(input(vec![a, b])),
            Err(CompilationError::DependencyCycle)
        ));

        let incoming = package("module-a", "1.0.0");
        let mut source = input(vec![incoming.clone()]);
        source.installed_versions.insert(
            incoming.manifest.module_id.clone(),
            BusinessModuleVersion::new("2.0.0").unwrap(),
        );
        assert!(matches!(
            compile(source),
            Err(CompilationError::Downgrade { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_and_cross_owner_contexts() {
        let mut a = package("module-a", "1.0.0");
        let mut b = package("module-b", "1.0.0");
        b.manifest.owned_bounded_contexts = a.manifest.owned_bounded_contexts.clone();
        assert!(matches!(
            compile(input(vec![a.clone(), b])),
            Err(CompilationError::OwnershipCollision { .. })
        ));
        a.manifest
            .owned_bounded_contexts
            .push("module-a-context".to_owned());
        assert!(matches!(
            compile(input(vec![a])),
            Err(CompilationError::Duplicate { .. })
        ));
    }

    #[test]
    fn rejects_legacy_and_typed_contribution_identity_collision() {
        let mut package = package("module-a", "1.0.0");
        package
            .manifest
            .ui_contributions
            .push(business_module_contracts::ContributionDescriptor {
                contribution_id: "module-a.shared".to_owned(),
                version: "1.0.0".to_owned(),
            });
        package
            .contributions
            .navigation
            .push(business_module_contracts::NavigationContribution {
                contribution_id: business_module_contracts::UiContributionId::from_parts(
                    "module-a", "shared",
                )
                .unwrap(),
                owner_module_id: BusinessModuleId::new("module-a").unwrap(),
                schema_version: "1.0.0".to_owned(),
                version: "1.0.0".to_owned(),
                target: PublicContributionTarget {
                    owner_module_id: BusinessModuleId::new("module-a").unwrap(),
                    target: PublicTargetKind::Resource {
                        resource_kind: "record".to_owned(),
                    },
                    version: "1.0.0".to_owned(),
                },
                label_key: "record".to_owned(),
                ordering: None,
                group: None,
                visibility: "always".to_owned(),
                required_policy: Vec::new(),
                required_capability: Vec::new(),
            });
        package
            .manifest
            .resource_kinds
            .push(ResourceKindDescriptor {
                resource_kind: "record".to_owned(),
                version: "1.0.0".to_owned(),
            });
        assert!(matches!(
            compile(input(vec![package])),
            Err(CompilationError::Duplicate { .. })
        ));
    }

    #[test]
    fn permutations_have_identical_model_bytes_and_digest() {
        let mut a = package("module-a", "1.0.0");
        a.manifest
            .owned_bounded_contexts
            .push("extra-context".to_owned());
        let b = package("module-b", "1.0.0");
        let first = compile(input(vec![a.clone(), b.clone()])).unwrap();
        a.manifest.owned_bounded_contexts.reverse();
        let second = compile(input(vec![b, a])).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.canonical_json(), second.canonical_json());
        assert_eq!(
            first.package_digest().as_str(),
            format!("{:x}", sha2::Sha256::digest(first.canonical_json())).as_str()
        );
    }

    #[test]
    fn serialized_compiled_manifest_rebuilds_canonical_bytes() {
        let compiled = compile(input(vec![package("module-a", "1.0.0")])).unwrap();
        let encoded = serde_json::to_vec(&compiled).unwrap();
        let decoded: business_application_compiler::CompiledBusinessApplicationManifest =
            serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.canonical_json(), compiled.canonical_json());
        assert_eq!(decoded.package_digest(), compiled.package_digest());
        assert!(
            business_application_compiler::dry_plan(
                &business_application_compiler::CurrentRegistrySnapshot::default(),
                &decoded
            )
            .unwrap()
            .applicable
        );
    }

    #[test]
    fn deserializing_tampered_package_digest_fails_closed() {
        let compiled = compile(input(vec![package("module-a", "1.0.0")])).unwrap();
        let mut encoded: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&compiled).unwrap()).unwrap();
        let mut tampered_digest = compiled.package_digest().as_str().to_owned();
        let replacement = if tampered_digest.ends_with('0') {
            '1'
        } else {
            '0'
        };
        tampered_digest.replace_range(tampered_digest.len() - 1.., &replacement.to_string());
        encoded["package_digest"] = serde_json::Value::String(tampered_digest);

        let error = serde_json::from_value::<
            business_application_compiler::CompiledBusinessApplicationManifest,
        >(encoded)
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "package digest does not match canonical bytes"
        );
    }

    #[test]
    fn accepts_cross_package_public_target_from_compiled_catalog() {
        let mut owner = package("module-a", "1.0.0");
        owner.manifest.resource_kinds.push(ResourceKindDescriptor {
            resource_kind: "record".to_owned(),
            version: "1.2.0".to_owned(),
        });
        let mut consumer = package("module-b", "1.0.0");
        consumer
            .contributions
            .navigation
            .push(business_module_contracts::NavigationContribution {
                contribution_id: business_module_contracts::UiContributionId::from_parts(
                    "module-b", "nav",
                )
                .unwrap(),
                owner_module_id: BusinessModuleId::new("module-b").unwrap(),
                schema_version: "1.0.0".to_owned(),
                version: "1.0.0".to_owned(),
                target: PublicContributionTarget {
                    owner_module_id: BusinessModuleId::new("module-a").unwrap(),
                    target: PublicTargetKind::Resource {
                        resource_kind: "record".to_owned(),
                    },
                    version: "1.2.0".to_owned(),
                },
                label_key: "nav.record".to_owned(),
                ordering: None,
                group: None,
                visibility: "always".to_owned(),
                required_policy: Vec::new(),
                required_capability: Vec::new(),
            });
        assert!(compile(input(vec![consumer, owner])).is_ok());
    }
    #[test]
    fn all_package_vector_permutations_have_identical_digest() {
        let mut owner = package("module-a", "1.0.0");
        owner.manifest.resource_kinds.extend([
            ResourceKindDescriptor {
                resource_kind: "zeta".to_owned(),
                version: "1.0.0".to_owned(),
            },
            ResourceKindDescriptor {
                resource_kind: "alpha".to_owned(),
                version: "1.0.0".to_owned(),
            },
        ]);
        owner
            .manifest
            .dependencies
            .push(business_module_contracts::ModuleDependency {
                module_id: BusinessModuleId::new("module-b").unwrap(),
                version_requirement: "^1.0.0".to_owned(),
            });
        let point_id = ExtensionPointId::from_parts("module-a", "slot").unwrap();
        owner.extension_points.push(PublishedExtensionPoint {
            extension_point_id: point_id.clone(),
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
        });
        let mut consumer = package("module-b", "1.0.0");
        consumer
            .extension_contributions
            .push(ExtensionContribution {
                contribution_id: ContributionId::from_parts("module-b", "use-slot").unwrap(),
                consumer_module_id: BusinessModuleId::new("module-b").unwrap(),
                target_extension_point_id: point_id,
                expected_contract_version: BusinessModuleVersion::new("1.0.0").unwrap(),
                classification: DataClassification::Internal,
                kind: ExtensionContributionKind::DetailUi,
            });
        let first = compile(input(vec![owner.clone(), consumer.clone()])).unwrap();
        owner.manifest.resource_kinds.reverse();
        owner.manifest.dependencies.reverse();
        let second = compile(input(vec![consumer, owner])).unwrap();
        assert_eq!(first.canonical_json(), second.canonical_json());
        assert_eq!(first.package_digest(), second.package_digest());
    }
}
