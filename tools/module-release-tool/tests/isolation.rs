#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Consumer isolation, judged on a resolved `cargo metadata` graph.
//!
//! These are the shapes review F4 listed as previously invisible to the string
//! scan: a forward-slash absolute path, a relative path, and a leak that only
//! exists *transitively* through the artifact crate. Each one must be refused,
//! and the refusal must name the package and the path. The rule under test is the
//! same helper the orchestration script calls through `module-release guard`,
//! not a second copy of it.

use std::path::{Path, PathBuf};

use module_release_tool::isolation::{check_metadata, IsolationPolicy, CHECK_ISOLATION};
use module_release_tool::tempdir::TempDir;
use module_release_tool::ToolError;
use serde_json::{json, Value};

/// One package entry as `cargo metadata --format-version 1` reports it.
struct Package<'a> {
    name: &'a str,
    version: &'a str,
    /// `None` is cargo's spelling of a local path package.
    source: Option<&'a str>,
    manifest_path: PathBuf,
}

fn registry(name: &'static str, version: &'static str) -> Package<'static> {
    Package {
        name,
        version,
        source: Some("registry+https://github.com/rust-lang/crates.io-index"),
        manifest_path: PathBuf::new(),
    }
}

fn path_package(name: &'static str, manifest_path: PathBuf) -> Package<'static> {
    Package {
        name,
        version: "0.1.0-rc.1",
        source: None,
        manifest_path,
    }
}

/// Writes the files a graph refers to, so isolation is decided on paths that
/// really exist — a path that cannot be resolved is refused, never assumed safe.
fn fixture(tag: &str) -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let out = TempDir::new(tag).expect("temp dir");
    let root = out.path().to_path_buf();
    let repo = root.join("repo");
    let consumer = root.join("consumer");
    let artifact = root.join("out").join("identity-0.1.0-rc.1").join("package");
    for directory in [
        repo.join("crates/identity/src"),
        repo.join("crates/business-module-contracts/src"),
        consumer.join("src"),
        artifact.join("contract/packages/identity"),
    ] {
        std::fs::create_dir_all(&directory).expect("create fixture directory");
    }
    for file in [
        repo.join("Cargo.toml"),
        repo.join("crates/identity/Cargo.toml"),
        repo.join("crates/business-module-contracts/Cargo.toml"),
        consumer.join("Cargo.toml"),
        artifact.join("contract/packages/identity/Cargo.toml"),
    ] {
        std::fs::write(&file, "[package]\n").expect("write fixture manifest");
    }
    (out, repo, consumer, artifact)
}

