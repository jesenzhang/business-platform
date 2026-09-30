#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Regressions for the Wave 5A **Repair 2** review (findings F1-A and F1-B).
//!
//! Repair 1 earned two claims and asserted a third it had not proved. It earned
//! "the packed inputs equal the blobs of the pinned commit" and "the payload is
//! physically complete and self-consistent". It did not earn "the derived products
//! were derived *from that commit*", because:
//!
//! * **F1-A** — the packed `Cargo.toml`, the shipped module manifest, the contract
//!   surface and the migration mapping were never recomputed from the pinned
//!   commit. Rewriting a dependency version in a packed manifest and recomputing
//!   the physical digest left `verify` reporting `verified` (review §3).
//! * **F1-B** — the expected source set was enumerated from the **disk**, so a
//!   tracked file deleted from the checkout was missing on both sides of the
//!   comparison: generation succeeded with 20 members instead of 21 and the
//!   candidate then failed to build with `E0583` (review §4).
//!
//! Every case here therefore starts from a candidate that verifies, damages one
//! thing in an independent copy, makes the *physical* record correct again with the
//! published `plan-0014.artifact-integrity.v1` algorithm, and asserts that the
//! refusal arrives under the identifier that owns the claim under attack. The
//! physical re-assertion is part of the test on purpose: if the digest stopped
//! matching, the case would be about physical integrity and would prove nothing
//! about derivation.
//!
//! The identifiers are literals rather than imports so this file states what the
//! tool owes, not what it happens to implement.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use business_application_compiler::{
    contract_digest, package_fingerprint, BusinessApplicationPackage,
};
use business_module_contracts::TypedContributionSet;
use module_release_tool::canonical::{artifact_integrity_digest, inventory};
use module_release_tool::packaging::{generate, verify};
use module_release_tool::repo::RepoContext;
use module_release_tool::spec::{self, ReleaseSpec};
use module_release_tool::surface;
use module_release_tool::tempdir::TempDir;
use module_release_tool::{
    ToolError, ARTIFACT_MANIFEST_FILE, RELEASE_BUNDLE_FILE, RELEASE_IDENTITY_FILE,
};

/// Owns "the derived members are byte-for-byte what the pinned commit produces"
/// (review §5.4.2), kept separate from the physical integrity identifier (§5.4.5).
const DERIVED: &str = "derived-members-match-revision";
/// Owns "the payload holds exactly the members the pinned commit's tree produces":
/// dropped, missing and extra members alike (review §5.4.4).
const MEMBER_SET: &str = "source-member-set-matches-revision";

fn context() -> RepoContext {
    RepoContext::discover().expect("repository context")
}

fn candidate(module: &str, tag: &str) -> (TempDir, PathBuf) {
    let out = TempDir::new(tag).expect("temp dir");
    let generated = generate(&context(), module, out.path()).expect("generate");
    (out, generated.directory)
}

/// Each case damages its own copy of a candidate that just verified, so one
/// injected fault can never be credited for another's.
fn pristine_copy(artifact: &Path, tag: &str) -> (TempDir, PathBuf) {
    verify(&context(), artifact).expect("the source candidate must verify before it is damaged");
    let out = TempDir::new(tag).expect("temp dir");
    let target = out.path().join(
        artifact
            .file_name()
            .expect("artifact directory name")
            .to_string_lossy()
            .as_ref(),
    );
    copy_tree(artifact, &target);
    (out, target)
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create directory");
    for entry in fs::read_dir(from).expect("read directory") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        let destination = to.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &destination);
        } else {
            fs::copy(&path, &destination).expect("copy file");
        }
    }
}

fn member_path(artifact: &Path, member: &str) -> PathBuf {
    artifact.join("package").join(member.replace('/', "\\"))
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("parse json")
}

fn write_json(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(value).expect("serialize")).expect("write");
}

fn check_of(error: &ToolError) -> String {
    error
        .to_string()
        .split('[')
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or("unclassified")
        .to_owned()
}

/// Re-lists the payload and rewrites the physical record using the tool's own
/// published v1 algorithm, so the damage below is *not* catchable as a physical
/// integrity failure. It recomputes; it judges nothing about provenance.
fn restore_physical_integrity(artifact: &Path) -> String {
    let files = inventory(&artifact.join("package")).expect("payload inventory");
    let digest = artifact_integrity_digest(&files);
    let path = artifact.join(ARTIFACT_MANIFEST_FILE);
    let mut record = read_json(&path);
    record["files"] = serde_json::to_value(&files).expect("serialise inventory");
    record["artifact_integrity_digest"] = serde_json::Value::String(digest.clone());
    write_json(&path, &record);
    digest
}

