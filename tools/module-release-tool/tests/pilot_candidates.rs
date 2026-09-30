#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! The two pilot candidates, checked as candidates rather than as fixtures.
//!
//! Cases: B-1 (independent verification traces the source revision),
//! D-2/D-3/D-4/D-5 (a changed member, a missing member, a wrong module id and a
//! wrong module version are all refused), D-7 (the compiler's identity
//! consistency gate refuses every single-field mismatch), E-1 (the migration
//! mapping equals the formal catalog), E-2 (mapping/catalog inconsistency is
//! refused), E-3 (a stateless module pins no migration revision).

use std::fs;
use std::path::Path;

use business_application_compiler::{
    verify_release_identity, CompiledBusinessApplicationManifest, ReleaseIdentityMismatch,
};
use business_module_contracts::{
    BusinessModuleId, BusinessModuleVersion, ContractDigest, ModuleReleaseIdentity, PackageDigest,
};
use module_release_tool::migrations::{build_mapping, verify_catalog};
use module_release_tool::packaging::{generate, verify};
use module_release_tool::repo::RepoContext;
use module_release_tool::spec::ReleaseSpec;
use module_release_tool::tempdir::TempDir;
use module_release_tool::{surface, RELEASE_IDENTITY_FILE};

const MODULES: [&str; 2] = ["identity", "policy"];

fn context() -> RepoContext {
    RepoContext::discover().expect("repository context")
}

fn artifact(
    module: &str,
    tag: &str,
) -> (TempDir, module_release_tool::packaging::GeneratedArtifact) {
    let out = TempDir::new(tag).expect("temp dir");
    let artifact = generate(&context(), module, out.path()).expect("generate");
    (out, artifact)
}

fn payload(artifact: &Path, member: &str) -> std::path::PathBuf {
    artifact.join("package").join(member.replace('/', "\\"))
}

fn read_identity(artifact: &Path) -> ModuleReleaseIdentity {
    let text = fs::read_to_string(artifact.join(RELEASE_IDENTITY_FILE)).expect("release.json");
    serde_json::from_str(&text).expect("release.json parses as a release identity")
}

fn compiled_evidence(module: &str) -> CompiledBusinessApplicationManifest {
    let context = context();
    let spec = ReleaseSpec::load(&context, module).expect("release spec");
    let package = business_application_compiler::BusinessApplicationPackage {
        manifest: spec.manifest().expect("manifest"),
        contributions: business_module_contracts::TypedContributionSet::default(),
        extension_points: Vec::new(),
        extension_contributions: Vec::new(),
    };
    surface::compile_single(package, context.platform_version()).expect("compile")
}

fn write_json(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(value).expect("serialize")).expect("write");
}

#[test]
fn every_pilot_candidate_verifies_against_the_repository_and_traces_its_revision() {
    let context = context();
    for module in MODULES {
        let (_out, candidate) = artifact(module, "verify");
        let index = verify(&context, &candidate.directory).expect("verification");
        assert_eq!(index.module_id, module);
        let ids: Vec<&str> = index.checks.iter().map(|check| check.id.as_str()).collect();
        for expected in [
            "artifact-record-format",
            "artifact-integrity-digest",
            "contract-surface-file-digest",
            "release-identity-consistency",
            "record-consistency",
            "packed-crate-bindings",
            "migration-mapping-matches-formal-catalog",
            "source-revision-exists",
            // Repair 2 split provenance into the three claims it actually makes:
            // which members exist, whether the copied ones are the commit's bytes,
            // and whether the derived ones are what the commit derives.
            "source-member-set-matches-revision",
            "source-inputs-match-revision",
            "derived-members-match-revision",
            "toolchain-declaration",
        ] {
            assert!(
                ids.contains(&expected),
                "verification skipped the '{expected}' check; ran {ids:?}"
            );
        }
        assert_eq!(
            ids.len(),
            12,
            "the verification index grew or shrank without this test being updated: {ids:?}"
        );
        let provenance = index
            .checks
            .iter()
            .find(|check| check.id == "source-inputs-match-revision")
            .expect("provenance input check");
        assert!(
            provenance.detail.contains(context.revision())
                && provenance.detail.contains("payload member"),
            "the input check has to say what it compared: {}",
            provenance.detail
        );
        for check in &index.checks {
            assert_eq!(
                check.outcome, "PASS",
                "check '{}' did not pass: {}",
                check.id, check.detail
            );
        }
        let identity = read_identity(&candidate.directory);
        assert_eq!(
            identity.source_revision.as_str(),
            context.revision(),
            "the candidate must pin the revision it was really built from"
        );
        assert_eq!(identity.module_id.as_str(), module);
        assert!(
            identity.migration_revision.is_some(),
            "both pilots are persistent modules and must pin a migration revision"
        );
    }
}

