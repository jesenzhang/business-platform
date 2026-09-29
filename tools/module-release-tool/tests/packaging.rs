#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The assembly rules of §4.2–§4.5, checked against real candidates.
//!
//! Cases: A-1 (two temporary directories produce byte-identical candidates),
//! A-4 (no host path, host identity or generation timestamp leaks into the
//! payload), A-5 (files outside the integrity digest are declared, not
//! silently ignored).

use std::fs;
use std::path::Path;

use module_release_tool::canonical::inventory;
use module_release_tool::packaging::{compare, generate};
use module_release_tool::repo::RepoContext;
use module_release_tool::tempdir::TempDir;
use module_release_tool::ARTIFACT_MANIFEST_FILE;

/// Everything that must not appear inside the hashed payload.
fn host_needles(context: &RepoContext, out_root: &Path) -> Vec<String> {
    let mut needles = Vec::new();
    for path in [
        context.root().display().to_string(),
        out_root.display().to_string(),
    ] {
        needles.push(path.clone());
        needles.push(path.replace('\\', "/"));
    }
    for variable in ["COMPUTERNAME", "USERDOMAIN", "HOMEPATH"] {
        if let Ok(value) = std::env::var(variable) {
            if value.len() > 3 {
                needles.push(value);
            }
        }
    }
    needles
}

/// ISO-8601 date-time (`YYYY-MM-DD` followed by a time) or a bare clock time
/// (`HH:MM:SS`), i.e. a generation timestamp. Document names inside authority
/// strings carry plain dates on purpose; a timestamp would make the payload
/// unreproducible, so only date-times and clock times are rejected.
fn contains_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digit = |byte: u8| byte.is_ascii_digit();
    let clock = bytes.windows(8).any(|window| {
        digit(window[0])
            && digit(window[1])
            && window[2] == b':'
            && digit(window[3])
            && digit(window[4])
            && window[5] == b':'
            && digit(window[6])
            && digit(window[7])
    });
    let date_time = bytes.windows(13).any(|window| {
        digit(window[0])
            && digit(window[1])
            && digit(window[2])
            && digit(window[3])
            && window[4] == b'-'
            && digit(window[5])
            && digit(window[6])
            && window[7] == b'-'
            && digit(window[8])
            && digit(window[9])
            && (window[10] == b'T' || window[10] == b' ')
            && digit(window[11])
            && digit(window[12])
    });
    clock || date_time
}
/// A drive-letter path such as `F:\...` or `f:/...`.
fn contains_absolute_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(4).any(|window| {
        window[0].is_ascii_alphabetic()
            && window[1] == b':'
            && (window[2] == b'\\' || window[2] == b'/')
            && window[3].is_ascii_alphanumeric()
    })
}

#[test]
fn regenerating_into_two_directories_yields_identical_candidates() {
    let context = RepoContext::discover().expect("repository context");
    for module in ["identity", "policy"] {
        let left = TempDir::new("regen-left").expect("temp dir");
        let right = TempDir::new("regen-right").expect("temp dir");
        let first = generate(&context, module, left.path()).expect("generate left");
        let second = generate(&context, module, right.path()).expect("generate right");
        assert_ne!(
            first.directory, second.directory,
            "the digests must not be derived from where the artifact was written"
        );
        assert_eq!(first.contract_digest, second.contract_digest);
        assert_eq!(first.package_digest, second.package_digest);
        assert_eq!(
            first.artifact_integrity_digest,
            second.artifact_integrity_digest
        );
        let comparison = compare(&first.directory, &second.directory).expect("compare");
        assert!(
            comparison.identical,
            "{module} differs between two generations: {:?}",
            comparison.differences
        );
    }
}

#[test]
fn payload_carries_no_host_path_host_identity_or_generation_timestamp() {
    let context = RepoContext::discover().expect("repository context");
    let out = TempDir::new("leak-check").expect("temp dir");
    let artifact = generate(&context, "identity", out.path()).expect("generate");
    let needles = host_needles(&context, out.path());
    let payload = artifact.directory.join("package");
    let files = inventory(&payload).expect("payload inventory");
    assert!(
        files.len() > 15,
        "expected a real payload, found {}",
        files.len()
    );
    for file in &files {
        let shown = payload.join(file.path.replace('/', "\\"));
        let raw = fs::read(&shown).expect("read member");
        assert!(
            !raw.contains(&b'\r'),
            "member '{}' contains a CR byte",
            file.path
        );
        assert!(!file.path.contains(':'), "member path '{}'", file.path);
        assert!(!file.path.contains('\\'), "member path '{}'", file.path);
        let text = String::from_utf8(raw).expect("every payload member is UTF-8 text");
        for needle in &needles {
            assert!(
                !text.contains(needle.as_str()),
                "member '{}' leaks host value {needle:?}",
                file.path
            );
        }
        assert!(
            !contains_timestamp(&text),
            "member '{}' carries a generation timestamp",
            file.path
        );
        assert!(
            !contains_absolute_path(&text),
            "member '{}' carries an absolute host path",
            file.path
        );
    }
}

#[test]
fn members_outside_the_digest_are_declared_not_silently_ignored() {
    let context = RepoContext::discover().expect("repository context");
    let out = TempDir::new("exclusions").expect("temp dir");
    let artifact = generate(&context, "policy", out.path()).expect("generate");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(artifact.directory.join(ARTIFACT_MANIFEST_FILE)).expect("manifest"),
    )
    .expect("manifest json");

    let excluded: Vec<&str> = manifest["excluded_members"]
        .as_array()
        .expect("excluded_members")
        .iter()
        .map(|entry| entry["path"].as_str().expect("path"))
        .collect();
    for expected in [
        "artifact-manifest.json",
        "release.json",
        "release-bundle.json",
        "verification.json",
    ] {
        assert!(
            excluded.contains(&expected),
            "'{expected}' is outside the payload digest but is not declared"
        );
    }
    for entry in manifest["excluded_members"].as_array().expect("entries") {
        let reason = entry["reason"].as_str().expect("reason");
        assert!(
            !reason.trim().is_empty(),
            "an exclusion has no reason: {entry}"
        );
    }

    let hashed: Vec<&str> = manifest["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|entry| entry["path"].as_str().expect("path"))
        .collect();
    for path in &excluded {
        assert!(
            !hashed.contains(path),
            "excluded member '{path}' is also hashed"
        );
        assert!(
            artifact.directory.join(path.replace('/', "\\")).is_file(),
            "declared member '{path}' is missing from the artifact"
        );
    }

    let on_disk: Vec<String> = inventory(&artifact.directory.join("package"))
        .expect("payload inventory")
        .into_iter()
        .map(|file| file.path)
        .collect();
    let hashed_owned: Vec<String> = hashed.iter().map(|path| (*path).to_owned()).collect();
    assert_eq!(
        on_disk, hashed_owned,
        "the payload on disk must be exactly the hashed member list"
    );
}