/// The evidence that a case is about derivation: after the injection and the
/// recomputation, the payload is still exactly what its own record claims.
fn assert_physically_intact(artifact: &Path) {
    let record = read_json(&artifact.join(ARTIFACT_MANIFEST_FILE));
    let recorded = record["artifact_integrity_digest"]
        .as_str()
        .expect("recorded digest")
        .to_owned();
    let files = inventory(&artifact.join("package")).expect("payload inventory");
    assert_eq!(
        artifact_integrity_digest(&files),
        recorded,
        "the recomputed digest must still equal the record, or this case is about physical \
         integrity rather than about derivation"
    );
    let listed: Vec<String> = record["files"]
        .as_array()
        .expect("file list")
        .iter()
        .map(|file| file["path"].as_str().expect("path").to_owned())
        .collect();
    let on_disk: Vec<String> = files.into_iter().map(|file| file.path).collect();
    assert_eq!(
        listed, on_disk,
        "the record must list exactly the payload members"
    );
}

fn assert_passed(index: &module_release_tool::packaging::VerificationIndex, id: &str) {
    assert!(
        index
            .checks
            .iter()
            .any(|check| check.id == id && check.outcome == "PASS"),
        "'{id}' must appear as PASS: {:?}",
        index
            .checks
            .iter()
            .map(|check| (check.id.clone(), check.outcome.clone()))
            .collect::<Vec<_>>()
    );
}

/// Walks one directory tree, returning repository-relative POSIX paths.
fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read source directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            walk(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("relative path")
            .to_string_lossy()
            .replace('\\', "/");
        out.push(relative);
    }
}