#[test]
fn changing_one_packed_source_breaks_the_artifact() {
    let context = context();
    let (_out, candidate) = artifact("identity", "tamper-content");
    let packed = payload(
        &candidate.directory,
        "contract/packages/identity/src/lib.rs",
    );
    let mut text = fs::read_to_string(&packed).expect("packed lib.rs");
    text.push_str("// one byte inside the published contract source\n");
    fs::write(&packed, text).expect("tamper");
    let error = verify(&context, &candidate.directory).expect_err("a tampered payload must fail");
    assert!(
        error.to_string().contains("artifact-integrity-digest"),
        "unexpected failure: {error}"
    );
}

#[test]
fn removing_one_payload_member_breaks_the_artifact() {
    let context = context();
    let (_out, candidate) = artifact("identity", "tamper-missing");
    fs::remove_file(payload(&candidate.directory, "CHANGELOG.md")).expect("remove");
    let error = verify(&context, &candidate.directory).expect_err("a missing member must fail");
    let message = error.to_string();
    assert!(
        message.contains("artifact-integrity-digest") || message.contains("artifact-inventory"),
        "unexpected failure: {message}"
    );
}

#[test]
fn a_wrong_module_id_in_the_pinned_identity_is_refused() {
    let context = context();
    let (_out, candidate) = artifact("identity", "tamper-id");
    let path = candidate.directory.join(RELEASE_IDENTITY_FILE);
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("release.json")).expect("json");
    value["module_id"] = serde_json::Value::String("organization".to_owned());
    write_json(&path, &value);
    let error = verify(&context, &candidate.directory).expect_err("a wrong module id must fail");
    assert!(
        !error.to_string().is_empty(),
        "the refusal must carry a reason"
    );
}

#[test]
fn a_wrong_module_version_in_the_pinned_identity_is_refused() {
    let context = context();
    let (_out, candidate) = artifact("identity", "tamper-version");
    let path = candidate.directory.join(RELEASE_IDENTITY_FILE);
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("release.json")).expect("json");
    value["module_version"] = serde_json::Value::String("9.9.9".to_owned());
    write_json(&path, &value);
    let error = verify(&context, &candidate.directory).expect_err("a wrong version must fail");
    assert!(error.to_string().contains("9.9.9") || error.to_string().contains("identity"));
}

