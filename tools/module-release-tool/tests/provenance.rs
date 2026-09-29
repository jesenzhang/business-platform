#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Provenance binding, exercised against real Git objects.
//!
//! Review F1 requires the pinned revision to mean "the commit Git also means",
//! and the packed inputs to be proved against *that* commit. These tests build
//! throwaway repositories with `git init` and assert the refusals the previous
//! hand-written `.git` reader could not express — including the two checkout
//! shapes (detached `HEAD`, linked worktree) where that reader and Git disagree.

use std::path::{Path, PathBuf};
use std::process::Command;

use module_release_tool::git::Git;
use module_release_tool::provenance::{
    Provenance, ProvenanceInput, CHECK_INPUTS, CHECK_REVISION_EXISTS,
};
use module_release_tool::tempdir::TempDir;
use module_release_tool::ToolError;

fn git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .arg("-c")
        .arg("init.defaultBranch=main")
        .arg("-c")
        .arg("user.email=provenance-test@example.invalid")
        .arg("-c")
        .arg("user.name=provenance test")
        .args(arguments)
        .output()
        .expect("git must be runnable for this test");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn repository(tag: &str) -> (TempDir, PathBuf) {
    let out = TempDir::new(tag).expect("temp dir");
    let root = out.path().join("repo");
    std::fs::create_dir_all(root.join("src")).expect("create repo");
    git(&root, &["init", "--quiet", "."]);
    std::fs::write(root.join("src/lib.rs"), "pub fn shipped() {}\n").expect("write");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"shipped\"\n").expect("write");
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "first"]);
    (out, root)
}

fn check_of(error: &ToolError) -> String {
    let text = error.to_string();
    text.split('[')
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or("unclassified")
        .to_owned()
}

fn input(root: &Path, repo_path: &str) -> ProvenanceInput {
    ProvenanceInput::from_working_tree(
        repo_path.to_owned(),
        std::fs::read(root.join(repo_path.replace('/', "\\"))).expect("read input"),
    )
}

#[test]
fn a_pinned_revision_must_be_a_real_commit_object() {
    let (_out, root) = repository("prov-no-commit");
    let error = Provenance::open(&root, "0000000000000000000000000000000000000000")
        .expect_err("forty zeroes is a shape, not a commit");
    assert_eq!(check_of(&error), CHECK_REVISION_EXISTS, "got: {error}");
    assert!(error.to_string().contains("no such commit"), "got: {error}");
}

#[test]
fn an_input_modified_but_never_committed_is_refused() {
    let (_out, root) = repository("prov-dirty");
    let head = git(&root, &["rev-parse", "HEAD"]);
    // The working tree moves ahead of the commit without committing: exactly the
    // state in which generating a candidate that pins `head` would be a lie.
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn shipped() { uncommitted() }\n",
    )
    .expect("write");
    let provenance = Provenance::open(&root, &head).expect("open");
    let error = provenance
        .compare_sources(&[input(&root, "src/lib.rs")])
        .expect_err("an uncommitted modification must not be presented as committed content");
    assert_eq!(check_of(&error), CHECK_INPUTS, "got: {error}");
    let reason = error.to_string();
    assert!(
        reason.contains("src/lib.rs"),
        "must name the file: {reason}"
    );
    assert!(
        reason.contains("committed"),
        "must say what differs: {reason}"
    );
}

#[test]
fn an_input_the_pinned_commit_does_not_contain_is_refused() {
    let (_out, root) = repository("prov-missing");
    let head = git(&root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("src/added.rs"), "// never committed\n").expect("write");
    let provenance = Provenance::open(&root, &head).expect("open");
    let error = provenance
        .compare_sources(&[input(&root, "src/added.rs")])
        .expect_err("a file the commit does not have cannot back a claim about it");
    assert_eq!(check_of(&error), CHECK_INPUTS, "got: {error}");
    assert!(
        error.to_string().contains("does not contain"),
        "got: {error}"
    );
}

#[test]
fn committed_inputs_match_and_the_check_reports_what_it_compared() {
    let (_out, root) = repository("prov-clean");
    let head = git(&root, &["rev-parse", "HEAD"]);
    let provenance = Provenance::open(&root, &head).expect("open");
    let compared = provenance
        .compare_sources(&[input(&root, "src/lib.rs"), input(&root, "Cargo.toml")])
        .expect("committed inputs must match their blobs");
    assert_eq!(compared, 2);
}

#[test]
fn a_detached_head_and_a_linked_worktree_mean_the_same_commit_as_git() {
    let (_out, root) = repository("prov-shapes");
    let scratch = TempDir::new("prov-shapes-worktree").expect("worktree temp dir");
    let commit = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["checkout", "--quiet", "--detach"]);
    assert_eq!(
        Git::open(&root).expect("open").head().expect("head"),
        commit,
        "a detached HEAD must resolve to the commit git reports"
    );

    let worktree = scratch.path().join("worktree");
    git(
        &root,
        &[
            "worktree",
            "add",
            "--quiet",
            &worktree.display().to_string(),
            &commit,
            "--detach",
        ],
    );
    assert_eq!(
        Git::open(&worktree)
            .expect("a linked worktree must be readable")
            .head()
            .expect("head"),
        commit,
        "a linked worktree resolves through gitdir:, which the hand-written reader got wrong"
    );
    git(
        &root,
        &["worktree", "remove", &worktree.display().to_string()],
    );
}

#[test]
fn a_blob_is_committed_content_not_working_tree_content() {
    let (_out, root) = repository("prov-blob");
    let commit = git(&root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("src/lib.rs"), "tampered\n").expect("write");
    let git = Git::open(&root).expect("open");
    let blob = git
        .blob(&commit, "src/lib.rs")
        .expect("blob")
        .expect("the path is in the commit");
    assert_eq!(
        String::from_utf8(blob).expect("utf-8"),
        "pub fn shipped() {}\n",
        "the committed bytes must be returned even while the working tree differs"
    );
    assert!(
        git.blob(&commit, "src/nope.rs").expect("blob").is_none(),
        "an absent path must be answered as absent, not as an error"
    );
}
