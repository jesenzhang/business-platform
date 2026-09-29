#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Regressions for the Wave 5A independent review (F1, F2, F3).
//!
//! Each test reproduces a defect the review reported and asserts the refusal it
//! must now produce:
//!
//! * **F1** — a pinned `source_revision` that is not a real commit, and the same
//!   fake revision written into both identity records, must not verify.
//! * **F2** — `compare` must read the real payload of both sides instead of
//!   trusting each side's own inventory record.
//! * **F3** — the descriptive records must agree with each other, with the
//!   compiled evidence and with the packed crate manifests, and an unknown
//!   schema or algorithm must be refused.
//!
//! Note the deliberate shape of these fixtures: none of them touches the payload
//! bytes in a way the integrity digest can see (F3) or leaves only one side of a
//! comparison damaged (F2), so a tool that only hashes the payload passes them.

use std::fs;
use std::path::{Path, PathBuf};

use module_release_tool::packaging::{compare, generate, verify};
use module_release_tool::repo::RepoContext;
use module_release_tool::tempdir::TempDir;
use module_release_tool::{ARTIFACT_MANIFEST_FILE, RELEASE_BUNDLE_FILE, RELEASE_IDENTITY_FILE};

const ZEROED_REVISION: &str = "0000000000000000000000000000000000000000";

fn context() -> RepoContext {
    RepoContext::discover().expect("repository context")
}

fn candidate(module: &str, tag: &str) -> (TempDir, PathBuf) {
    let out = TempDir::new(tag).expect("temp dir");
    let generated = generate(&context(), module, out.path()).expect("generate");
    (out, generated.directory)
}

/// Copies a verified artifact into a fresh directory so a test may damage it.
/// Every negative case gets its own copy: one injected fault per copy, always
/// starting from a candidate that verified (review F5.1).
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

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("parse json")
}

fn write_json(path: &Path, value: &serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(value).expect("serialize")).expect("write");
}

fn payload(artifact: &Path, member: &str) -> PathBuf {
    artifact.join("package").join(member.replace('/', "\\"))
}

fn check_of(error: &module_release_tool::ToolError) -> String {
    let text = error.to_string();
    text.split('[')
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or("unclassified")
        .to_owned()
}

// ------------------------------------------------------------------ F1 -------

#[test]
fn a_pinned_source_revision_that_is_not_a_real_commit_is_refused() {
    let (_out, artifact) = candidate("identity", "f1-zero-revision");
    let (_copy, damaged) = pristine_copy(&artifact, "f1-zero-revision-copy");
    for file in [RELEASE_IDENTITY_FILE, RELEASE_BUNDLE_FILE] {
        let path = damaged.join(file);
        let mut value = read_json(&path);
        let target = if file == RELEASE_BUNDLE_FILE {
            &mut value["identity"]["source_revision"]
        } else {
            &mut value["source_revision"]
        };
        *target = serde_json::Value::String(ZEROED_REVISION.to_owned());
        assert_eq!(
            target.as_str(),
            Some(ZEROED_REVISION),
            "the injection must land before the refusal is asserted"
        );
        write_json(&path, &value);
    }
    let error = verify(&context(), &damaged).expect_err("a zeroed source revision must be refused");
    assert_eq!(check_of(&error), "source-revision-exists", "got: {error}");
}

#[test]
fn the_pinned_revision_must_actually_contain_the_inputs_it_claims() {
    let (_out, artifact) = candidate("identity", "f1-older-commit");
    let (_copy, damaged) = pristine_copy(&artifact, "f1-older-commit-copy");
    // The parent of the commit that introduced this module's release spec is a
    // real commit that cannot have produced this candidate: the spec — a build
    // input of it — is not in that tree at all. Pasting its SHA into the identity
    // records must therefore be refused, even though the shape is a valid
    // revision and the payload bytes are untouched.
    let ancestor = {
        let shown = context()
            .root()
            .join("tools/module-release-tool/specs/identity.json")
            .display()
            .to_string();
        let output = std::process::Command::new("git")
            .args([
                "-C",
                &context().root().display().to_string(),
                "log",
                "--format=%H",
                "--",
                &shown,
            ])
            .output()
            .expect("git log");
        let history = String::from_utf8(output.stdout).expect("utf-8");
        let introducing = history
            .lines()
            .last()
            .expect("the spec has a committing history")
            .trim()
            .to_owned();
        let output = std::process::Command::new("git")
            .args([
                "-C",
                &context().root().display().to_string(),
                "rev-parse",
                &format!("{introducing}^"),
            ])
            .output()
            .expect("git rev-parse");
        String::from_utf8(output.stdout)
            .expect("utf-8")
            .trim()
            .to_owned()
    };
    for file in [RELEASE_IDENTITY_FILE, RELEASE_BUNDLE_FILE] {
        let path = damaged.join(file);
        let mut value = read_json(&path);
        if file == RELEASE_BUNDLE_FILE {
            value["identity"]["source_revision"] = serde_json::Value::String(ancestor.clone());
        } else {
            value["source_revision"] = serde_json::Value::String(ancestor.clone());
        }
        write_json(&path, &value);
    }
    let error = verify(&context(), &damaged)
        .expect_err("an artifact cannot claim a commit its inputs were not read from");
    assert_eq!(
        check_of(&error),
        "source-inputs-match-revision",
        "got: {error}"
    );
    let reason = error.to_string();
    assert!(
        reason.contains("tools/module-release-tool/specs/identity.")
            && reason.contains("does not contain"),
        "the refusal must name the input the commit lacks: {reason}"
    );
}
// ------------------------------------------------------------------ F2 -------

