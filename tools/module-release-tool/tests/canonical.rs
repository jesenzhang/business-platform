#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Normalisation and inventory rules from the Wave 5A record (§4.4): the
//! payload digest must not depend on host paths, directory-walk order or the
//! line-ending convention of the checkout that produced the bytes.
//!
//! Cases: A-2 (inventory ordering), A-3 (line-ending normalisation).

use std::fs;
use std::path::Path;

use module_release_tool::canonical::{
    artifact_integrity_digest, inventory, normalize_text_bytes, sha256_hex, validate_member_path,
};
use module_release_tool::tempdir::TempDir;

fn write(root: &Path, member: &str, bytes: &[u8]) {
    let path = root.join(member.replace('/', "\\"));
    fs::create_dir_all(path.parent().expect("a member has a parent")).expect("create parent");
    fs::write(path, bytes).expect("write member");
}

fn paths_of(root: &Path) -> Vec<String> {
    inventory(root)
        .expect("inventory")
        .into_iter()
        .map(|file| file.path)
        .collect()
}

#[test]
fn inventory_is_ordered_by_path_bytes_and_not_by_walk_order() {
    let dir = TempDir::new("inventory-order").expect("temp dir");
    // Deliberately created in an order that is neither alphabetical nor shallow
    // first, so a walk-order-dependent implementation cannot pass. `Zed.md`
    // comes first because the order is by path *bytes*: an uppercase byte sorts
    // before a lowercase one and nothing is case-folded. (The names below have
    // no case twins on purpose — a case-insensitive host filesystem would
    // collapse `b.txt` and `B.txt` onto one file, which is a property of the
    // host, not of the digest.)
    for (member, body) in [
        ("sub/x/y.rs", "y"),
        ("b.txt", "b"),
        ("sub/c.txt", "c"),
        ("a.txt", "a"),
        ("Zed.md", "Z"),
        ("sub/a.txt", "a"),
    ] {
        write(dir.path(), member, body.as_bytes());
    }
    let observed = paths_of(dir.path());
    assert_eq!(
        observed,
        vec![
            "Zed.md",
            "a.txt",
            "b.txt",
            "sub/a.txt",
            "sub/c.txt",
            "sub/x/y.rs",
        ]
    );
    assert_eq!(paths_of(dir.path()), observed, "a re-read must be stable");
}

#[test]
fn digest_ignores_line_endings_and_a_missing_trailing_newline() {
    let lf = TempDir::new("lf").expect("temp dir");
    let crlf = TempDir::new("crlf").expect("temp dir");
    write(lf.path(), "a.md", b"# title\nline\n");
    write(lf.path(), "sub/b.rs", b"fn main() {}\n");
    write(crlf.path(), "a.md", b"# title\r\nline");
    write(crlf.path(), "sub/b.rs", b"fn main() {}\r\n");
    assert_eq!(
        artifact_integrity_digest(&inventory(lf.path()).expect("lf inventory")),
        artifact_integrity_digest(&inventory(crlf.path()).expect("crlf inventory")),
        "CRLF vs LF and a trailing newline must not move the digest"
    );
}

#[test]
fn digest_reacts_to_one_content_byte_and_to_a_path_change() {
    let left = TempDir::new("content-a").expect("temp dir");
    let right = TempDir::new("content-b").expect("temp dir");
    let other = TempDir::new("content-c").expect("temp dir");
    write(left.path(), "a.md", b"same\n");
    write(right.path(), "a.md", b"samf\n");
    write(other.path(), "b.md", b"same\n");
    let left_digest = artifact_integrity_digest(&inventory(left.path()).expect("left"));
    assert_ne!(
        left_digest,
        artifact_integrity_digest(&inventory(right.path()).expect("right"))
    );
    assert_ne!(
        left_digest,
        artifact_integrity_digest(&inventory(other.path()).expect("other")),
        "the member path is part of the digest, so a rename must move it"
    );
}

#[test]
fn normalisation_rejects_binary_content_and_strips_a_byte_order_mark() {
    assert!(normalize_text_bytes(&[0xff_u8, 0xfe, 0x00, 0x00], "binary.bin").is_err());
    assert_eq!(
        normalize_text_bytes("\u{feff}a".as_bytes(), "text.md").expect("utf-8"),
        b"a\n"
    );
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn member_paths_must_be_relative_posix_paths_without_dot_components() {
    for bad in [
        "../escape.md",
        "./here.md",
        "a/../b.md",
        "a/./b.md",
        "dir//file.md",
        "dir/",
        "C:/absolute.md",
        "back\\slash.md",
        "/absolute.md",
        "",
        "evil\nname.md",
    ] {
        assert!(
            validate_member_path(bad).is_err(),
            "'{bad}' must not be an acceptable artifact member path"
        );
    }
    assert!(validate_member_path("contract/packages/identity/src/lib.rs").is_ok());
    assert!(validate_member_path(".hidden-but-plain.md").is_ok());
}