/// Every repository file a candidate for `module` is built from, so an isolated
/// input copy can hold exactly those and nothing else.
fn input_paths(module: &str) -> Vec<String> {
    let context = context();
    let spec = ReleaseSpec::load(&context, module).expect("release spec");
    let mut paths = vec![
        "Cargo.toml".to_owned(),
        "Cargo.lock".to_owned(),
        "rust-toolchain.toml".to_owned(),
        spec::spec_path(module),
        spec.changelog_file.clone(),
    ];
    for crate_spec in &spec.packed_crates {
        paths.push(format!("{}/Cargo.toml", crate_spec.source_dir));
        let source = context.root().join(&crate_spec.source_dir).join("src");
        let mut files = Vec::new();
        walk(&source, context.root(), &mut files);
        paths.append(&mut files);
    }
    for set in &spec.migration_sets {
        paths.push(set.catalog_manifest.clone());
        for file in &set.files {
            paths.push(format!("{}/{}", set.catalog, file));
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn copy_inputs(module: &str, into: &Path) {
    let context = context();
    for path in input_paths(module) {
        let from = context.root().join(path.replace('/', "\\"));
        assert!(from.is_file(), "'{path}' is missing from the working tree");
        let to = into.join(path.replace('/', "\\"));
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::copy(&from, &to).unwrap_or_else(|source| panic!("copy '{path}': {source}"));
    }
}

/// A checkout of its own: the input files, plus a `gitdir:` pointer to this
/// repository's object database, so the copy resolves the *same* commit while the
/// files on disk are its own. The real working tree is never touched.
fn isolated_checkout(tag: &str, module: &str) -> (TempDir, PathBuf) {
    let context = context();
    let out = TempDir::new(tag).expect("temp dir");
    let copy = out.path().join("checkout");
    fs::create_dir_all(&copy).expect("create checkout");
    fs::write(
        copy.join(".git"),
        format!(
            "gitdir: {}\n",
            context
                .root()
                .join(".git")
                .display()
                .to_string()
                .replace('\\', "/")
        ),
    )
    .expect("write git pointer");
    copy_inputs(module, &copy);
    let head = Command::new("git")
        .arg("-C")
        .arg(&copy)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    assert_eq!(
        String::from_utf8_lossy(&head.stdout).trim(),
        context.revision(),
        "the isolated checkout must sit on the same commit: {}",
        String::from_utf8_lossy(&head.stderr)
    );
    (out, copy)
}

/// Rewrites the first pinned dependency version in one packed manifest, so the
/// injection is a property of the artifact rather than a hard-coded literal.
fn bump_first_dependency_version(manifest: &Path) -> String {
    let text = fs::read_to_string(manifest).expect("packed manifest");
    let marker = "version = \"=";
    let mut replaced = String::new();
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut line = line.to_owned();
        if replaced.is_empty() {
            if let Some(at) = line.find(marker) {
                let start = at + marker.len();
                if let Some(end) = line[start..].find('"') {
                    // Guarded by `replaced.is_empty()` above, so this is the only
                    // write and recording the original pin needs no assignment.
                    replaced.push_str(&line[start..start + end]);
                    line.replace_range(start..start + end, "99.0.0");
                }
            }
        }
        lines.push(line);
    }
    assert!(
        !replaced.is_empty(),
        "the packed manifest pins no dependency version to rewrite"
    );
    fs::write(manifest, format!("{}\n", lines.join("\n"))).expect("write packed manifest");
    replaced
}

// -------------------------------------------------------- F1-A: derivation --

#[test]
fn a_packed_manifest_the_pinned_commit_cannot_produce_is_refused() {
    let (_out, artifact) = candidate("identity", "f1a-cargo");
    for (title, from, to) in [
        (
            "a dependency version the commit does not pin",
            "async-trait = { version = \"=0.1.92\" }",
            "async-trait = { version = \"=99.0.0\" }",
        ),
        (
            "features the commit does not declare",
            "uuid = { version = \"=1.26.0\", features = [\"serde\", \"v4\", \"v5\", \"v7\"] }",
            "uuid = { version = \"=1.26.0\", features = [\"serde\", \"v4\", \"v5\", \"v7\", \"v9\"] }",
        ),
        (
            "a path dependency pointing back into a source tree",
            "thiserror = { version = \"=2.0.20\" }",
            "thiserror = { version = \"=2.0.20\", path = \"../../../../crates/thiserror\" }",
        ),
    ] {
        let (_copy, damaged) = pristine_copy(&artifact, "f1a-cargo-copy");
        let manifest = member_path(&damaged, "contract/packages/identity/Cargo.toml");
        let before = fs::read_to_string(&manifest).expect("packed manifest");
        assert!(before.contains(from), "the fixture moved: '{from}' is gone");
        fs::write(&manifest, before.replace(from, to)).expect("inject");
        assert_ne!(
            fs::read_to_string(&manifest).expect("read back"),
            before,
            "the {title} injection did not change a single byte"
        );
        restore_physical_integrity(&damaged);
        assert_physically_intact(&damaged);

        let error = verify(&context(), &damaged).expect_err(&format!(
            "{title}: a packed manifest the pinned commit cannot produce must not verify"
        ));
        assert_eq!(check_of(&error), DERIVED, "got: {error}");
        assert!(
            error.to_string().contains("contract/packages/identity/Cargo.toml"),
            "the refusal must name the member: {error}"
        );
    }
}

#[test]
fn a_packed_dependency_bump_is_refused_for_every_pilot_module() {
    // Not a rule about one crate or one literal: whichever pilot candidate stops
    // matching what its own commit produces has to be refused.
    for module in ["identity", "policy"] {
        let (_out, artifact) = candidate(module, "f1a-any");
        let (_copy, damaged) = pristine_copy(&artifact, "f1a-any-copy");
        let manifest = member_path(&damaged, &format!("contract/packages/{module}/Cargo.toml"));
        let bumped = bump_first_dependency_version(&manifest);
        restore_physical_integrity(&damaged);
        assert_physically_intact(&damaged);

        let error = match verify(&context(), &damaged) {
            Ok(index) => {
                panic!("{module}: rewriting '{bumped}' as 99.0.0 must be refused, got {index:?}")
            }
            Err(error) => error,
        };
        assert_eq!(check_of(&error), DERIVED, "{module}: got: {error}");
    }
}

#[test]
fn a_manifest_compiled_from_a_different_spec_is_refused_even_when_self_consistent() {
    // `verify` recompiles the declaration the candidate ships, so a candidate whose
    // manifest is internally consistent with the digests pinned beside it passes
    // every declaration check. Only recompiling the *commit's own spec* shows that
    // the shipped declaration is not the one that commit declares.
    let (_out, artifact) = candidate("identity", "f1a-spec");
    let (_copy, damaged) = pristine_copy(&artifact, "f1a-spec-copy");

    let spec_file = context()
        .root()
        .join(spec::spec_path("identity").replace('/', "\\"));
    let text = fs::read_to_string(&spec_file).expect("release spec");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("spec json");
    value["published_queries"][0]["version"] = serde_json::Value::String("9.9.9".to_owned());
    let tampered = ReleaseSpec::from_json(&value.to_string(), "identity", "memory://review-spec")
        .expect("the tampered spec must still be a well-formed spec");
    let package = BusinessApplicationPackage {
        manifest: tampered.manifest().expect("tampered manifest"),
        contributions: TypedContributionSet::default(),
        extension_points: Vec::new(),
        extension_contributions: Vec::new(),
    };
    let compiled = surface::compile_single(package, context().platform_version())
        .expect("the tampered spec must still compile");
    let shipped = compiled
        .packages
        .first()
        .expect("one package for one module");
    let surface_bytes = surface::contract_surface_bytes(shipped).expect("contract surface");
    let contract = contract_digest(shipped)
        .expect("contract digest")
        .to_string();
    let package_digest = package_fingerprint(shipped)
        .expect("package digest")
        .to_string();

    fs::write(
        member_path(&damaged, "module.manifest.json"),
        serde_json::to_vec(&shipped.manifest).expect("serialise manifest"),
    )
    .expect("write the tampered manifest");
    fs::write(
        member_path(&damaged, "contract/contract-surface.json"),
        surface_bytes,
    )
    .expect("write the tampered surface");
    for file in [RELEASE_IDENTITY_FILE, RELEASE_BUNDLE_FILE] {
        let path = damaged.join(file);
        let mut record = read_json(&path);
        let target = if file == RELEASE_IDENTITY_FILE {
            record.as_object_mut().expect("identity record")
        } else {
            record["identity"]
                .as_object_mut()
                .expect("bundled identity")
        };
        target["contract_digest"] = serde_json::Value::String(contract.clone());
        target["package_digest"] = serde_json::Value::String(package_digest.clone());
        write_json(&path, &record);
    }
    let record_path = damaged.join(ARTIFACT_MANIFEST_FILE);
    let mut record = read_json(&record_path);
    record["contract_digest"] = serde_json::Value::String(contract);
    record["package_digest"] = serde_json::Value::String(package_digest);
    write_json(&record_path, &record);
    restore_physical_integrity(&damaged);
    assert_physically_intact(&damaged);

    let error = verify(&context(), &damaged).expect_err(
        "a candidate whose declaration is not the pinned commit's must not verify just because \
         it is self-consistent",
    );
    assert_eq!(check_of(&error), DERIVED, "got: {error}");
}

// ----------------------------------------------------- F1-B: member set -----

#[test]
fn generating_from_a_checkout_that_dropped_a_committed_source_file_is_refused() {
    let (_out, copy) = isolated_checkout("f1b-missing", "identity");
    let dropped = copy.join("crates\\identity\\src\\testing.rs");
    assert!(
        dropped.is_file(),
        "the fixture starts from a complete checkout of the packed crate"
    );
    fs::remove_file(&dropped).expect("delete the tracked source file");

    let isolated = RepoContext::with_root(copy).expect("isolated context");
    let out = TempDir::new("f1b-missing-out").expect("temp dir");
    let error = generate(&isolated, "identity", out.path())
        .expect_err("a committed source file missing from the checkout must not generate");
    assert_eq!(check_of(&error), MEMBER_SET, "got: {error}");
    assert!(
        error.to_string().contains("testing.rs"),
        "the refusal must name the missing file: {error}"
    );
    assert!(
        !out.path().join("identity-0.1.0-rc.1").exists(),
        "a refused generation must not leave a candidate directory behind"
    );
}

#[test]
fn generating_from_a_checkout_with_an_uncommitted_source_file_is_refused() {
    let (_out, copy) = isolated_checkout("f1b-extra", "identity");
    fs::write(
        copy.join("crates\\identity\\src\\review_untracked.rs"),
        "// never committed\n",
    )
    .expect("write an extra source file");

    let isolated = RepoContext::with_root(copy).expect("isolated context");
    let out = TempDir::new("f1b-extra-out").expect("temp dir");
    let error = generate(&isolated, "identity", out.path())
        .expect_err("a file the pinned commit does not contain must not become a member");
    assert_eq!(check_of(&error), MEMBER_SET, "got: {error}");
    assert!(
        error.to_string().contains("review_untracked.rs"),
        "the refusal must name the extra file: {error}"
    );
    assert!(
        !out.path().join("identity-0.1.0-rc.1").exists(),
        "a refused generation must not leave a candidate directory behind"
    );
}

#[test]
fn a_candidate_missing_a_committed_source_member_is_refused_after_rehashing() {
    let (_out, artifact) = candidate("identity", "f1b-drop");
    let (_copy, damaged) = pristine_copy(&artifact, "f1b-drop-copy");
    let member = member_path(&damaged, "contract/packages/identity/src/testing.rs");
    assert!(member.is_file(), "the shipped candidate carries the member");
    fs::remove_file(&member).expect("drop the member");
    restore_physical_integrity(&damaged);
    assert_physically_intact(&damaged);

    let error = verify(&context(), &damaged)
        .expect_err("a source file the commit contains may not simply be absent");
    assert_eq!(check_of(&error), MEMBER_SET, "got: {error}");
    assert!(
        error.to_string().contains("testing.rs"),
        "the refusal must name the missing member: {error}"
    );
}

#[test]
fn a_candidate_with_an_extra_source_member_is_refused_after_rehashing() {
    let (_out, artifact) = candidate("identity", "f1b-extra-member");
    let (_copy, damaged) = pristine_copy(&artifact, "f1b-extra-member-copy");
    fs::write(
        member_path(
            &damaged,
            "contract/packages/identity/src/review_invented.rs",
        ),
        "// invented by the reviewer\n",
    )
    .expect("add a member");
    restore_physical_integrity(&damaged);
    assert_physically_intact(&damaged);

    let error = verify(&context(), &damaged)
        .expect_err("a payload member the pinned commit does not contain must be refused");
    assert_eq!(check_of(&error), MEMBER_SET, "got: {error}");
    assert!(
        error.to_string().contains("review_invented.rs"),
        "the refusal must name the extra member: {error}"
    );
}

// ----------------------------------------------------------- §5.5 compat ----

#[test]
fn a_pinned_candidate_still_verifies_after_the_working_tree_inputs_move() {
    // Verification is a statement about the pinned commit. The checkout it happens
    // to run in must neither change that statement nor be trusted in its place.
    let (_out, artifact) = candidate("identity", "compat-move");
    let (_copy, checkout) = isolated_checkout("compat-move-checkout", "identity");
    fs::write(
        checkout.join("crates\\identity\\Cargo.toml"),
        "# edited after the candidate was cut\n",
    )
    .expect("modify a feeding input");
    fs::write(
        checkout.join("crates\\identity\\src\\review_untracked.rs"),
        "// uncommitted\n",
    )
    .expect("add an extra source file");
    fs::remove_file(checkout.join("crates\\identity\\src\\testing.rs")).expect("delete a source");

    let isolated = RepoContext::with_root(checkout).expect("isolated context");
    let index = verify(&isolated, &artifact)
        .expect("the pinned candidate must verify against the pinned commit, not the checkout");
    assert_passed(&index, DERIVED);
    assert_passed(&index, MEMBER_SET);
}

#[test]
fn a_candidate_whose_commit_object_is_absent_is_refused_without_a_disk_fallback() {
    // Its own repository, its own commit, the same files on disk: the pinned
    // revision simply is not here, and reading the working tree instead is not an
    // answer the tool may fall back to.
    let (_out, artifact) = candidate("identity", "compat-no-commit");
    let scratch = TempDir::new("compat-no-commit-repo").expect("temp dir");
    let root = scratch.path().join("repo");
    fs::create_dir_all(&root).expect("create repository");
    copy_inputs("identity", &root);
    for arguments in [
        vec!["init", "-q", "--initial-branch=main", "."],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "different history",
        ],
    ] {
        let run = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(&arguments)
            .output()
            .expect("git");
        assert!(
            run.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    let other = RepoContext::with_root(root).expect("scratch context");
    let error = verify(&other, &artifact)
        .expect_err("a missing commit object must be refused; there is no fallback to the disk");
    assert_eq!(check_of(&error), "source-revision-exists", "got: {error}");
}
