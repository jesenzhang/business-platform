#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Release compatibility, proven with the real compiler on the real pilot
//! declaration (no fixture invented for the test).
//!
//! Cases: C-1 (a fresh installation of the current candidate plans `AddModule`
//! and reports the same digest the release identity pins), C-2 (a supported
//! earlier revision of the same contract plans `UpgradeModule`), C-3 (an
//! unsatisfiable dependency range and a platform outside the window are both
//! refused), C-4 (a downgrade is refused), C-5 (what `^0.1.0` means against a
//! pre-release — because this release *is* a pre-release), C-6 (an unknown
//! dependency is refused), D-1 (only a change to the published contract moves
//! the contract digest).

use std::collections::BTreeMap;

use business_application_compiler::{
    compile, contract_digest, dry_plan, package_fingerprint, BusinessApplicationCompilerInput,
    BusinessApplicationPackage, CompilationError, CurrentModuleSnapshot, CurrentRegistrySnapshot,
    PackageChange,
};
use business_module_contracts::{
    BusinessModuleId, BusinessModuleVersion, DataClassification, ModuleDataState, ModuleDependency,
    ModuleInstallationState,
};
use module_release_tool::repo::RepoContext;
use module_release_tool::spec::ReleaseSpec;
use semver::{Version, VersionReq};

fn context() -> RepoContext {
    RepoContext::discover().expect("repository context")
}

/// The pilot declaration as the compiler sees it, exactly as released.
fn pilot(module: &str) -> BusinessApplicationPackage {
    let context = context();
    let spec = ReleaseSpec::load(&context, module).expect("release spec");
    BusinessApplicationPackage {
        manifest: spec.manifest().expect("manifest"),
        contributions: business_module_contracts::TypedContributionSet::default(),
        extension_points: Vec::new(),
        extension_contributions: Vec::new(),
    }
}

fn compile_packages(
    packages: Vec<BusinessApplicationPackage>,
    platform_version: &str,
    installed: Vec<(&str, &str)>,
) -> Result<
    business_application_compiler::CompiledBusinessApplicationManifest,
    business_application_compiler::CompilationError,
> {
    let mut installed_versions = BTreeMap::new();
    for (module_id, version) in installed {
        installed_versions.insert(
            BusinessModuleId::new(module_id).expect("module id"),
            BusinessModuleVersion::new(version).expect("version"),
        );
    }
    compile(BusinessApplicationCompilerInput {
        platform_version: platform_version.to_owned(),
        packages,
        installed_versions,
        ..BusinessApplicationCompilerInput::default()
    })
}

fn snapshot(packages: &[BusinessApplicationPackage]) -> CurrentRegistrySnapshot {
    CurrentRegistrySnapshot {
        modules: packages
            .iter()
            .map(|package| CurrentModuleSnapshot {
                package: package.clone(),
                package_digest: package_fingerprint(package).expect("fingerprint"),
                installation_state: ModuleInstallationState::Enabled,
                data_state: ModuleDataState::Retained,
            })
            .collect(),
    }
}

#[test]
fn a_fresh_installation_of_the_candidate_plans_add_module_with_the_pinned_digest() {
    for module in ["identity", "policy"] {
        let package = pilot(module);
        let compiled = compile_packages(vec![package.clone()], "0.1.0", Vec::new())
            .expect("the candidate compiles");
        let plan = dry_plan(&CurrentRegistrySnapshot::default(), &compiled).expect("plan");
        assert!(plan.applicable, "the plan is not applicable: {plan:?}");
        let added = plan
            .changes
            .iter()
            .find_map(|change| match change {
                PackageChange::AddModule { module_id, digest } if module_id.as_str() == module => {
                    Some(digest.clone())
                }
                _ => None,
            })
            .expect("the planner adds the module");
        assert_eq!(
            added.as_str(),
            package_fingerprint(&package).expect("fingerprint").as_str(),
            "the planner must report the same package digest the release identity pins"
        );
    }
}

#[test]
fn an_earlier_revision_of_the_same_contract_upgrades_in_place() {
    let current = pilot("identity");
    let contract_now = contract_digest(&current).expect("contract digest");

    let mut previous = pilot("identity");
    previous.manifest.module_version = BusinessModuleVersion::new("0.1.0-rc.0").expect("version");
    let contract_before = contract_digest(&previous).expect("contract digest");
    assert_eq!(
        contract_before.as_str(),
        contract_now.as_str(),
        "a release-layer version bump must not move the contract digest"
    );
    assert_ne!(
        package_fingerprint(&previous).expect("fp").as_str(),
        package_fingerprint(&current).expect("fp").as_str(),
        "…but the compiled declaration fingerprint must change"
    );

    let compiled = compile_packages(
        vec![current.clone()],
        "0.1.0",
        vec![("identity", "0.1.0-rc.0")],
    )
    .expect("an upgrade compiles");
    let plan = dry_plan(&snapshot(&[previous]), &compiled).expect("plan");
    assert!(
        plan.applicable,
        "the upgrade plan is not applicable: {plan:?}"
    );
    let upgraded = plan.changes.iter().any(|change| {
        matches!(change, PackageChange::UpgradeModule { module_id, .. }
            if module_id.as_str() == "identity")
    });
    assert!(
        upgraded,
        "expected an UpgradeModule change, got {:?}",
        plan.changes
    );
}

