//! Deterministic byte canonicalisation and the digests built on it.
//!
//! The rules implemented here are defined in
//! `docs/reviews/2026-09-29-wave5a-implementation-record.md` §4.4 and exist
//! because the repository had no rule for *artifact bytes* (its determinism
//! statements cover canonical *declarations*, see Wave 2 review §5). Each rule
//! prefix is part of the hashed content, so changing a rule changes every
//! digest and is caught by the repeat-build tests.
//!
//! Rules, in short:
//!
//! 1. members are regular files only; symbolic links and special entries are
//!    refused, never skipped;
//! 2. paths are repository-relative POSIX (`/`), never absolute, never `.` or
//!    `..`;
//! 3. ordering is UTF-8 byte order of the relative path — independent of locale,
//!    directory-walk order and file system;
//! 4. text is normalised before hashing: valid UTF-8 required, BOM removed,
//!    `\r\n` and lone `\r` folded to `\n`, one trailing `\n` guaranteed;
//! 5. one entry is `"<sha256> <path>\n"`, entries are concatenated in order and
//!    the algorithm identifier is prepended before the final SHA-256;
//! 6. no timestamp, host name, user name, output directory or random identifier
//!    may appear inside hashed content.

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use crate::{ToolError, ToolResult};

/// Algorithm identifier prepended to the artifact integrity pre-image.
pub const ARTIFACT_INTEGRITY_ALGORITHM: &str = "plan-0014.artifact-integrity.v1";
/// Algorithm identifier prepended to the migration-set pre-image.
pub const MIGRATION_SET_ALGORITHM: &str = "plan-0014.migration-set.v1";

/// Lower-case SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Normalises a text member to its canonical hashed form.
///
/// Rejects invalid UTF-8 instead of guessing an encoding, so a binary member
/// can never be silently packed.
pub fn normalize_text_bytes(raw: &[u8], path: &str) -> ToolResult<Vec<u8>> {
    let text = std::str::from_utf8(raw).map_err(|_| ToolError::NotUtf8 {
        path: path.to_owned(),
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut normalized = String::with_capacity(text.len());
    let mut previous_was_cr = false;
    for character in text.chars() {
        match character {
            '\r' => {
                normalized.push('\n');
                previous_was_cr = true;
            }
            '\n' if previous_was_cr => previous_was_cr = false,
            other => {
                normalized.push(other);
                previous_was_cr = false;
            }
        }
    }
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    Ok(normalized.into_bytes())
}

/// Applies the same normalisation the repository migration manifest check
/// applies, used when a recorded checksum was produced with the other line
/// ending convention.
#[must_use]
pub fn fold_line_endings(raw: &[u8]) -> Vec<u8> {
    let mut folded = Vec::with_capacity(raw.len());
    let mut previous_was_cr = false;
    for &byte in raw {
        match byte {
            b'\r' => {
                folded.push(b'\n');
                previous_was_cr = true;
            }
            b'\n' if previous_was_cr => previous_was_cr = false,
            other => {
                folded.push(other);
                previous_was_cr = false;
            }
        }
    }
    folded
}

/// One packed artifact member.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFile {
    /// Repository-relative POSIX path inside the payload.
    pub path: String,
    /// SHA-256 of the canonicalised bytes.
    pub sha256: String,
    /// Length of the canonicalised bytes.
    pub bytes: usize,
}

/// Validates an inventory path (rule 2).
///
/// A member path must be a relative POSIX path made of plain name components.
/// `a/../b.md`, `a/./b.md`, `a//b.md` and a trailing separator all spell a path
/// that is not the path they resolve to, and `.` / `..` / empty components let
/// two members collide onto one file — so every component is checked, not just
/// the prefix.
pub fn validate_member_path(path: &str) -> ToolResult<()> {
    let invalid = || ToolError::InvalidMemberPath {
        path: path.to_owned(),
    };
    if path.is_empty() || path.contains('\\') || path.contains(':') || path.starts_with('/') {
        return Err(invalid());
    }
    for component in path.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with('.')
            || component.chars().any(char::is_control)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Walks `root` and returns every member in canonical order (rules 1–4).
///
/// The walk order is deliberately irrelevant: the result is sorted by path
/// bytes before it is returned.
pub fn inventory(root: &Path) -> ToolResult<Vec<ArtifactFile>> {
    let mut files = Vec::new();
    collect_members(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    for pair in files.windows(2) {
        if pair[0].path == pair[1].path {
            return Err(ToolError::DuplicateMember {
                path: pair[0].path.clone(),
            });
        }
    }
    Ok(files)
}

fn collect_members(root: &Path, directory: &Path, out: &mut Vec<ArtifactFile>) -> ToolResult<()> {
    let read = std::fs::read_dir(directory).map_err(|source| ToolError::Io {
        path: directory.display().to_string(),
        source,
    })?;
    let mut entries: Vec<PathBuf> = read
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .collect();
    // Sort only so that the *first* reported failure is deterministic; the
    // digest itself depends on the sorted member list, never on this order.
    entries.sort();
    for path in entries {
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let shown = path.display().to_string();
        if metadata.file_type().is_symlink() {
            return Err(ToolError::UnsupportedEntry { path: shown });
        }
        if metadata.is_dir() {
            collect_members(root, &path, out)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(ToolError::UnsupportedEntry { path: shown });
        }
        let raw = std::fs::read(&path).map_err(|source| ToolError::Io {
            path: shown.clone(),
            source,
        })?;
        let relative = relative_member_path(root, &path)?;
        let normalized = normalize_text_bytes(&raw, &relative)?;
        out.push(ArtifactFile {
            path: relative,
            sha256: sha256_hex(&normalized),
            bytes: normalized.len(),
        });
    }
    Ok(())
}

fn relative_member_path(root: &Path, path: &Path) -> ToolResult<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ToolError::InvalidMemberPath {
            path: path.display().to_string(),
        })?;
    let mut segments = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => segments.push(segment.to_string_lossy().into_owned()),
            _ => {
                return Err(ToolError::InvalidMemberPath {
                    path: path.display().to_string(),
                })
            }
        }
    }
    let joined = segments.join("/");
    validate_member_path(&joined)?;
    Ok(joined)
}