#[test]
fn a_tampered_migration_mapping_is_refused() {
    let context = context();
    let (_out, candidate) = artifact("policy", "tamper-mapping");
    let path = payload(&candidate.directory, "migrations/migration-mapping.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("mapping")).expect("json");
    let first = &mut value["files"][0]["sha256"];
    *first = serde_json::Value::String("0".repeat(64));
    write_json(&path, &value);
    let error = verify(&context, &candidate.directory).expect_err("a doctored mapping must fail");
    assert!(
        error.to_string().contains("artifact-integrity-digest")
            || error.to_string().contains("migration-mapping"),
        "unexpected failure: {error}"
    );
}

#[test]
fn the_identity_consistency_gate_refuses_every_single_field_mismatch() {
    for module in MODULES {
        let (_out, candidate) = artifact(module, "identity-gate");
        let compiled = compiled_evidence(module);
        let identity = read_identity(&candidate.directory);
        verify_release_identity(&compiled, &identity)
            .expect("the artifact's own identity verifies");

        let mut unknown_module = identity.clone();
        unknown_module.module_id = BusinessModuleId::new("organization").expect("module id");
        assert!(matches!(
            verify_release_identity(&compiled, &unknown_module),
            Err(ReleaseIdentityMismatch::UnknownModule { .. })
        ));

        let mut wrong_version = identity.clone();
        wrong_version.module_version = BusinessModuleVersion::new("9.9.9").expect("version");
        assert!(matches!(
            verify_release_identity(&compiled, &wrong_version),
            Err(ReleaseIdentityMismatch::ModuleVersion { .. })
        ));

        let mut wrong_contract = identity.clone();
        wrong_contract.contract_digest =
            ContractDigest::new("f".repeat(64)).expect("contract digest");
        assert!(matches!(
            verify_release_identity(&compiled, &wrong_contract),
            Err(ReleaseIdentityMismatch::ContractDigest { .. })
        ));

        let mut wrong_package = identity.clone();
        wrong_package.package_digest = PackageDigest::new("e".repeat(64)).expect("package digest");
        assert!(matches!(
            verify_release_identity(&compiled, &wrong_package),
            Err(ReleaseIdentityMismatch::PackageDigest { .. })
        ));
    }
}

#[test]
fn the_migration_mapping_equals_the_formal_catalog() {
    let context = context();
    for module in MODULES {
        let spec = ReleaseSpec::load(&context, module).expect("release spec");
        let mapping = build_mapping(&context, &spec).expect("mapping");
        assert!(
            !mapping.files.is_empty(),
            "{module} pins no migration files"
        );
        for file in &mapping.files {
            let manifest = context
                .root()
                .join(file.catalog_manifest.replace('/', "\\"));
            let recorded = fs::read_to_string(manifest)
                .expect("catalog manifest")
                .lines()
                .map(|line: &str| line.trim_end_matches('\r'))
                .find(|line| line.ends_with(&format!(" {}", file.file_name)))
                .unwrap_or_else(|| {
                    panic!("'{}' is absent from its catalog manifest", file.file_name)
                })
                .split("  ")
                .next()
                .expect("digest column")
                .to_owned();
            assert_eq!(
                recorded, file.sha256,
                "{}: the mapping does not carry the catalog's own checksum",
                file.file_name
            );
            let bytes = fs::read(
                context
                    .root()
                    .join(file.catalog.replace('/', "\\"))
                    .join(&file.file_name),
            )
            .expect("catalog file");
            let raw = module_release_tool::canonical::sha256_hex(&bytes);
            let folded = module_release_tool::canonical::sha256_hex(
                &module_release_tool::canonical::fold_line_endings(&bytes),
            );
            assert!(
                (raw == recorded && file.digest_form == "raw")
                    || (folded == recorded && file.digest_form == "lf-normalized"),
                "{}: recorded {} / raw {raw} / lf {folded}",
                file.file_name,
                file.sha256
            );
        }
    }
}

#[test]
fn a_migration_claim_that_the_catalog_does_not_support_is_refused() {
    let context = context();
    let spec = ReleaseSpec::load(&context, "identity").expect("release spec");

    let mut invented = spec.clone();
    invented
        .migration_sets
        .iter_mut()
        .find(|set| set.backend == "sqlite")
        .expect("the identity pilot declares a sqlite migration set")
        .files
        .push("999_invented_migration.sql".to_owned());
    let error = build_mapping(&context, &invented).expect_err("an invented file must be refused");
    assert!(error.to_string().contains("999_invented_migration.sql"));

    let mut claims_shared_catalog = spec.clone();
    let postgres = claims_shared_catalog
        .migration_sets
        .iter_mut()
        .find(|set| set.backend == "postgres")
        .expect("the identity pilot declares a postgres migration set");
    postgres.module_owned = true;
    let error = verify_catalog(context.root(), postgres)
        .expect_err("sole ownership of the shared workspace catalog must be refused");
    assert!(error.to_string().contains("module-owned"));
}

#[test]
fn a_stateless_module_pins_no_migration_revision() {
    let context = context();
    let spec = ReleaseSpec::load(&context, "identity").expect("release spec");

    let mut stateless = spec.clone();
    stateless.persistent = false;
    stateless.migration_sets.clear();
    let mapping = build_mapping(&context, &stateless).expect("stateless mapping");
    assert!(mapping.files.is_empty(), "a stateless module pins no files");
    assert!(
        mapping.revision().expect("revision").is_none(),
        "a stateless module must not pin a migration revision"
    );

    let text = fs::read_to_string(
        context
            .root()
            .join("tools\\module-release-tool\\specs\\identity.json"),
    )
    .expect("spec file");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("spec json");
    value["migration_sets"] = serde_json::Value::Array(Vec::new());
    let error = ReleaseSpec::from_json(&value.to_string(), "identity", "memory://test")
        .expect_err("persistent with no migration sets is not a valid declaration");
    assert!(error.to_string().contains("persistent"));
}