#[test]
fn an_unsatisfiable_dependency_range_is_refused() {
    let candidate = pilot("identity");
    let mut consumer = pilot("policy");
    consumer.manifest.module_id = BusinessModuleId::new("wave5a-consumer").expect("module id");
    consumer.manifest.module_version = BusinessModuleVersion::new("0.1.0").expect("version");
    consumer.manifest.migration_namespace = "wave5a-consumer".to_owned();
    consumer.manifest.dependencies = vec![ModuleDependency {
        module_id: BusinessModuleId::new("identity").expect("module id"),
        // The candidate is a pre-release; a caret range written this way does
        // not include it (see the semver facts below).
        version_requirement: "^0.1.0".to_owned(),
    }];
    consumer.manifest.published_commands = Vec::new();
    consumer.manifest.published_queries = Vec::new();

    let error = compile_packages(vec![candidate, consumer], "0.1.0", Vec::new())
        .expect_err("an unsatisfiable range must be refused");
    assert!(
        matches!(error, CompilationError::IncompatibleDependency { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn a_platform_outside_the_declared_window_is_refused() {
    let error = compile_packages(vec![pilot("identity")], "0.0.9", Vec::new())
        .expect_err("a platform below the declared minimum must be refused");
    assert!(
        matches!(error, CompilationError::IncompatiblePlatform { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn a_downgrade_is_refused() {
    let error = compile_packages(
        vec![pilot("identity")],
        "0.1.0",
        vec![("identity", "0.1.0")],
    )
    .expect_err("0.1.0-rc.1 over an installed 0.1.0 is a downgrade");
    assert!(
        matches!(error, CompilationError::Downgrade { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn an_unknown_dependency_is_refused() {
    let mut candidate = pilot("identity");
    candidate.manifest.dependencies = vec![ModuleDependency {
        module_id: BusinessModuleId::new("does-not-exist").expect("module id"),
        version_requirement: "^0.1.0".to_owned(),
    }];
    let error = compile_packages(vec![candidate], "0.1.0", Vec::new())
        .expect_err("a dependency on an absent module must be refused");
    assert!(
        matches!(error, CompilationError::UnknownDependency { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn the_caret_range_a_reader_would_reach_for_does_not_cover_this_prerelease() {
    let released = Version::parse("0.1.0-rc.1").expect("version");
    assert!(
        !VersionReq::parse("^0.1.0")
            .expect("requirement")
            .matches(&released),
        "if this ever becomes true, the dependency-range rule in the record must be re-derived"
    );
    assert!(VersionReq::parse("=0.1.0-rc.1")
        .expect("requirement")
        .matches(&released));
    assert!(VersionReq::parse("^0.1.0-rc.1")
        .expect("requirement")
        .matches(&released));
    assert!(!VersionReq::parse("=0.1.0-rc.1")
        .expect("requirement")
        .matches(&Version::parse("0.1.0-rc.2").expect("version")));
}

#[test]
fn only_a_change_to_the_published_contract_moves_the_contract_digest() {
    let released = pilot("identity");
    let contract = contract_digest(&released).expect("contract digest");
    let package = package_fingerprint(&released).expect("package fingerprint");

    let mut narrower_window = pilot("identity");
    narrower_window
        .manifest
        .compatibility
        .minimum_platform_version = Some("0.2.0".to_owned());
    assert_eq!(
        contract_digest(&narrower_window)
            .expect("contract digest")
            .as_str(),
        contract.as_str(),
        "the compatibility window is not part of the cross-module contract surface"
    );
    assert_ne!(
        package_fingerprint(&narrower_window)
            .expect("fingerprint")
            .as_str(),
        package.as_str(),
        "…but it is part of the compiled declaration"
    );

    let mut reclassified = pilot("identity");
    reclassified
        .manifest
        .data_classification
        .push(DataClassification::Restricted);
    assert_ne!(
        contract_digest(&reclassified)
            .expect("contract digest")
            .as_str(),
        contract.as_str(),
        "the data classifications a module may emit are part of its contract surface"
    );
    assert_ne!(
        package_fingerprint(&reclassified)
            .expect("fingerprint")
            .as_str(),
        package.as_str()
    );
}