/// SHA-256 over the canonical inventory pre-image (rules 5–6).
#[must_use]
pub fn artifact_integrity_digest(files: &[ArtifactFile]) -> String {
    let mut preimage = String::new();
    preimage.push_str(ARTIFACT_INTEGRITY_ALGORITHM);
    preimage.push('\n');
    for file in files {
        let _ = writeln!(preimage, "{} {}", file.sha256, file.path);
    }
    sha256_hex(preimage.as_bytes())
}

/// One entry of the canonical migration-set manifest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MigrationSetEntry {
    /// Backend the file belongs to (`sqlite`, `postgres`).
    pub backend: String,
    /// Migration file name.
    pub file_name: String,
    /// Recorded SHA-256 of the file.
    pub sha256: String,
}

/// SHA-256 over the canonical migration-set manifest for one namespace.
#[must_use]
pub fn migration_set_digest(namespace: &str, entries: &[MigrationSetEntry]) -> String {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|left, right| {
        (left.backend.as_bytes(), left.file_name.as_bytes())
            .cmp(&(right.backend.as_bytes(), right.file_name.as_bytes()))
    });
    let mut preimage = String::new();
    preimage.push_str(MIGRATION_SET_ALGORITHM);
    preimage.push('\n');
    let _ = writeln!(preimage, "namespace={namespace}");
    for entry in sorted {
        let _ = writeln!(
            preimage,
            "{} {} {}",
            entry.backend, entry.file_name, entry.sha256
        );
    }
    sha256_hex(preimage.as_bytes())
}

/// Rejects a string that is not 64 lower-case hexadecimal characters.
pub fn validate_sha256_hex(value: &str, what: &str) -> ToolResult<()> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(ToolError::Verification {
            check: what.to_owned(),
            reason: format!("'{value}' is not 64 lower-case hexadecimal characters"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn crlf_and_lone_cr_fold_to_single_lf() {
        assert_eq!(
            normalize_text_bytes(b"a\r\nb\r\nc", "x").unwrap(),
            b"a\nb\nc\n"
        );
        assert_eq!(normalize_text_bytes(b"a\rb", "x").unwrap(), b"a\nb\n");
        assert_eq!(normalize_text_bytes(b"a\n", "x").unwrap(), b"a\n");
        assert_eq!(normalize_text_bytes(b"a\n\n", "x").unwrap(), b"a\n\n");
        assert_eq!(
            normalize_text_bytes("\u{feff}a".as_bytes(), "x").unwrap(),
            b"a\n"
        );
        assert!(normalize_text_bytes(&[0xff, 0xfe], "x").is_err());
    }

    #[test]
    fn digest_changes_when_a_path_or_a_byte_changes() {
        let base = vec![
            ArtifactFile {
                path: "a/b.txt".to_owned(),
                sha256: sha256_hex(b"one\n"),
                bytes: 4,
            },
            ArtifactFile {
                path: "a/c.txt".to_owned(),
                sha256: sha256_hex(b"two\n"),
                bytes: 4,
            },
        ];
        let digest = artifact_integrity_digest(&base);
        assert_eq!(digest, artifact_integrity_digest(&base));
        let mut renamed = base.clone();
        renamed[0].path = "a/b2.txt".to_owned();
        assert_ne!(digest, artifact_integrity_digest(&renamed));
        let mut changed = base.clone();
        changed[1].sha256 = sha256_hex(b"TWO\n");
        assert_ne!(digest, artifact_integrity_digest(&changed));
        let mut fewer = base.clone();
        fewer.remove(0);
        assert_ne!(digest, artifact_integrity_digest(&fewer));
    }

    #[test]
    fn member_paths_reject_host_and_traversal_forms() {
        for bad in ["", "a\\b", "C:/x", "/abs", "./x", "../x", "a/../b"] {
            assert!(validate_member_path(bad).is_err(), "accepted {bad:?}");
        }
        assert!(validate_member_path("contract/packages/identity/src/lib.rs").is_ok());
    }

    #[test]
    fn migration_set_digest_is_backend_ordered_and_namespace_bound() {
        let entries = vec![
            MigrationSetEntry {
                backend: "sqlite".to_owned(),
                file_name: "001_a.sql".to_owned(),
                sha256: sha256_hex(b"sqlite"),
            },
            MigrationSetEntry {
                backend: "postgres".to_owned(),
                file_name: "019_a.sql".to_owned(),
                sha256: sha256_hex(b"postgres"),
            },
        ];
        let digest = migration_set_digest("identity-management", &entries);
        assert_eq!(
            digest,
            migration_set_digest("identity-management", &entries)
        );
        assert_eq!(
            digest,
            migration_set_digest(
                "identity-management",
                &entries.iter().rev().cloned().collect::<Vec<_>>()
            )
        );
        assert_ne!(digest, migration_set_digest("policy", &entries));
        let mut other = entries;
        other[0].sha256 = sha256_hex(b"changed");
        assert_ne!(digest, migration_set_digest("identity-management", &other));
    }
}