#[test]
fn compare_reads_the_real_payload_of_the_right_side() {
    let (_left_out, left) = candidate("identity", "f2-left");
    let (_right_out, right) = candidate("identity", "f2-right");
    let packed = payload(&right, "contract/packages/identity/src/lib.rs");
    let mut text = fs::read_to_string(&packed).expect("packed lib.rs");
    text.push_str("compile_error!(\"review tamper\");\n");
    fs::write(&packed, text).expect("tamper");
    let error = compare(&left, &right).expect_err("a damaged right payload must not compare equal");
    assert_eq!(
        check_of(&error),
        "compare-payload-integrity",
        "got: {error}"
    );
}

#[test]
fn compare_reads_the_real_payload_of_the_left_side() {
    let (_left_out, left) = candidate("identity", "f2-left-damage");
    let (_right_out, right) = candidate("identity", "f2-right-clean");
    fs::remove_file(payload(&left, "CHANGELOG.md")).expect("remove a member");
    let error = compare(&left, &right).expect_err("a missing left member must not compare equal");
    assert_eq!(
        check_of(&error),
        "compare-payload-integrity",
        "got: {error}"
    );
}

#[test]
fn two_independent_candidates_still_compare_identical() {
    let (_left_out, left) = candidate("policy", "f2-good-left");
    let (_right_out, right) = candidate("policy", "f2-good-right");
    let comparison = compare(&left, &right).expect("two candidates from one commit must compare");
    assert!(
        comparison.identical,
        "unexpected differences: {:?}",
        comparison.differences
    );
}

// ------------------------------------------------------------------ F3 -------

#[test]
fn an_unknown_artifact_record_schema_or_algorithm_is_refused() {
    for (field, value) in [
        ("artifact_integrity_algorithm", "unknown-algorithm.v9"),
        ("schema_version", "plan-0014.wave5a.artifact-manifest.v99"),
    ] {
        let (_out, artifact) = candidate("identity", "f3-schema");
        let (_copy, damaged) = pristine_copy(&artifact, "f3-schema-copy");
        let path = damaged.join(ARTIFACT_MANIFEST_FILE);
        let mut record = read_json(&path);
        record[field] = serde_json::Value::String(value.to_owned());
        assert_eq!(record[field].as_str(), Some(value));
        write_json(&path, &record);
        let error =
            verify(&context(), &damaged).expect_err("an unknown record format must be refused");
        assert_eq!(check_of(&error), "artifact-record-format", "got: {error}");
    }
}

#[test]
fn the_descriptive_records_must_describe_one_module_and_version() {
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("module_id", serde_json::Value::String("policy".to_owned())),
        (
            "module_version",
            serde_json::Value::String("9.9.9".to_owned()),
        ),
        ("contract_digest", serde_json::Value::String("f".repeat(64))),
        ("package_digest", serde_json::Value::String("e".repeat(64))),
    ];
    for (field, replacement) in cases {
        let (_out, artifact) = candidate("identity", "f3-records");
        let (_copy, damaged) = pristine_copy(&artifact, "f3-records-copy");
        let path = damaged.join(ARTIFACT_MANIFEST_FILE);
        let mut record = read_json(&path);
        let expected = replacement.as_str().expect("string replacement").to_owned();
        record[field] = replacement;
        assert_eq!(record[field].as_str(), Some(expected.as_str()));
        write_json(&path, &record);
        let error = verify(&context(), &damaged)
            .expect_err("artifact-manifest.json may not describe a different release");
        assert_eq!(check_of(&error), "record-consistency", "got: {error}");
    }
}

#[test]
fn the_bundle_crate_bindings_must_match_the_packed_cargo_manifests() {
    let cases: Vec<(&str, serde_json::Value)> = vec![
        (
            "crate_version",
            serde_json::Value::String("9.9.9".to_owned()),
        ),
        (
            "crate_name",
            serde_json::Value::String("identity-renamed".to_owned()),
        ),
    ];
    for (field, replacement) in cases {
        let (_out, artifact) = candidate("identity", "f3-bindings");
        let (_copy, damaged) = pristine_copy(&artifact, "f3-bindings-copy");
        let path = damaged.join(RELEASE_BUNDLE_FILE);
        let mut record = read_json(&path);
        let expected = replacement.as_str().expect("string replacement").to_owned();
        record["crate_versions"][0][field] = replacement;
        assert_eq!(
            record["crate_versions"][0][field].as_str(),
            Some(expected.as_str())
        );
        write_json(&path, &record);
        let error = verify(&context(), &damaged)
            .expect_err("crate bindings must be checked against the packed manifests");
        assert_eq!(check_of(&error), "packed-crate-bindings", "got: {error}");
    }
}
