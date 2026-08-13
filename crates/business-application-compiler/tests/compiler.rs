use business_application_compiler::{
    compile, BusinessApplicationCompilerInput, BusinessApplicationPackage, CompilationError,
};
use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, CompatibilityDescriptor,
    DataClassification, ManifestSchemaVersion, PublicContributionTarget, PublicTargetKind,
    ResourceKindDescriptor,
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
}