fn metadata(workspace_root: &Path, packages: &[Package<'_>]) -> String {
    let entries: Vec<Value> = packages
        .iter()
        .map(|package| {
            json!({
                "name": package.name,
                "version": package.version,
                "id": format!("{}:{}", package.name, package.version),
                "license": "MIT",
                "source": package.source,
                "manifest_path": package.manifest_path.display().to_string(),
                "dependencies": [],
                "features": {},
            })
        })
        .collect();
    json!({
        "packages": entries,
        "workspace_members": [],
        "workspace_default_members": [],
        "resolve": null,
        "target_directory": workspace_root.join("target").display().to_string(),
        "version": 1,
        "workspace_root": workspace_root.display().to_string(),
    })
    .to_string()
}

fn policy(consumer: &Path, artifact: &Path, repo: &Path) -> IsolationPolicy {
    IsolationPolicy {
        consumer_root: consumer.to_path_buf(),
        allowed_roots: vec![artifact.join("contract/packages/identity")],
        forbidden_roots: vec![repo.to_path_buf()],
    }
}

fn check_of(error: &ToolError) -> String {
    let text = error.to_string();
    text.split('[')
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or("unclassified")
        .to_owned()
}

#[test]
fn a_consumer_that_resolves_only_into_itself_and_the_artifact_passes() {
    let (_out, repo, consumer, artifact) = fixture("iso-clean");
    let document = metadata(
        &consumer,
        &[
            path_package("wave5a-consumer", consumer.join("Cargo.toml")),
            path_package(
                "identity",
                artifact.join("contract/packages/identity/Cargo.toml"),
            ),
            registry("chrono", "0.4.45"),
            registry("uuid", "1.26.0"),
            registry("tokio", "1.53.1"),
        ],
    );
    let report = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect("an isolated consumer must pass");
    assert_eq!(
        report.local_packages.len(),
        2,
        "{:?}",
        report.local_packages
    );
    assert_eq!(report.registry_packages, 3);
    assert!(
        !report.workspace_root.contains("\\\\?\\"),
        "the verbatim prefix is noise in a report: {}",
        report.workspace_root
    );
    for package in &report.local_packages {
        assert!(
            package.verdict == "consumer" || package.verdict.starts_with("allowed:"),
            "unexpected verdict: {package:?}"
        );
    }
}

#[test]
fn a_path_dependency_into_the_source_repository_is_refused() {
    // The reviewer's F4 injection, spelled with forward slashes: a leaked
    // dependency on the repository's own crate.
    let (_out, repo, consumer, artifact) = fixture("iso-absolute");
    let document = metadata(
        &consumer,
        &[
            path_package("wave5a-consumer", consumer.join("Cargo.toml")),
            path_package(
                "identity",
                artifact.join("contract/packages/identity/Cargo.toml"),
            ),
            path_package("identity", repo.join("crates/identity/Cargo.toml")),
        ],
    );
    let error = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect_err("a local package inside the source repository is a leak");
    assert_eq!(check_of(&error), CHECK_ISOLATION, "got: {error}");
    let reason = error.to_string();
    assert!(reason.contains("identity 0.1.0-rc.1"), "got: {reason}");
    assert!(
        reason.contains(&Path::new("crates").join("identity").display().to_string())
            || reason.contains("crates/identity"),
        "the refusal must show where it resolved: {reason}"
    );
    assert!(reason.contains("FORBIDDEN"), "got: {reason}");
}

#[test]
fn a_relative_path_escaping_into_the_repository_is_refused() {
    // `path = "../repo/crates/identity"` is reported by cargo with the `..` already
    // resolved away; the rule has to follow the resolution, not pattern-match a string.
    let (_out, repo, consumer, artifact) = fixture("iso-relative");
    let escaping = consumer.join("../repo/crates/identity/Cargo.toml");
    let document = metadata(
        &consumer,
        &[
            path_package("wave5a-consumer", consumer.join("Cargo.toml")),
            path_package(
                "identity",
                artifact.join("contract/packages/identity/Cargo.toml"),
            ),
            path_package("identity", escaping),
        ],
    );
    let error = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect_err("a relative path is not a reason to miss the leak");
    assert_eq!(check_of(&error), CHECK_ISOLATION, "got: {error}");
    let reason = error.to_string();
    assert!(reason.contains("FORBIDDEN"), "got: {reason}");
}

#[test]
fn a_transitive_path_dependency_into_the_repository_is_refused() {
    // The consumer names only the artifact crate; the artifact crate reaches a
    // repository crate. Nothing in the consumer's own Cargo.toml mentions it, so
    // only the resolved graph can see this — which is why F4 asked for metadata.
    let (_out, repo, consumer, artifact) = fixture("iso-transitive");
    let document = metadata(
        &consumer,
        &[
            path_package("wave5a-consumer", consumer.join("Cargo.toml")),
            path_package(
                "identity",
                artifact.join("contract/packages/identity/Cargo.toml"),
            ),
            path_package(
                "business-module-contracts",
                repo.join("crates/business-module-contracts/Cargo.toml"),
            ),
            registry("chrono", "0.4.45"),
        ],
    );
    let error = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect_err("a transitive local dependency is still a dependency");
    assert_eq!(check_of(&error), CHECK_ISOLATION, "got: {error}");
    let reason = error.to_string();
    assert!(
        reason.contains("business-module-contracts 0.1.0-rc.1"),
        "got: {reason}"
    );
}

#[test]
fn a_local_package_outside_every_declared_root_is_refused() {
    let (out, repo, consumer, artifact) = fixture("iso-elsewhere");
    let elsewhere = out.path().join("elsewhere/crate");
    std::fs::create_dir_all(&elsewhere).expect("create directory");
    std::fs::write(elsewhere.join("Cargo.toml"), "[package]\n").expect("write");
    let document = metadata(
        &consumer,
        &[
            path_package("wave5a-consumer", consumer.join("Cargo.toml")),
            path_package("surprise", elsewhere.join("Cargo.toml")),
        ],
    );
    let error = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect_err("an undeclared local root is not automatically allowed");
    assert_eq!(check_of(&error), CHECK_ISOLATION, "got: {error}");
    let reason = error.to_string();
    assert!(reason.contains("unauthorized"), "got: {reason}");
}

#[test]
fn containment_is_decided_on_a_path_boundary_not_a_string_prefix() {
    // `out/identity-0.1.0-rc.1-evil` shares a string prefix with the allowed
    // directory and must still be refused.
    let (out, repo, consumer, artifact) = fixture("iso-boundary");
    let evil = out
        .path()
        .join("out/identity-0.1.0-rc.1-evil/package/contract/packages/identity");
    std::fs::create_dir_all(&evil).expect("create directory");
    std::fs::write(evil.join("Cargo.toml"), "[package]\n").expect("write");
    let document = metadata(
        &consumer,
        &[
            path_package("wave5a-consumer", consumer.join("Cargo.toml")),
            path_package("identity", evil.join("Cargo.toml")),
        ],
    );
    let error = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect_err("a sibling directory with a similar name is not inside the allowed root");
    assert_eq!(check_of(&error), CHECK_ISOLATION, "got: {error}");
}

#[test]
fn a_path_that_cannot_be_resolved_is_refused_rather_than_assumed_safe() {
    let (_out, repo, consumer, artifact) = fixture("iso-unresolvable");
    let document = metadata(
        &consumer,
        &[
            path_package("wave5a-consumer", consumer.join("Cargo.toml")),
            path_package(
                "identity",
                artifact.join("contract/packages/ghost/Cargo.toml"),
            ),
        ],
    );
    let error = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect_err("an unresolvable manifest path cannot be judged isolated");
    assert_eq!(check_of(&error), CHECK_ISOLATION, "got: {error}");
    let reason = error.to_string();
    assert!(reason.contains("cannot resolve"), "got: {reason}");
}

#[test]
fn a_consumer_building_inside_another_workspace_is_refused() {
    let (_out, repo, consumer, artifact) = fixture("iso-workspace");
    let document = metadata(
        &repo,
        &[
            path_package("identity", repo.join("crates/identity/Cargo.toml")),
            path_package(
                "identity",
                artifact.join("contract/packages/identity/Cargo.toml"),
            ),
        ],
    );
    let error = check_metadata(&document, &policy(&consumer, &artifact, &repo))
        .expect_err("a resolved workspace root outside the consumer is a leak of context");
    assert_eq!(check_of(&error), CHECK_ISOLATION, "got: {error}");
    let reason = error.to_string();
    assert!(reason.contains("workspace root"), "got: {reason}");
}
