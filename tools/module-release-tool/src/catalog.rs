//! Offline validation of the module migration catalogs (PLAN-0014 Wave 5B-S1).
//!
//! This module is a **read-only validator**: it reads files and read-only Git
//! objects and it never writes, never connects to a database and never runs a
//! migration. It exists because ADR-0029 is still *Proposed*: the layout it
//! checks (`migrations/modules/<namespace>/`) must be provable offline, long
//! before any runner, ledger table or CLI exists (design §11, §11.6).
//!
//! Three independently selectable checks share one implementation so that the
//! two S1 readers (`scripts/check-architecture.ps1` and the future
//! `runtime-migration/build.rs` of S2) can never drift apart:
//!
//! * [`CHECK_CATALOG`] — the metadata, path, manifest, dependency and SQL-owner
//!   rules of design §11.1, §11.2 and review §3.4/§3.5;
//! * [`CHECK_HISTORY`] — the pinned historical protection of design §11.3 as
//!   corrected by review §2 C1 / §3.6: the expectation table is generated from
//!   the fixed commit [`PINNED_BASELINE_REVISION`] and compares the pinned
//!   blob bytes, `HEAD`, the index and the working tree;
//! * [`CHECK_FREEZE`] — the *explicit candidate* rule that the flat baseline
//!   directory holds exactly the pinned 19 SQL files. It is never part of the
//!   default set: a global "no future baseline SQL, ever" policy waits for the
//!   accepted ADR-0029 (review §3.1, §3.6 rule 5).
//!
//! ## Digest rules, stated once
//!
//! * a *new* namespace catalog records exactly the canonical LF digest of each
//!   SQL file ([`crate::canonical::fold_line_endings`] then SHA-256); no other
//!   form is legal, so a `core.autocrlf` checkout cannot smuggle a second
//!   convention into a catalog that does not exist yet;
//! * the *historical* baseline manifest keeps the raw/LF/CRLF compatibility the
//!   repository already relies on (review §2 C1: 7 of the 19 historical entries
//!   match only the CRLF digest, and rewriting them would violate history
//!   protection). That compatibility is confined to the historical manifest and
//!   is guarded by the pinned blob digests, so it can never accept a content
//!   change;
//! * a Git object id is never treated as a content digest — the pinned table
//!   stores SHA-256 over the bytes `git cat-file blob` returns.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::canonical::{fold_line_endings, sha256_hex, validate_sha256_hex};
use crate::git::Git;
use crate::{ToolError, ToolResult};

/// Schema identifier of one `catalog.json` (design §5.2, §11.1).
pub const CATALOG_SCHEMA_VERSION: &str = "plan-0014.wave5b.catalog.v1";
/// File name of the namespace metadata inside a catalog directory.
pub const CATALOG_FILE_NAME: &str = "catalog.json";
/// File name of the per-catalog checksum manifest.
pub const CATALOG_MANIFEST_FILE_NAME: &str = "MANIFEST.sha256";
/// Repository-relative root of the module catalogs.
pub const MODULES_DIRECTORY: &str = "migrations/modules";
/// Repository-relative host baseline directory (frozen history).
pub const BASELINE_DIRECTORY: &str = "migrations";
/// The only ledger a module catalog may name (design §5.2, §5.3).
pub const REQUIRED_LEDGER_TABLE: &str = "platform_module_migrations";
/// The only SQL schema a Wave 5B catalog may name (design §5.2, §4 L3).
pub const REQUIRED_SQL_SCHEMA: &str = "public";
/// Dependency name of the frozen host baseline.
pub const BASELINE_DEPENDENCY_NAME: &str = "platform-baseline";
/// Header marker that lets a revision authorise a table it creates (§11.2 #11).
pub const OWNED_TABLE_DECLARATION: &str = "@bp-owned:";
/// An empty catalog is a reservation, never an installation (§11.2 #9).
pub const STATUS_RESERVED: &str = "RESERVED";
/// A non-empty catalog whose static contract holds.
pub const STATUS_VALIDATED: &str = "VALIDATED";

/// The fixed commit the historical baseline digests come from (review §3.6).
pub const PINNED_BASELINE_REVISION: &str = "5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601";
/// Schema identifier of the pinned history table.
pub const PINNED_HISTORY_SCHEMA_VERSION: &str = "plan-0014.wave5b.pinned-baseline-history.v1";
/// The pinned table itself, version-controlled next to the shared fixtures.
///
/// The expectation is *data in the repository*, produced from the fixed commit;
/// nothing here may derive it from the files currently on disk, because that
/// would turn "the history moved" into "the expectation follows along".
pub const PINNED_BASELINE_HISTORY_JSON: &str =
    include_str!("../tests/fixtures/wave5b/history/pinned-baseline-5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601.json");

/// Check identifier of the catalog static contract.
pub const CHECK_CATALOG: &str = "module-catalog";
/// Check identifier of the pinned historical protection.
pub const CHECK_HISTORY: &str = "pinned-baseline-history";
/// Check identifier of the explicit candidate freeze.
pub const CHECK_FREEZE: &str = "baseline-candidate-freeze";

/// Owner contexts that may own a migration namespace today.
///
/// Authority: `docs/architecture/BOUNDED_CONTEXT_MAP.md` §4/§5/§6, the
/// `[package.metadata.architecture]` blocks of `crates/identity`,
/// `crates/organization` and `crates/policy`, and the object attribution table
/// of the Wave 5B design §3.1. A new entry needs an accepted ADR or plan clause
/// naming the owner; an unknown context is refused rather than guessed at.
pub const AUTHORITATIVE_NAMESPACE_CONTEXTS: &[&str] =
    &["identity-management", "organization", "policy"];

/// Host baseline tables by owning context (Wave 5B design §3.1, derived from the
/// `019_identity_authorization_foundation.sql` header and its `CREATE TABLE`
/// text; `tests/wave5b_catalog.rs` re-checks the table against that file).
///
/// Anything absent from this list is *not* owned by the module that asks, which
/// is the point: the platform ledger, the host `tenants`/`documents`/`audit`
/// tables and the legacy `v0_demo_*` tables are deliberately not here.
pub const HOST_BASELINE_TABLE_OWNERSHIP: &[(&str, &str)] = &[
    ("platform_users", "identity-management"),
    ("external_identities", "identity-management"),
    ("tenant_memberships", "identity-management"),
    ("identity_idempotency", "identity-management"),
    ("platform_bootstrap_executions", "identity-management"),
    ("organization_units", "organization"),
    ("organization_members", "organization"),
    ("organization_idempotency", "organization"),
    ("permission_definitions", "policy"),
    ("roles", "policy"),
    ("role_permissions", "policy"),
    ("role_bindings", "policy"),
    ("policy_idempotency", "policy"),
];

/// One selectable check of [`validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Check {
    /// Catalog metadata, paths, manifest, dependencies and SQL ownership.
    Catalog,
    /// Pinned historical baseline protection.
    History,
    /// The single baseline embed point of review §3.7.
    Embed,
    /// Explicit candidate freeze of the flat baseline directory.
    Freeze,
}

impl Check {
    /// The stable name used on the command line and in the log.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Catalog => CHECK_CATALOG,
            Self::History => CHECK_HISTORY,
            Self::Embed => CHECK_EMBED,
            Self::Freeze => CHECK_FREEZE,
        }
    }

    /// Resolves one command-line check name.
    pub fn parse(name: &str) -> ToolResult<Self> {
        match name {
            CHECK_CATALOG => Ok(Self::Catalog),
            CHECK_HISTORY => Ok(Self::History),
            CHECK_EMBED => Ok(Self::Embed),
            CHECK_FREEZE => Ok(Self::Freeze),
            other => Err(ToolError::Usage(format!(
                "unknown check '{other}'; expected one of {CHECK_CATALOG}, {CHECK_HISTORY}, {CHECK_EMBED}, {CHECK_FREEZE}"
            ))),
        }
    }
}

/// The default set a repository gate runs: the catalog contract, the pinned
/// history and the single embed point — without the candidate-only freeze, which
/// needs an explicit `--only` until ADR-0029 is accepted.
#[must_use]
pub fn default_checks() -> Vec<Check> {
    vec![Check::Catalog, Check::History, Check::Embed]
}

/// Refuses with a named check and an observed-versus-expected reason.
fn refuse(check: &str, reason: impl Into<String>) -> ToolError {
    ToolError::Verification {
        check: check.to_owned(),
        reason: reason.into(),
    }
}

// ---------------------------------------------------------------------------
// pinned history table
// ---------------------------------------------------------------------------

/// One entry of the pinned history table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedEntry {
    /// Repository-relative POSIX path.
    pub path: String,
    /// `baseline-sql` or `baseline-manifest`.
    pub kind: String,
    /// Revision token, present for SQL entries only.
    #[serde(default)]
    pub revision: Option<String>,
    /// SHA-256 of the pinned blob bytes.
    pub blob_sha256: String,
    /// What the historical manifest recorded for this file.
    #[serde(default)]
    pub historical_manifest_digest: Option<String>,
    /// Which line-ending form that recorded digest corresponds to.
    #[serde(default)]
    pub historical_manifest_form: Option<String>,
}

/// The frozen expectation for the historical baseline.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedBaselineHistory {
    /// Table schema identifier.
    pub schema_version: String,
    /// The commit every entry was read out of.
    pub pinned_revision: String,
    /// Number of baseline SQL revisions, i.e. the highest available one.
    pub baseline_revision: u32,
    /// Human-readable digest rule, shipped so a reviewer never has to guess.
    pub digest_rule: String,
    /// Entries, ordered by path.
    pub entries: Vec<PinnedEntry>,
}

impl PinnedBaselineHistory {
    /// The pinned baseline SQL entries, ordered by path.
    #[must_use]
    pub fn sql_entries(&self) -> Vec<&PinnedEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.kind == "baseline-sql")
            .collect()
    }
}

/// Reads the version-controlled pinned history table.
///
/// The table is validated rather than trusted: a schema identifier that is not
/// the current one, a pinned revision that is not the constant, an entry set
/// that is not ordered, unique and complete, or a digest that is not lowercase
/// hex is a refusal here rather than a strange failure later.
#[allow(clippy::too_many_lines)] // one rule per branch: the check reads as the rule list it implements
pub fn pinned_baseline_history() -> ToolResult<PinnedBaselineHistory> {
    let table: PinnedBaselineHistory =
        serde_json::from_str(PINNED_BASELINE_HISTORY_JSON).map_err(|error| {
            refuse(
                CHECK_HISTORY,
                format!("pinned history table does not parse: {error}"),
            )
        })?;
    if table.schema_version != PINNED_HISTORY_SCHEMA_VERSION {
        return Err(refuse(
            CHECK_HISTORY,
            format!(
                "pinned history table declares '{}' but '{PINNED_HISTORY_SCHEMA_VERSION}' is required",
                table.schema_version
            ),
        ));
    }
    if table.pinned_revision != PINNED_BASELINE_REVISION {
        return Err(refuse(
            CHECK_HISTORY,
            format!(
                "pinned history table pins '{}' but this build pins '{PINNED_BASELINE_REVISION}'",
                table.pinned_revision
            ),
        ));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut previous: Option<&str> = None;
    let mut sql = 0_u32;
    for entry in &table.entries {
        validate_sha256_hex(&entry.blob_sha256, "pinned baseline blob digest")
            .map_err(|error| refuse(CHECK_HISTORY, error.to_string()))?;
        if !seen.insert(entry.path.as_str()) {
            return Err(refuse(
                CHECK_HISTORY,
                format!("pinned history table lists '{}' twice", entry.path),
            ));
        }
        if previous.is_some_and(|before| before >= entry.path.as_str()) {
            return Err(refuse(
                CHECK_HISTORY,
                format!(
                    "pinned history table is not ordered by path around '{}'",
                    entry.path
                ),
            ));
        }
        previous = Some(entry.path.as_str());
        match entry.kind.as_str() {
            "baseline-sql" => {
                sql = sql
                    .checked_add(1)
                    .ok_or_else(|| refuse(CHECK_HISTORY, "pinned SQL entry count overflowed"))?;
                if entry.revision.as_deref().unwrap_or_default().len() != 3 {
                    return Err(refuse(
                        CHECK_HISTORY,
                        format!("pinned SQL entry '{}' lacks a 3-digit revision", entry.path),
                    ));
                }
                let name = path_basename(&entry.path).ok_or_else(|| {
                    refuse(
                        CHECK_HISTORY,
                        format!("pinned entry '{}' is not a basename path", entry.path),
                    )
                })?;
                if !entry.path.starts_with(&format!("{BASELINE_DIRECTORY}/"))
                    || !is_sql_file_name(name)
                {
                    return Err(refuse(
                        CHECK_HISTORY,
                        format!(
                            "pinned SQL entry '{}' is not inside '{BASELINE_DIRECTORY}'",
                            entry.path
                        ),
                    ));
                }
            }
            "baseline-manifest" => {}
            other => {
                return Err(refuse(
                    CHECK_HISTORY,
                    format!("pinned entry '{}' has unknown kind '{other}'", entry.path),
                ))
            }
        }
    }
    if sql != table.baseline_revision {
        return Err(refuse(
            CHECK_HISTORY,
            format!(
                "pinned history table declares baseline_revision {} but lists {sql} SQL entries",
                table.baseline_revision
            ),
        ));
    }
    if !table
        .entries
        .iter()
        .any(|entry| entry.kind == "baseline-manifest")
    {
        return Err(refuse(
            CHECK_HISTORY,
            "pinned history table does not pin the baseline MANIFEST.sha256",
        ));
    }
    Ok(table)
}

fn path_basename(path: &str) -> Option<&str> {
    if path.contains('\\') {
        return None;
    }
    let name = path.rsplit('/').next()?;
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return None;
    }
    Some(name)
}

// ---------------------------------------------------------------------------
// catalog metadata
// ---------------------------------------------------------------------------

/// One parsed `catalog.json` (design §5.2: exactly these six fields).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    /// Catalog schema identifier.
    pub schema_version: String,
    /// Directory-scoped migration namespace.
    pub namespace: String,
    /// Owner bounded context.
    pub owner: String,
    /// Ledger table the runner will use.
    pub ledger: String,
    /// `PostgreSQL` schema the DDL targets.
    pub sql_schema: String,
    /// Declared dependencies, in declaration order.
    pub depends_on: Vec<String>,
}

/// Parses one `catalog.json` under the strict rules of design §11.1.
///
/// Unknown fields, missing fields, wrong types and *duplicate JSON members* are
/// all refusals: `serde`'s `deny_unknown_fields` catches the first three, and
/// [`scan_json_for_duplicate_members`] closes the last one, which serde would
/// otherwise silently resolve to "the later value wins".
pub fn parse_catalog(text: &str, shown: &str) -> ToolResult<Catalog> {
    scan_json_for_duplicate_members(text)
        .map_err(|reason| refuse(CHECK_CATALOG, format!("{shown}: {reason}")))?;
    let catalog: Catalog = serde_json::from_str(text)
        .map_err(|error| refuse(CHECK_CATALOG, format!("{shown}: {error}")))?;
    if catalog.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "{shown}: schema_version '{}' is not '{CATALOG_SCHEMA_VERSION}'",
                catalog.schema_version
            ),
        ));
    }
    Ok(catalog)
}

/// Rejects duplicate members and malformed JSON without a second parser.
///
/// `serde_json` resolves a duplicate object member to its last value, so a
/// `catalog.json` that declares `namespace` twice with different values would
/// otherwise pass. This scanner walks the text once, tracks the member names of
/// every object it is inside, and reports the first collision it finds; it is
/// deliberately a *rejection* scanner — anything it cannot prove well-formed is
/// refused and `serde` still does the typed parse afterwards.
pub fn scan_json_for_duplicate_members(text: &str) -> ToolResult<()> {
    let characters: Vec<char> = text.chars().collect();
    let mut cursor = JsonCursor {
        characters: &characters,
        position: 0,
    };
    cursor.skip_trivia();
    cursor.scan_value(&mut Vec::new())?;
    cursor.skip_trivia();
    if cursor.position != characters.len() {
        return Err(refuse(
            CHECK_CATALOG,
            format!("JSON has trailing content at offset {}", cursor.position),
        ));
    }
    Ok(())
}

/// Cursor for [`scan_json_for_duplicate_members`].
struct JsonCursor<'a> {
    characters: &'a [char],
    position: usize,
}

impl JsonCursor<'_> {
    fn fail<T>(&self, reason: &str) -> ToolResult<T> {
        Err(refuse(
            CHECK_CATALOG,
            format!(
                "JSON is not well-formed at offset {}: {reason}",
                self.position
            ),
        ))
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.position).copied()
    }

    fn skip_trivia(&mut self) {
        while let Some(character) = self.peek() {
            if character.is_whitespace() {
                self.position += 1;
            } else {
                break;
            }
        }
    }

    fn expect_char(&mut self, character: char) -> ToolResult<()> {
        if self.peek() == Some(character) {
            self.position += 1;
            self.skip_trivia();
            return Ok(());
        }
        self.fail::<()>(&format!("expected '{character}'"))
    }

    fn scan_value(&mut self, stack: &mut Vec<String>) -> ToolResult<()> {
        match self.peek() {
            Some('{') => self.scan_object(stack),
            Some('[') => self.scan_array(),
            Some('"') => self.scan_string().map(|_| ()),
            Some(_) => self.scan_atom(),
            None => self.fail("unexpected end of input"),
        }
    }

    fn scan_object(&mut self, stack: &mut Vec<String>) -> ToolResult<()> {
        self.expect_char('{')?;
        let base = stack.len();
        let mut members: Vec<String> = Vec::new();
        if self.peek() == Some('}') {
            self.position += 1;
            self.skip_trivia();
            return Ok(());
        }
        loop {
            let name = self.scan_string()?;
            if let Some(duplicate) = members.iter().find(|seen| **seen == name) {
                stack.push(name.clone());
                let path = stack.join(".");
                stack.truncate(base);
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("JSON member '{duplicate}' is declared twice in '{path}'"),
                ));
            }
            members.push(name.clone());
            stack.push(name);
            self.expect_char(':')?;
            self.scan_value(stack)?;
            stack.truncate(base);
            match self.peek() {
                Some(',') => {
                    self.position += 1;
                    self.skip_trivia();
                }
                Some('}') => {
                    self.position += 1;
                    self.skip_trivia();
                    return Ok(());
                }
                _ => return self.fail("expected ',' or '}' in object"),
            }
        }
    }

    fn scan_array(&mut self) -> ToolResult<()> {
        self.expect_char('[')?;
        if self.peek() == Some(']') {
            self.position += 1;
            self.skip_trivia();
            return Ok(());
        }
        loop {
            self.scan_value(&mut Vec::new())?;
            match self.peek() {
                Some(',') => {
                    self.position += 1;
                    self.skip_trivia();
                }
                Some(']') => {
                    self.position += 1;
                    self.skip_trivia();
                    return Ok(());
                }
                _ => return self.fail("expected ',' or ']' in array"),
            }
        }
    }

    fn scan_string(&mut self) -> ToolResult<String> {
        self.expect_char('"')?;
        let mut value = String::new();
        loop {
            let Some(character) = self.peek() else {
                return self.fail("unterminated string");
            };
            self.position += 1;
            match character {
                '"' => {
                    self.skip_trivia();
                    return Ok(value);
                }
                '\\' => {
                    let Some(escape) = self.peek() else {
                        return self.fail("unterminated escape");
                    };
                    self.position += 1;
                    match escape {
                        '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => value.push(escape),
                        'u' => {
                            for _ in 0..4 {
                                let digit = self.peek().ok_or_else(|| {
                                    refuse(CHECK_CATALOG, "truncated \\u escape".to_owned())
                                })?;
                                if !digit.is_ascii_hexdigit() {
                                    return self.fail("non-hexadecimal \\u escape");
                                }
                                self.position += 1;
                            }
                        }
                        other => return self.fail(&format!("invalid escape '\\{other}'")),
                    }
                }
                '\u{0000}'..='\u{001f}' => return self.fail("control character in string"),
                other => value.push(other),
            }
        }
    }

    fn scan_atom(&mut self) -> ToolResult<()> {
        for candidate in ["true", "false", "null"] {
            if self.characters[self.position..].starts_with(&candidate.chars().collect::<Vec<_>>())
            {
                self.position += candidate.len();
                self.skip_trivia();
                return Ok(());
            }
        }
        let start = self.position;
        while let Some(character) = self.peek() {
            if character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E') {
                self.position += 1;
            } else {
                break;
            }
        }
        if start == self.position {
            let found = self.peek().unwrap_or(' ');
            return self.fail(&format!("unexpected character '{found}'"));
        }
        self.skip_trivia();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// paths
// ---------------------------------------------------------------------------

/// Rejects a path component that is not a plain name.
fn validate_name(name: &str, what: &str) -> ToolResult<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains(':')
        || name.chars().any(char::is_control)
    {
        return Err(refuse(
            CHECK_CATALOG,
            format!("{what} '{name}' is not a plain file or directory name"),
        ));
    }
    Ok(())
}

/// Whether `path` is a symbolic link or, on Windows, a reparse point.
#[must_use]
pub fn is_reparse_or_link(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Opens `path` as a regular file, refusing links, reparse points and anything
/// that is not a plain file *before* reading it (review §3.4: never "follow the
/// link first and check afterwards"). `check` is the check that is asking, so a
/// refusal always names the check that produced it.
fn read_regular_file(check: &str, path: &Path, what: &str) -> ToolResult<Vec<u8>> {
    let shown = path.display().to_string();
    let refused = |reason: String| refuse(check, reason);
    let metadata = std::fs::symlink_metadata(path).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    if is_reparse_or_link(&metadata) {
        return Err(refused(format!(
            "{what} '{shown}' is a symbolic link or reparse point"
        )));
    }
    if !metadata.is_file() {
        return Err(refused(format!("{what} '{shown}' is not a regular file")));
    }
    let bytes = std::fs::read(path).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    // The entry is re-checked after the read: a link installed between the first
    // `symlink_metadata` and the read must not silently deliver foreign bytes.
    let after = std::fs::symlink_metadata(path).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    if is_reparse_or_link(&after) || !after.is_file() {
        return Err(refused(format!(
            "'{shown}' became a symbolic link or a non-file while it was being read"
        )));
    }
    Ok(bytes)
}

/// Opens `path` as a plain directory, refusing links and reparse points.
fn read_regular_directory(check: &str, path: &Path, what: &str) -> ToolResult<()> {
    let shown = path.display().to_string();
    let metadata = std::fs::symlink_metadata(path).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    if is_reparse_or_link(&metadata) {
        return Err(refuse(
            check,
            format!("{what} '{shown}' is a symbolic link or reparse point"),
        ));
    }
    if !metadata.is_dir() {
        return Err(refuse(
            check,
            format!("{what} '{shown}' is not a directory"),
        ));
    }
    Ok(())
}

/// Resolves `root` to its canonical form. Every path the validator reads is
/// then required to resolve *below* this canonical root, so a symbolic link or
/// junction anywhere in the middle of a path (`migrations/` itself, a namespace
/// directory, the catalog root) cannot smuggle content in from outside the
/// checkout under review (review §3.4 path rules, §3.8 link fixtures).
fn canonical_root(root: &Path, check: &str) -> ToolResult<PathBuf> {
    std::fs::canonicalize(root).map_err(|source| ToolError::Io {
        path: format!("{} ({check})", root.display()),
        source,
    })
}

/// Canonicalises `path` and refuses it when the resolution leaves `root`.
fn assert_canonical_within(
    canonical_root: &Path,
    path: &Path,
    check: &str,
    what: &str,
) -> ToolResult<PathBuf> {
    let shown = path.display().to_string();
    let canonical = std::fs::canonicalize(path).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    if canonical == canonical_root || !canonical.starts_with(canonical_root) {
        return Err(refuse(
            check,
            format!(
                "{what} '{shown}' resolves to '{}' rather than to a path inside the checked-out \
                 root '{}'",
                canonical.display(),
                canonical_root.display()
            ),
        ));
    }
    Ok(canonical)
}

/// Confirms `path` still resolves inside `ancestor` after component resolution.
fn assert_within(ancestor: &Path, path: &Path) -> ToolResult<()> {
    let relative = path.strip_prefix(ancestor).map_err(|_| {
        refuse(
            CHECK_CATALOG,
            format!(
                "'{}' escapes the catalog root '{}'",
                path.display(),
                ancestor.display()
            ),
        )
    })?;
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "'{}' reaches outside '{}' through '.' or '..'",
                    path.display(),
                    ancestor.display()
                ),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// namespace discovery
// ---------------------------------------------------------------------------

/// One SQL revision of a namespace catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// File name inside the catalog directory.
    pub file_name: String,
    /// Numeric revision, e.g. `1` for `001_…`.
    pub number: u32,
    /// Tables this revision creates, in the order they are created.
    pub created: Vec<String>,
}

/// One validated namespace catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    /// Directory name under `migrations/modules/`.
    pub directory: String,
    /// Parsed metadata.
    pub catalog: Catalog,
    /// Manifest entries, ordered by file name.
    pub manifest: BTreeMap<String, String>,
    /// Revisions in ascending order.
    pub revisions: Vec<Revision>,
    /// Tables the namespace owns after applying every revision in this catalog.
    pub owned_tables: BTreeSet<String>,
}

impl Namespace {
    /// The status this catalog reports: a reservation, or a validated set.
    ///
    /// There is deliberately no third state here: nothing in this package can
    /// observe an installation, so nothing may print one (§11.2 #9).
    #[must_use]
    pub fn status(&self) -> &'static str {
        if self.revisions.is_empty() {
            STATUS_RESERVED
        } else {
            STATUS_VALIDATED
        }
    }
}

/// Discovers and validates every namespace catalog below `root`.
pub fn validate_catalogs(root: &Path) -> ToolResult<Vec<Namespace>> {
    let canonical_root = canonical_root(root, CHECK_CATALOG)?;
    let modules = root.join(MODULES_DIRECTORY);
    read_regular_directory(CHECK_CATALOG, &modules, "module catalog root")?;
    assert_within(root, &modules)?;
    assert_canonical_within(
        &canonical_root,
        &modules,
        CHECK_CATALOG,
        "module catalog root",
    )?;
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    let read = std::fs::read_dir(&modules).map_err(|source| ToolError::Io {
        path: modules.display().to_string(),
        source,
    })?;
    for entry in read {
        let entry = entry.map_err(|source| ToolError::Io {
            path: modules.display().to_string(),
            source,
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        validate_name(&name, "namespace directory")?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if is_reparse_or_link(&metadata) {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "namespace directory '{}' is a symbolic link or reparse point",
                    path.display()
                ),
            ));
        }
        assert_canonical_within(&canonical_root, &path, CHECK_CATALOG, "namespace directory")?;
        if !metadata.is_dir() {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "'{}' is not a namespace directory; only one directory per namespace is \
                     allowed below '{MODULES_DIRECTORY}'",
                    path.display()
                ),
            ));
        }
        entries.push((name, path));
    }
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    assert_no_case_collisions(
        &entries
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>(),
        "namespace directory",
    )?;

    let mut namespaces: Vec<Namespace> = Vec::new();
    for (name, path) in &entries {
        namespaces.push(validate_namespace(name, path)?);
    }
    let mut created_by: BTreeMap<String, &str> = BTreeMap::new();
    for namespace in &namespaces {
        for table in &namespace.owned_tables {
            if let Some(other) = created_by.insert(table.clone(), &namespace.catalog.namespace) {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!(
                        "table '{table}' is created by namespace '{other}' and by '{}'; two \
                         namespaces may not create the same table",
                        namespace.catalog.namespace
                    ),
                ));
            }
        }
    }
    Ok(namespaces)
}

/// Rejects two names that differ only by case (review §3.4).
fn assert_no_case_collisions(names: &[String], what: &str) -> ToolResult<()> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for name in names {
        let key = name.to_lowercase();
        if let Some(previous) = seen.get(&key) {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{what} '{name}' differs only by case from '{previous}'; the catalog must not \
                     depend on a case-sensitive file system",
                ),
            ));
        }
        seen.insert(key, name.clone());
    }
    Ok(())
}

/// Validates one namespace directory: metadata, manifest, files and SQL owner.
fn validate_namespace(directory: &str, path: &Path) -> ToolResult<Namespace> {
    let catalog_path = path.join(CATALOG_FILE_NAME);
    let catalog_text =
        read_regular_file(CHECK_CATALOG, &catalog_path, "catalog file").and_then(|bytes| {
            String::from_utf8(bytes).map_err(|_| {
                refuse(
                    CHECK_CATALOG,
                    format!(
                        "catalog file '{}' is not valid UTF-8",
                        catalog_path.display()
                    ),
                )
            })
        })?;
    let shown = catalog_path.display().to_string();
    let catalog = parse_catalog(&catalog_text, &shown)?;

    if catalog.namespace != directory {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "{shown}: namespace '{}' does not equal the directory name '{directory}'",
                catalog.namespace
            ),
        ));
    }
    if catalog.owner != catalog.namespace {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "{shown}: owner '{}' does not equal namespace '{}'",
                catalog.owner, catalog.namespace
            ),
        ));
    }
    validate_namespace_name(&catalog.namespace)?;
    if !AUTHORITATIVE_NAMESPACE_CONTEXTS.contains(&catalog.owner.as_str()) {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "{shown}: owner '{}' is not one of the authoritative contexts {:?}; adding one \
                 requires an accepted ADR or plan clause",
                catalog.owner, AUTHORITATIVE_NAMESPACE_CONTEXTS
            ),
        ));
    }
    if catalog.ledger != REQUIRED_LEDGER_TABLE {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "{shown}: ledger '{}' is not '{REQUIRED_LEDGER_TABLE}'",
                catalog.ledger
            ),
        ));
    }
    if catalog.sql_schema != REQUIRED_SQL_SCHEMA {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "{shown}: sql_schema '{}' is not '{REQUIRED_SQL_SCHEMA}'; physical schema \
                 isolation is not part of Wave 5B",
                catalog.sql_schema
            ),
        ));
    }

    let (manifest, files) = read_manifest_and_files(path)?;
    let revisions = reconcile_manifest_and_files(path, &manifest, &files)?;
    let (owned_tables, revisions) = scan_sql_ownership(path, &catalog, &revisions)?;

    Ok(Namespace {
        directory: directory.to_owned(),
        catalog,
        manifest,
        revisions,
        owned_tables,
    })
}

/// Validates the namespace name grammar, which is also the ledger's `CHECK`.
fn validate_namespace_name(name: &str) -> ToolResult<()> {
    let valid = name.len() <= 63
        && name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    if !valid {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "namespace '{name}' does not match the ledger constraint \
                 '^[a-z0-9][a-z0-9-]{{0,62}}$'"
            ),
        ));
    }
    Ok(())
}

/// Reads `MANIFEST.sha256` and lists the SQL files of one catalog directory.
fn read_manifest_and_files(path: &Path) -> ToolResult<(BTreeMap<String, String>, Vec<String>)> {
    let manifest_path = path.join(CATALOG_MANIFEST_FILE_NAME);
    let manifest_bytes = read_regular_file(CHECK_CATALOG, &manifest_path, "catalog manifest")?;
    let shown = manifest_path.display().to_string();
    let mut manifest: BTreeMap<String, String> = BTreeMap::new();
    if manifest_bytes.is_empty() {
        // §11.5: an empty catalog is *literally* empty, not a file with one
        // blank line, so "RESERVED" cannot be faked by a whitespace manifest.
        return Ok((manifest, list_sql_files(path)?));
    }
    let text = String::from_utf8(manifest_bytes).map_err(|_| {
        refuse(
            CHECK_CATALOG,
            format!("catalog manifest '{shown}' is not valid UTF-8"),
        )
    })?;
    for line in split_lf(&text) {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            return Err(refuse(
                CHECK_CATALOG,
                format!("catalog manifest '{shown}' contains a blank line"),
            ));
        }
        let Some((digest, file_name)) = line.split_once("  ") else {
            return Err(refuse(
                CHECK_CATALOG,
                format!("catalog manifest '{shown}' has an unparsable line '{line}'"),
            ));
        };
        validate_sha256_hex(digest, "catalog manifest digest")
            .map_err(|error| refuse(CHECK_CATALOG, format!("'{shown}': {error}")))?;
        validate_manifest_entry_name(file_name, &shown)?;
        if manifest
            .insert(file_name.to_owned(), digest.to_owned())
            .is_some()
        {
            return Err(refuse(
                CHECK_CATALOG,
                format!("catalog manifest '{shown}' lists '{file_name}' twice"),
            ));
        }
    }
    Ok((manifest, list_sql_files(path)?))
}

/// Splits on LF without the trailing empty piece.
fn split_lf(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
}

/// True when a bare file name carries the `.sql` extension, compared the way a
/// file system compares extensions (the entry grammar itself stays lowercase).
fn is_sql_file_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
}

/// A manifest entry is a bare `NNN_name.sql`, never a path (review §3.4).
fn validate_manifest_entry_name(name: &str, shown: &str) -> ToolResult<()> {
    let valid = name.len() > 8
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && name.as_bytes()[..3].iter().all(u8::is_ascii_digit)
        && name.as_bytes()[3] == b'_'
        && is_sql_file_name(name)
        && name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '.' | '-')
        });
    if !valid || name.contains("..") {
        return Err(refuse(
            CHECK_CATALOG,
            format!(
                "catalog manifest '{shown}' entry '{name}' is not a plain 'NNN_lower_name.sql' \
                 basename"
            ),
        ));
    }
    Ok(())
}

/// Lists `*.sql` files in one catalog directory, refusing links and odd entries.
fn list_sql_files(path: &Path) -> ToolResult<Vec<String>> {
    let mut names = Vec::new();
    let read = std::fs::read_dir(path).map_err(|source| ToolError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        entries.push((
            entry.file_name().to_string_lossy().into_owned(),
            entry.path(),
        ));
    }
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    for (name, file) in entries {
        validate_name(&name, "catalog entry")?;
        if is_sql_file_name(&name) {
            let metadata = std::fs::symlink_metadata(&file).map_err(|source| ToolError::Io {
                path: file.display().to_string(),
                source,
            })?;
            if is_reparse_or_link(&metadata) {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!(
                        "SQL file '{}' is a symbolic link or reparse point",
                        file.display()
                    ),
                ));
            }
            if !metadata.is_file() {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("SQL file '{}' is not a regular file", file.display()),
                ));
            }
            names.push(name);
        } else if name != CATALOG_FILE_NAME && name != CATALOG_MANIFEST_FILE_NAME {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "'{}' is neither SQL nor catalog metadata; a namespace catalog directory may \
                     not hold other entries",
                    file.display()
                ),
            ));
        }
    }
    assert_no_case_collisions(&names, "catalog SQL file")?;
    Ok(names)
}

/// Checks the manifest and the files against each other and returns revisions.
fn reconcile_manifest_and_files(
    path: &Path,
    manifest: &BTreeMap<String, String>,
    files: &[String],
) -> ToolResult<Vec<Revision>> {
    let mut revisions: Vec<Revision> = Vec::new();
    for file in files {
        let Some(digest) = manifest.get(file) else {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "'{}' exists in the catalog directory but is absent from \
                     '{CATALOG_MANIFEST_FILE_NAME}'",
                    path.join(file).display()
                ),
            ));
        };
        validate_manifest_entry_name(file, CATALOG_MANIFEST_FILE_NAME)?;
        let number = file[..3]
            .parse::<u32>()
            .map_err(|error| refuse(CHECK_CATALOG, format!("'{file}': {error}")))?;
        let raw = read_regular_file(CHECK_CATALOG, &path.join(file), "SQL file")?;
        let canonical = sha256_hex(&fold_line_endings(&raw));
        if canonical != *digest {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "'{}' hashes to the canonical LF digest '{canonical}' but \
                     '{CATALOG_MANIFEST_FILE_NAME}' records '{digest}'; a module catalog records \
                     the canonical digest only (no raw or CRLF form)",
                    path.join(file).display()
                ),
            ));
        }
        revisions.push(Revision {
            file_name: file.clone(),
            number,
            created: Vec::new(),
        });
    }
    for name in manifest.keys() {
        if !files.iter().any(|file| file == name) {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "'{name}' is listed in '{CATALOG_MANIFEST_FILE_NAME}' but no such file is in \
                     '{}'",
                    path.display()
                ),
            ));
        }
    }
    revisions.sort_by_key(|revision| revision.number);
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for (position, revision) in revisions.iter().enumerate() {
        if !seen.insert(revision.number) {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "revision {:03} is used by more than one file in '{}'",
                    revision.number,
                    path.display()
                ),
            ));
        }
        let expected = u32::try_from(position + 1)
            .map_err(|_| refuse(CHECK_CATALOG, "a catalog cannot hold that many revisions"))?;
        if revision.number != expected {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "revision {:03} of '{}' leaves a gap: revisions must run from 001 without a \
                     hole",
                    revision.number,
                    path.display()
                ),
            ));
        }
    }
    Ok(revisions)
}

// ---------------------------------------------------------------------------
// SQL owner scan (static text fitness)
// ---------------------------------------------------------------------------

/// One table-level event found in a revision's SQL text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlEvent {
    /// The revision creates this table.
    Create(String),
    /// The revision references a table it does not create here.
    Reference(String),
}

/// Splits SQL text into statements of tokens, comments and literals removed.
///
/// The tokenizer is deliberately small and fails closed: a dollar-quoted block
/// (dynamic SQL), an unbalanced quote or a comment that never ends is a named
/// refusal, because a scanner that quietly skips what it does not understand is
/// exactly how a foreign table reference would slip through (review §3.5).
#[must_use]
pub fn is_owned_table_declaration(line: &str) -> Option<String> {
    let comment = line.trim().strip_prefix("--")?;
    let rest = comment.trim_start();
    let value = rest.strip_prefix(OWNED_TABLE_DECLARATION)?.trim();
    if value.is_empty() || value.contains(char::is_whitespace) {
        return None;
    }
    Some(value.to_owned())
}

/// Reads the leading `-- @bp-owned:` declarations of one revision file.
///
/// A declaration is only read while the file has not started its statements yet:
/// a marker hidden between statements would let a revision authorise a table
/// after the fact, which is exactly the audit trail design §11.2 #11 depends on.
fn scan_declarations(text: &str, shown: &str) -> ToolResult<Vec<String>> {
    let mut declarations = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.trim_start().starts_with("--") {
            break;
        }
        if let Some(table) = is_owned_table_declaration(line) {
            validate_table_name(&table, shown)?;
            if declarations.iter().any(|seen: &String| seen == &table) {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("{shown}: '{OWNED_TABLE_DECLARATION} {table}' is declared twice"),
                ));
            }
            declarations.push(table);
        }
    }
    Ok(declarations)
}

/// Analyses one revision file: its declarations and its table events in order.
pub fn analyse_sql(text: &str, shown: &str) -> ToolResult<(Vec<String>, Vec<SqlEvent>)> {
    let declarations = scan_declarations(text, shown)?;
    let mut events: Vec<SqlEvent> = Vec::new();
    for tokens in sql_statements(text, shown)? {
        let Some(first) = first_keyword(&tokens) else {
            continue;
        };
        events.extend(statement_events(&tokens, first, shown)?);
    }
    for declared in &declarations {
        if !events
            .iter()
            .any(|event| matches!(event, SqlEvent::Create(created) if created == declared))
        {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{shown}: declares '{OWNED_TABLE_DECLARATION} {declared}' but the revision \
                     never creates it",
                ),
            ));
        }
    }
    Ok((declarations, events))
}

/// One token of a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// An identifier or keyword, lower-cased unless it was quoted.
    Word(String),
    /// Punctuation such as `(`, `)`, `,`, `.`.
    Symbol(String),
}

impl Token {
    /// The keyword text of a word token, or an empty string.
    #[must_use]
    fn keyword(&self) -> &str {
        match self {
            Self::Word(word) => word,
            Self::Symbol(_) => "",
        }
    }
}

#[allow(clippy::too_many_lines)] // one rule per branch: the check reads as the rule list it implements
fn sql_statements(text: &str, shown: &str) -> ToolResult<Vec<Vec<Token>>> {
    let characters: Vec<char> = text.chars().collect();
    let mut statements: Vec<Vec<Token>> = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    let mut index = 0_usize;
    while index < characters.len() {
        let character = characters[index];
        if character == '-' && characters.get(index + 1) == Some(&'-') {
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if character == '/' && characters.get(index + 1) == Some(&'*') {
            let mut depth = 0_usize;
            while index < characters.len() {
                if characters[index] == '/' && characters.get(index + 1) == Some(&'*') {
                    depth += 1;
                    index += 2;
                } else if characters[index] == '*' && characters.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    index += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    index += 1;
                }
            }
            if depth != 0 {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("{shown}: a block comment is never closed"),
                ));
            }
            continue;
        }
        if character == '$' {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{shown}: dollar-quoted or dynamic SQL is not supported by the owner scan; \
                     it must be reviewed by hand"
                ),
            ));
        }
        if character == '\'' {
            index += 1;
            let mut closed = false;
            while index < characters.len() {
                if characters[index] == '\'' {
                    if characters.get(index + 1) == Some(&'\'') {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("{shown}: a string literal is never closed"),
                ));
            }
            current.push(Token::Word("''".to_owned()));
            continue;
        }
        if character == '"' {
            let start = index + 1;
            index += 1;
            while index < characters.len() && characters[index] != '"' {
                index += 1;
            }
            if index >= characters.len() {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("{shown}: a quoted identifier is never closed"),
                ));
            }
            let quoted: String = characters[start..index].iter().collect();
            index += 1;
            if quoted.chars().any(char::is_uppercase) {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!(
                        "{shown}: quoted identifier '{quoted}' mixes case; a module migration must \
                         use plain lower-case names"
                    ),
                ));
            }
            current.push(Token::Word(quoted));
            continue;
        }
        if character.is_whitespace() || character == ';' {
            if character == ';' {
                statements.push(std::mem::take(&mut current));
            }
            index += 1;
            continue;
        }
        if character.is_ascii_alphanumeric() || character == '_' {
            let start = index;
            while index < characters.len()
                && (characters[index].is_ascii_alphanumeric() || characters[index] == '_')
            {
                index += 1;
            }
            let word: String = characters[start..index].iter().collect();
            current.push(Token::Word(word.to_ascii_lowercase()));
            continue;
        }
        if character == '%' {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{shown}: a format placeholder suggests dynamic SQL, which the owner scan \
                        does not support"
                ),
            ));
        }
        current.push(Token::Symbol(character.to_string()));
        index += 1;
    }
    if !current.is_empty() {
        statements.push(current);
    }
    Ok(statements)
}

fn first_keyword(tokens: &[Token]) -> Option<&str> {
    tokens.iter().find_map(|token| match token {
        Token::Word(word) => Some(word.as_str()),
        Token::Symbol(_) => None,
    })
}

fn validate_table_name(name: &str, shown: &str) -> ToolResult<()> {
    let valid = !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character == '_')
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        });
    if !valid {
        return Err(refuse(
            CHECK_CATALOG,
            format!("{shown}: '{name}' is not a plain lower-case table name"),
        ));
    }
    Ok(())
}

/// The word after `skip` keywords, or an error if there is none.
fn object_after(
    tokens: &[Token],
    from: usize,
    skip: &[&str],
    shown: &str,
) -> ToolResult<(String, usize)> {
    let mut index = from;
    loop {
        let Some(token) = tokens.get(index) else {
            return Err(refuse(
                CHECK_CATALOG,
                format!("{shown}: a statement ends before its object name"),
            ));
        };
        match token {
            Token::Word(word) if skip.contains(&word.as_str()) => index += 1,
            Token::Word(word) => {
                validate_table_name(word, shown)?;
                if matches!(tokens.get(index + 1), Some(Token::Symbol(symbol)) if symbol == ".") {
                    return Err(refuse(
                        CHECK_CATALOG,
                        format!(
                            "{shown}: '{word}' is used as a schema-qualified reference; Wave 5B \
                             catalogs stay unqualified, so a qualified name needs a decision \
                             before it may pass"
                        ),
                    ));
                }
                return Ok((word.clone(), index + 1));
            }
            Token::Symbol(symbol) => {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("{shown}: expected an object name, found '{symbol}'"),
                ))
            }
        }
    }
}

/// Turns one statement into table events, refusing constructs the scan cannot
/// judge (review §3.5: an unsupported form is a named failure, never a success).
///
/// The supported subset is the SQL a module migration legitimately needs and
/// nothing more: `CREATE TABLE`, `CREATE [UNIQUE] INDEX`, `ALTER TABLE`,
/// `DROP TABLE`, `TRUNCATE`, `INSERT INTO`, `UPDATE`, `DELETE FROM` and
/// `COMMENT ON TABLE`, plus `REFERENCES` / `FROM` / `JOIN` targets inside them.
/// Everything else — `CREATE VIEW`, `CREATE SEQUENCE`, `DO`, dynamic SQL,
/// sub-queries, schema-qualified names — is a named refusal, so an object the
/// scanner cannot attribute can never pass as "nothing to check".
#[allow(clippy::too_many_lines)] // one rule per branch: the check reads as the rule list it implements
fn statement_events(tokens: &[Token], first: &str, shown: &str) -> ToolResult<Vec<SqlEvent>> {
    let mut events: Vec<SqlEvent> = Vec::new();
    match first {
        "create" => {
            let kind = word_after(
                tokens,
                index_of_keyword(tokens, "create"),
                &["or", "replace", "materialized"],
                shown,
            )?;
            match kind.as_str() {
                "table" => {
                    let at = index_of_keyword(tokens, "table").ok_or_else(|| {
                        refuse(
                            CHECK_CATALOG,
                            format!("{shown}: 'CREATE TABLE' has no name"),
                        )
                    })?;
                    let (name, index) =
                        object_after(tokens, at + 1, &["if", "not", "exists"], shown)?;
                    events.push(SqlEvent::Create(name));
                    events.extend(body_events(tokens, index, shown)?);
                }
                "index" => {
                    events.extend(index_events(tokens, shown)?);
                }
                "unique" => {
                    let at = index_of_keyword(tokens, "unique").ok_or_else(|| {
                        refuse(
                            CHECK_CATALOG,
                            format!("{shown}: 'CREATE UNIQUE' has no object"),
                        )
                    })?;
                    let next = word_after(tokens, Some(at), &["unique"], shown)?;
                    if next != "index" && next != "constraint" {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "{shown}: 'CREATE UNIQUE {next}' is outside the supported subset"
                            ),
                        ));
                    }
                    if next == "constraint" {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "{shown}: a table constraint is attributed through its own table \
                                 and is outside the supported subset of the owner scan"
                            ),
                        ));
                    }
                    events.extend(index_events(tokens, shown)?);
                }
                other => {
                    return Err(refuse(
                        CHECK_CATALOG,
                        format!("{shown}: 'CREATE {other}' is outside the supported SQL subset"),
                    ))
                }
            }
        }
        "alter" => {
            let at = index_of_keyword(tokens, "table").ok_or_else(|| {
                refuse(
                    CHECK_CATALOG,
                    format!("{shown}: only 'ALTER TABLE' is supported"),
                )
            })?;
            let (name, index) = object_after(tokens, at + 1, &["if", "exists", "only"], shown)?;
            events.push(SqlEvent::Reference(name));
            events.extend(body_events(tokens, index, shown)?);
        }
        "drop" => {
            let at = index_of_keyword(tokens, "table").ok_or_else(|| {
                refuse(
                    CHECK_CATALOG,
                    format!("{shown}: only 'DROP TABLE' is supported"),
                )
            })?;
            let listed = reference_list(tokens, at + 1, shown)?;
            assert_host_baseline_untouched("DROP TABLE", &listed, shown)?;
            events.extend(listed);
        }
        "truncate" => {
            let at = index_of_keyword(tokens, "truncate").ok_or_else(|| {
                refuse(CHECK_CATALOG, format!("{shown}: 'TRUNCATE' has no target"))
            })?;
            let listed = reference_list(tokens, at + 1, shown)?;
            assert_host_baseline_untouched("TRUNCATE", &listed, shown)?;
            events.extend(listed);
        }
        "insert" => {
            let at = index_of_keyword(tokens, "into").ok_or_else(|| {
                refuse(
                    CHECK_CATALOG,
                    format!("{shown}: only 'INSERT INTO' is supported"),
                )
            })?;
            let (name, index) = object_after(tokens, at + 1, &[], shown)?;
            events.push(SqlEvent::Reference(name));
            events.extend(body_events(tokens, index, shown)?);
        }
        "update" => {
            let at = index_of_keyword(tokens, "update")
                .ok_or_else(|| refuse(CHECK_CATALOG, format!("{shown}: 'UPDATE' has no target")))?;
            let (name, index) = object_after(tokens, at + 1, &["only"], shown)?;
            events.push(SqlEvent::Reference(name));
            events.extend(body_events(tokens, index, shown)?);
        }
        "delete" => {
            let at = index_of_keyword(tokens, "from").ok_or_else(|| {
                refuse(
                    CHECK_CATALOG,
                    format!("{shown}: only 'DELETE FROM' is supported"),
                )
            })?;
            let (name, index) = object_after(tokens, at + 1, &["only"], shown)?;
            events.push(SqlEvent::Reference(name));
            events.extend(body_events(tokens, index, shown)?);
        }
        "comment" => {
            let at = index_of_keyword(tokens, "table").ok_or_else(|| {
                refuse(
                    CHECK_CATALOG,
                    format!("{shown}: only 'COMMENT ON TABLE' is supported"),
                )
            })?;
            let (name, index) = object_after(tokens, at + 1, &["if", "exists"], shown)?;
            events.push(SqlEvent::Reference(name));
            events.extend(body_events(tokens, index, shown)?);
        }
        other => {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{shown}: a statement starting with '{other}' is outside the supported SQL \
                     subset of the owner scan"
                ),
            ))
        }
    }
    Ok(events)
}

fn index_of_keyword(tokens: &[Token], keyword: &str) -> Option<usize> {
    tokens.iter().position(|token| token.keyword() == keyword)
}

/// The first word after `from`, skipping `skip`, without treating it as an
/// object name (used for the object *kind* of a DDL statement).
fn word_after(
    tokens: &[Token],
    from: Option<usize>,
    skip: &[&str],
    shown: &str,
) -> ToolResult<String> {
    let mut index = from.map_or(0, |position| position + 1);
    while let Some(token) = tokens.get(index) {
        match token {
            Token::Word(word) if skip.contains(&word.as_str()) => index += 1,
            Token::Word(word) => return Ok(word.clone()),
            Token::Symbol(_) => {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!("{shown}: expected an object kind, found '{token:?}'"),
                ))
            }
        }
    }
    Err(refuse(
        CHECK_CATALOG,
        format!("{shown}: the statement ends before its object kind"),
    ))
}

/// `CREATE [UNIQUE] INDEX … ON <table> …`.
fn index_events(tokens: &[Token], shown: &str) -> ToolResult<Vec<SqlEvent>> {
    let on = index_of_keyword(tokens, "on").ok_or_else(|| {
        refuse(
            CHECK_CATALOG,
            format!("{shown}: an index without an 'ON' target is not supported"),
        )
    })?;
    let (name, index) = object_after(tokens, on + 1, &["only"], shown)?;
    let mut events = vec![SqlEvent::Reference(name)];
    events.extend(body_events(tokens, index, shown)?);
    Ok(events)
}

/// Comma separated object lists (`DROP TABLE a, b`).
fn reference_list(tokens: &[Token], from: usize, shown: &str) -> ToolResult<Vec<SqlEvent>> {
    let mut events = Vec::new();
    let mut index = from;
    loop {
        let (name, next) = object_after(tokens, index, &["if", "exists", "only", "table"], shown)?;
        events.push(SqlEvent::Reference(name));
        index = next;
        match tokens.get(index) {
            Some(Token::Symbol(symbol)) if symbol == "," => index += 1,
            _ => break,
        }
    }
    Ok(events)
}

/// A module revision destroys nothing the host baseline owns.
///
/// The owner rule of §11.2 #10 would otherwise let the owner of a host baseline
/// table drop it — `identity-management` may write `roles`, but dropping the
/// frozen table would silently delete a host baseline object that the pinned
/// history, the runner and every other module still assume exists. Data
/// statements stay allowed; the schema-level verbs do not.
fn assert_host_baseline_untouched(verb: &str, listed: &[SqlEvent], shown: &str) -> ToolResult<()> {
    for event in listed {
        if let SqlEvent::Reference(table) = event {
            if let Some(owner) = host_table_owner(table) {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!(
                        "{shown}: '{verb} {table}' is refused; '{table}' is a host baseline \
                         table owned by '{owner}' (design §3.1) and a module revision may not \
                         destroy it, even as its owner"
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// References inside a statement body: `REFERENCES`, `FROM`, `JOIN`.
fn body_events(tokens: &[Token], from: usize, shown: &str) -> ToolResult<Vec<SqlEvent>> {
    let mut events = Vec::new();
    let mut index = from;
    while index < tokens.len() {
        let keyword = tokens[index].keyword();
        // Three forms move or copy a whole table without naming it where the
        // owner scan can attribute it. They are refused by name instead of
        // guessed at, which is the fail-closed rule of review §3.5.
        if keyword == "rename"
            && matches!(tokens.get(index + 1), Some(Token::Word(next)) if next == "to")
        {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{shown}: 'RENAME TO' renames the whole table and is outside the supported \
                     SQL subset of the owner scan (a column rename 'RENAME COLUMN … TO …' is \
                     accepted)"
                ),
            ));
        }
        if keyword == "partition"
            && matches!(tokens.get(index + 1), Some(Token::Word(next)) if next == "of")
        {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{shown}: 'PARTITION OF' inherits a parent table and is outside the \
                     supported SQL subset of the owner scan"
                ),
            ));
        }
        if keyword == "like"
            && matches!(tokens.get(index + 1), Some(Token::Word(next)) if next != "''")
        {
            return Err(refuse(
                CHECK_CATALOG,
                format!(
                    "{shown}: 'LIKE <table>' copies another table's definition and is outside \
                     the supported SQL subset of the owner scan"
                ),
            ));
        }
        if matches!(keyword, "references" | "from" | "join") {
            if matches!(tokens.get(index + 1), Some(Token::Word(word)) if word == "select") {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!(
                        "{shown}: a sub-query behind '{keyword}' is outside the supported SQL \
                         subset of the owner scan"
                    ),
                ));
            }
            let (name, next) = object_after(tokens, index + 1, &["only", "lateral"], shown)?;
            events.push(SqlEvent::Reference(name));
            index = next;
            continue;
        }
        index += 1;
    }
    Ok(events)
}

/// Applies the owner rules of design §11.2 #10 to every revision of a catalog.
///
/// Returns the tables the namespace owns after its own catalog, and the
/// revisions annotated with what each of them creates.
fn scan_sql_ownership(
    path: &Path,
    catalog: &Catalog,
    revisions: &[Revision],
) -> ToolResult<(BTreeSet<String>, Vec<Revision>)> {
    let mut owned: BTreeSet<String> = HOST_BASELINE_TABLE_OWNERSHIP
        .iter()
        .filter(|(_, owner)| *owner == catalog.owner)
        .map(|(table, _)| (*table).to_owned())
        .collect();
    let mut annotated: Vec<Revision> = Vec::new();
    for revision in revisions {
        let shown = path.join(&revision.file_name).display().to_string();
        let text = String::from_utf8(read_regular_file(
            CHECK_CATALOG,
            &path.join(&revision.file_name),
            "SQL file",
        )?)
        .map_err(|_| {
            refuse(
                CHECK_CATALOG,
                format!("{shown}: a migration file must be valid UTF-8"),
            )
        })?;
        let (declarations, events) = analyse_sql(&text, &shown)?;
        let mut local: Vec<String> = Vec::new();
        for event in &events {
            match event {
                SqlEvent::Create(table) => {
                    if !declarations.iter().any(|declared| declared == table) {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "{shown}: creates '{table}' without an \
                                 '{OWNED_TABLE_DECLARATION} {table}' declaration",
                            ),
                        ));
                    }
                    if let Some(other) = host_table_owner(table) {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "{shown}: '{table}' is a host baseline table owned by '{other}'; \
                                 a module revision may not create another owner's table",
                            ),
                        ));
                    }
                    if table == REQUIRED_LEDGER_TABLE {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "{shown}: '{REQUIRED_LEDGER_TABLE}' is the platform ledger, owned \
                                 by the migration runner and never by a module",
                            ),
                        ));
                    }
                    owned.insert(table.clone());
                    local.push(table.clone());
                }
                SqlEvent::Reference(table) => {
                    if owned.contains(table) {
                        continue;
                    }
                    if table == REQUIRED_LEDGER_TABLE {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!("{shown}: references the platform ledger '{table}'"),
                        ));
                    }
                    if let Some(other) = host_table_owner(table) {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "{shown}: references '{table}', which the host baseline gives to \
                                 owner '{other}', not to '{}'",
                                catalog.owner
                            ),
                        ));
                    }
                    return Err(refuse(
                        CHECK_CATALOG,
                        format!(
                            "{shown}: references '{table}', which is neither owned by '{}' nor \
                             created by an earlier statement or revision of this catalog",
                            catalog.owner
                        ),
                    ));
                }
            }
        }
        annotated.push(Revision {
            file_name: revision.file_name.clone(),
            number: revision.number,
            created: local,
        });
    }
    Ok((owned, annotated))
}

/// The owner of a host baseline table, if the attribution table names one.
#[must_use]
pub fn host_table_owner(table: &str) -> Option<&'static str> {
    HOST_BASELINE_TABLE_OWNERSHIP
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, owner)| *owner)
}

// ---------------------------------------------------------------------------
// dependencies
// ---------------------------------------------------------------------------

/// One parsed `depends_on` entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dependency {
    /// The frozen host baseline, at a minimum revision.
    Baseline(u32),
    /// Another namespace, at a minimum revision count.
    Namespace(String, u32),
}

/// Parses one `depends_on` string.
///
/// The grammar is fixed here and in the fixtures, not guessed at call sites:
/// `platform-baseline@<revision>` or `<namespace>@<revision>`, where the
/// revision is a positive decimal without leading zeroes.
pub fn parse_dependency(entry: &str) -> ToolResult<Dependency> {
    let (name, revision) = entry.split_once('@').ok_or_else(|| {
        refuse(
            CHECK_CATALOG,
            format!(
                "dependency '{entry}' is not '<name>@<revision>'; the only accepted forms are \
                 '{BASELINE_DEPENDENCY_NAME}@<revision>' and '<namespace>@<revision>'"
            ),
        )
    })?;
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(refuse(
            CHECK_CATALOG,
            format!("dependency '{entry}' has an invalid dependency name"),
        ));
    }
    if revision.is_empty() || !revision.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(refuse(
            CHECK_CATALOG,
            format!("dependency '{entry}' does not name a numeric revision"),
        ));
    }
    if revision.len() > 1 && revision.starts_with('0') {
        return Err(refuse(
            CHECK_CATALOG,
            format!("dependency '{entry}' must not pad its revision with zeroes"),
        ));
    }
    let number = revision.parse::<u32>().map_err(|_| {
        refuse(
            CHECK_CATALOG,
            format!("dependency '{entry}' has an unusable revision"),
        )
    })?;
    if number == 0 {
        return Err(refuse(
            CHECK_CATALOG,
            format!("dependency '{entry}' must require a positive revision"),
        ));
    }
    Ok(if name == BASELINE_DEPENDENCY_NAME {
        Dependency::Baseline(number)
    } else {
        validate_namespace_name(name)?;
        Dependency::Namespace(name.to_owned(), number)
    })
}

/// Resolves the dependency graph of all catalogs against the pinned baseline.
#[allow(clippy::too_many_lines)] // one rule per branch: the check reads as the rule list it implements
pub fn resolve_dependencies(
    namespaces: &[Namespace],
    baseline_revision: u32,
) -> ToolResult<Vec<String>> {
    let known: BTreeSet<&str> = namespaces
        .iter()
        .map(|namespace| namespace.catalog.namespace.as_str())
        .collect();
    let available: BTreeMap<&str, usize> = namespaces
        .iter()
        .map(|namespace| {
            (
                namespace.catalog.namespace.as_str(),
                namespace.revisions.len(),
            )
        })
        .collect();
    let mut edges: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for namespace in namespaces {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut edges_for: BTreeSet<String> = BTreeSet::new();
        for entry in &namespace.catalog.depends_on {
            let dependency = parse_dependency(entry)?;
            let target = match &dependency {
                Dependency::Baseline(_) => BASELINE_DEPENDENCY_NAME.to_owned(),
                Dependency::Namespace(name, _) => name.clone(),
            };
            // Two entries for the same target are a duplicate even when they
            // name different revisions: the catalog would then state two
            // incompatible requirements for one dependency.
            if !seen.insert(target.clone()) {
                return Err(refuse(
                    CHECK_CATALOG,
                    format!(
                        "namespace '{}' lists dependency '{entry}' more than once",
                        namespace.catalog.namespace
                    ),
                ));
            }
            match dependency {
                Dependency::Baseline(number) => {
                    if number > baseline_revision {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "namespace '{}' requires host baseline revision {number} but the \
                                 pinned baseline provides {baseline_revision}",
                                namespace.catalog.namespace
                            ),
                        ));
                    }
                }
                Dependency::Namespace(name, number) => {
                    if name == namespace.catalog.namespace {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "namespace '{}' depends on itself",
                                namespace.catalog.namespace
                            ),
                        ));
                    }
                    if !known.contains(name.as_str()) {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "namespace '{}' depends on unknown namespace '{name}' \
                                 (dependency '{entry}')",
                                namespace.catalog.namespace
                            ),
                        ));
                    }
                    // §11.2 #7: a required revision must exist in the catalog
                    // that is checked out here, exactly like the baseline rule.
                    let count = available[name.as_str()];
                    if count == 0 || usize::try_from(number).unwrap_or(usize::MAX) > count {
                        return Err(refuse(
                            CHECK_CATALOG,
                            format!(
                                "namespace '{}' requires revision {number} of '{name}' but that \
                                 catalog {} here",
                                namespace.catalog.namespace,
                                if count == 0 {
                                    "declares no revision".to_owned()
                                } else {
                                    format!("only declares revisions 001..{count:03}")
                                }
                            ),
                        ));
                    }
                    edges_for.insert(name);
                }
            }
        }
        edges.insert(namespace.catalog.namespace.as_str(), edges_for);
    }
    // Kahn's algorithm over the sorted node set: a leftover node is a cycle.
    let mut resolved: Vec<&str> = Vec::new();
    let mut remaining: BTreeMap<&str, BTreeSet<String>> = edges.clone();
    while !remaining.is_empty() {
        let ready: Vec<&str> = remaining
            .iter()
            .filter(|(_, needs)| needs.iter().all(|need| resolved.contains(&need.as_str())))
            .map(|(node, _)| *node)
            .collect();
        if ready.is_empty() {
            let mut stuck: Vec<&str> = remaining.keys().copied().collect();
            stuck.sort_unstable();
            return Err(refuse(
                CHECK_CATALOG,
                format!("namespace dependencies form a cycle among {stuck:?}"),
            ));
        }
        for node in ready {
            // Every edge of `node` points at an already resolved namespace by
            // the filter above, so removing the node is all the bookkeeping
            // Kahn's algorithm needs here.
            remaining.remove(node);
            resolved.push(node);
        }
    }
    Ok(resolved.iter().map(|node| (*node).to_owned()).collect())
}

// ---------------------------------------------------------------------------
// pinned history
// ---------------------------------------------------------------------------

/// What the history check observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryReport {
    /// The pinned commit the digests come from.
    pub pinned_revision: String,
    /// The commit `HEAD` resolves to.
    pub head_revision: String,
    /// Baseline SQL revisions the pinned table pins.
    pub baseline_revision: u32,
    /// Blob entries compared byte-for-byte (`HEAD` and index each included).
    pub blob_entries_verified: usize,
    /// Working-tree entries compared after LF folding.
    pub working_tree_entries_verified: usize,
    /// Historical manifest entries whose recorded digest is *not* the canonical
    /// LF form — the seven CRLF entries review §2 C1 requires to stay green.
    pub legacy_manifest_form_entries: usize,
    /// Paths that were checked, for the log.
    pub paths: Vec<String>,
}

/// Verifies the frozen baseline against the pinned table (review §3.6).
#[allow(clippy::too_many_lines)] // one rule per branch: the check reads as the rule list it implements
pub fn verify_history(root: &Path, table: &PinnedBaselineHistory) -> ToolResult<HistoryReport> {
    let git = Git::open(root)?;
    // The work-tree half of this check reads files below `migrations/`, so the
    // same containment rule as the catalog check applies: no link, and the
    // directory must really resolve inside this checkout.
    let canonical_root = canonical_root(root, CHECK_HISTORY)?;
    let baseline_directory = root.join(BASELINE_DIRECTORY);
    read_regular_directory(
        CHECK_HISTORY,
        &baseline_directory,
        "host baseline directory",
    )?;
    assert_canonical_within(
        &canonical_root,
        &baseline_directory,
        CHECK_HISTORY,
        "host baseline directory",
    )?;
    if table.pinned_revision != PINNED_BASELINE_REVISION {
        return Err(refuse(
            CHECK_HISTORY,
            "the pinned history table does not name the pinned revision",
        ));
    }
    if !git.has_commit(&table.pinned_revision)? {
        return Err(refuse(
            CHECK_HISTORY,
            format!(
                "pinned commit {} is not present in this repository; the historical baseline \
                 cannot be verified and the check does not fall back to HEAD or to any other \
                 revision",
                table.pinned_revision
            ),
        ));
    }
    let head = git.head()?;
    let pinned_manifest = pinned_entry(table, "migrations/MANIFEST.sha256")?;
    let historical = historical_manifest(&git, table)?;

    let mut paths = Vec::new();
    let mut blob_entries_verified = 0_usize;
    let mut working_tree_entries_verified = 0_usize;
    let mut legacy_manifest_form_entries = 0_usize;
    for entry in &table.entries {
        let shown = format!("{}:{}", table.pinned_revision, entry.path);
        let pinned = git
            .blob(&table.pinned_revision, &entry.path)?
            .ok_or_else(|| {
                refuse(
                    CHECK_HISTORY,
                    format!("pinned commit does not contain '{}'", entry.path),
                )
            })?;
        if sha256_hex(&pinned) != entry.blob_sha256 {
            return Err(refuse(
                CHECK_HISTORY,
                format!(
                    "'{shown}' hashes to '{}' but the pinned history table records '{}'; the \
                     table and the fixed commit disagree",
                    sha256_hex(&pinned),
                    entry.blob_sha256
                ),
            ));
        }
        if entry.kind == "baseline-sql" {
            // §3.6 rule 4: the historical manifest keeps the raw/LF/CRLF
            // compatibility the repository already relies on. It is checked
            // against the *pinned* bytes and the file itself is pinned by blob
            // digest, so the compatibility cannot hide a content change.
            let name = path_basename(&entry.path).ok_or_else(|| {
                refuse(
                    CHECK_HISTORY,
                    format!("pinned entry '{}' is not a basename path", entry.path),
                )
            })?;
            let recorded = historical.get(name).ok_or_else(|| {
                refuse(
                    CHECK_HISTORY,
                    format!(
                        "the pinned '{}' does not list '{}'",
                        pinned_manifest.path, entry.path
                    ),
                )
            })?;
            if !manifest_digest_accepts(&pinned, recorded) {
                return Err(refuse(
                    CHECK_HISTORY,
                    format!(
                        "the historical manifest records '{recorded}' for '{}', which is neither \
                         the raw, the canonical LF nor the CRLF digest of the pinned blob",
                        entry.path
                    ),
                ));
            }
            if *recorded != sha256_hex(&fold_line_endings(&pinned)) {
                legacy_manifest_form_entries += 1;
            }
            if let Some(expected) = &entry.historical_manifest_digest {
                if expected != recorded {
                    return Err(refuse(
                        CHECK_HISTORY,
                        format!(
                            "the pinned history table records '{expected}' for '{}' but the \
                             pinned manifest records '{recorded}'",
                            entry.path
                        ),
                    ));
                }
            }
        }

        let head_blob = git.blob(&head, &entry.path)?.ok_or_else(|| {
            refuse(
                CHECK_HISTORY,
                format!(
                    "'HEAD:{}' is absent while the pinned commit contains it; a historical \
                     baseline file has been deleted",
                    entry.path
                ),
            )
        })?;
        if head_blob != pinned {
            return Err(refuse(
                CHECK_HISTORY,
                format!(
                    "'HEAD:{}' hashes to '{}' but the pinned commit stores '{}'; a historical \
                     baseline file has been rewritten",
                    entry.path,
                    sha256_hex(&head_blob),
                    entry.blob_sha256
                ),
            ));
        }
        let index_blob = git.blob("", &entry.path)?;
        match index_blob {
            Some(index_blob) if index_blob == pinned => {}
            Some(index_blob) => {
                return Err(refuse(
                    CHECK_HISTORY,
                    format!(
                        "the index stores '{}' for '{}' but the pinned commit stores '{}'; a \
                         historical baseline file has been staged",
                        sha256_hex(&index_blob),
                        entry.path,
                        entry.blob_sha256
                    ),
                ));
            }
            None => {
                return Err(refuse(
                    CHECK_HISTORY,
                    format!(
                        "the index does not contain '{}'; a historical baseline file has been \
                         removed from the index",
                        entry.path
                    ),
                ));
            }
        }
        blob_entries_verified += 2;

        // Working tree: line endings may differ (a `core.autocrlf` checkout
        // does), nothing else may, so both sides are folded before comparing.
        let disk = root.join(entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !disk.exists() {
            return Err(refuse(
                CHECK_HISTORY,
                format!(
                    "the working tree no longer contains '{}'; a historical baseline file has been deleted",
                    disk.display()
                ),
            ));
        }
        let bytes = read_regular_file(CHECK_HISTORY, &disk, "historical baseline file")?;
        if fold_line_endings(&bytes) != fold_line_endings(&pinned) {
            return Err(refuse(
                CHECK_HISTORY,
                format!(
                    "the working tree file '{}' differs from the pinned baseline beyond line \
                     endings (working tree '{}', pinned '{}')",
                    disk.display(),
                    sha256_hex(&fold_line_endings(&bytes)),
                    sha256_hex(&fold_line_endings(&pinned))
                ),
            ));
        }
        working_tree_entries_verified += 1;
        paths.push(entry.path.clone());
    }
    Ok(HistoryReport {
        pinned_revision: table.pinned_revision.clone(),
        head_revision: head,
        baseline_revision: table.baseline_revision,
        blob_entries_verified,
        working_tree_entries_verified,
        legacy_manifest_form_entries,
        paths,
    })
}

fn pinned_entry<'a>(table: &'a PinnedBaselineHistory, path: &str) -> ToolResult<&'a PinnedEntry> {
    table
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .ok_or_else(|| {
            refuse(
                CHECK_HISTORY,
                format!("pinned history table lacks '{path}'"),
            )
        })
}

/// Reads the *pinned* historical manifest into its recorded entries.
fn historical_manifest(
    git: &Git,
    table: &PinnedBaselineHistory,
) -> ToolResult<BTreeMap<String, String>> {
    let pinned = table
        .entries
        .iter()
        .find(|entry| entry.kind == "baseline-manifest")
        .ok_or_else(|| refuse(CHECK_HISTORY, "pinned history table pins no manifest"))?;
    let bytes = git
        .blob(&table.pinned_revision, &pinned.path)?
        .ok_or_else(|| {
            refuse(
                CHECK_HISTORY,
                format!("pinned commit does not contain '{}'", pinned.path),
            )
        })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        refuse(
            CHECK_HISTORY,
            format!("the pinned '{}' is not valid UTF-8", pinned.path),
        )
    })?;
    let mut entries: BTreeMap<String, String> = BTreeMap::new();
    for line in split_lf(&text) {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            return Err(refuse(
                CHECK_HISTORY,
                format!("the pinned '{}' has a blank line", pinned.path),
            ));
        }
        let Some((digest, name)) = line.split_once("  ") else {
            return Err(refuse(
                CHECK_HISTORY,
                format!(
                    "the pinned '{}' has an unparsable line '{line}'",
                    pinned.path
                ),
            ));
        };
        validate_sha256_hex(digest, "historical manifest digest")
            .map_err(|error| refuse(CHECK_HISTORY, error.to_string()))?;
        if entries.insert(name.to_owned(), digest.to_owned()).is_some() {
            return Err(refuse(
                CHECK_HISTORY,
                format!("the pinned '{}' lists '{name}' twice", pinned.path),
            ));
        }
    }
    let expected: BTreeSet<&str> = table
        .sql_entries()
        .iter()
        .filter_map(|entry| path_basename(&entry.path))
        .collect();
    let recorded: BTreeSet<&str> = entries.keys().map(String::as_str).collect();
    if expected != recorded {
        return Err(refuse(
            CHECK_HISTORY,
            format!(
                "the pinned '{}' lists {:?} but the pinned table pins {:?}",
                pinned.path, recorded, expected
            ),
        ));
    }
    Ok(entries)
}

/// The historical raw/LF/CRLF compatibility of review §3.6 rule 4.
///
/// This is the *only* place a non-canonical digest is accepted, it applies to
/// the historical manifest alone, and every file it covers is separately pinned
/// by blob digest, so it can never soften a content change into a pass.
#[must_use]
pub fn manifest_digest_accepts(blob: &[u8], recorded: &str) -> bool {
    let folded = fold_line_endings(blob);
    let raw = sha256_hex(blob);
    let lf = sha256_hex(&folded);
    let crlf = sha256_hex(&to_crlf(&folded));
    recorded == raw || recorded == lf || recorded == crlf
}

/// Converts canonical LF bytes into CRLF bytes.
#[must_use]
pub fn to_crlf(bytes: &[u8]) -> Vec<u8> {
    let mut crlf = Vec::with_capacity(bytes.len());
    for byte in bytes {
        if *byte == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(*byte);
    }
    crlf
}

// ---------------------------------------------------------------------------
// explicit candidate freeze
// ---------------------------------------------------------------------------

/// What the freeze check observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezeReport {
    /// Top-level baseline SQL file names, sorted.
    pub baseline_sql: Vec<String>,
    /// Subdirectories allowed inside the baseline directory.
    pub subdirectories: Vec<String>,
}

/// Runs the explicit candidate freeze (review §3.6 rule 5, design §11.3 #1).
#[allow(clippy::too_many_lines)] // one rule per branch: the check reads as the rule list it implements
pub fn verify_candidate_freeze(
    root: &Path,
    table: &PinnedBaselineHistory,
) -> ToolResult<FreezeReport> {
    let canonical_root = canonical_root(root, CHECK_FREEZE)?;
    let baseline = root.join(BASELINE_DIRECTORY);
    read_regular_directory(CHECK_FREEZE, &baseline, "host baseline directory")?;
    assert_canonical_within(
        &canonical_root,
        &baseline,
        CHECK_FREEZE,
        "host baseline directory",
    )?;
    let read = std::fs::read_dir(&baseline).map_err(|source| ToolError::Io {
        path: baseline.display().to_string(),
        source,
    })?;
    let mut sql: Vec<String> = Vec::new();
    let mut subdirectories: Vec<String> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|source| ToolError::Io {
            path: baseline.display().to_string(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        validate_name(&name, "baseline entry")?;
        let path = entry.path();
        assert_canonical_within(&canonical_root, &path, CHECK_FREEZE, "host baseline entry")?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if is_reparse_or_link(&metadata) {
            return Err(refuse(
                CHECK_FREEZE,
                format!("'{}' is a symbolic link or reparse point", path.display()),
            ));
        }
        if metadata.is_dir() {
            subdirectories.push(name);
            continue;
        }
        if is_sql_file_name(&name) {
            sql.push(name);
            continue;
        }
        // A freeze that only looked at `*.sql` would let a stray file slip into
        // the baseline directory, so anything the pinned candidate does not hold
        // is refused here as well.
        if table
            .entries
            .iter()
            .filter(|pinned| pinned.kind != "baseline-sql")
            .filter_map(|pinned| path_basename(&pinned.path))
            .any(|pinned| pinned == name)
        {
            continue;
        }
        return Err(refuse(
            CHECK_FREEZE,
            format!(
                "'{BASELINE_DIRECTORY}/{name}' is not part of the pinned baseline directory, \
                 which holds exactly the {} pinned SQL files, the pinned manifest and \
                 '{MODULES_DIRECTORY}'",
                table.sql_entries().len()
            ),
        ));
    }
    sql.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    subdirectories.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let expected: Vec<String> = table
        .sql_entries()
        .iter()
        .filter_map(|entry| path_basename(&entry.path))
        .map(str::to_owned)
        .collect();
    if sql != expected {
        let mut differences: Vec<String> = Vec::new();
        for name in expected.iter().filter(|name| !sql.contains(name)) {
            differences.push(format!("'{name}' is pinned but missing"));
        }
        for name in sql
            .iter()
            .filter(|name| !expected.iter().any(|pinned| pinned == *name))
        {
            differences.push(format!("'{name}' is not part of the pinned baseline"));
        }
        return Err(refuse(
            CHECK_FREEZE,
            format!(
                "the flat baseline directory must hold exactly the {} pinned SQL files: {}",
                expected.len(),
                differences.join("; ")
            ),
        ));
    }
    for directory in &subdirectories {
        if directory != "modules" {
            return Err(refuse(
                CHECK_FREEZE,
                format!(
                    "'{BASELINE_DIRECTORY}/{directory}' mixes a subdirectory into the flat \
                     baseline; only '{MODULES_DIRECTORY}' is allowed there"
                ),
            ));
        }
    }
    Ok(FreezeReport {
        baseline_sql: sql,
        subdirectories,
    })
}

// ---------------------------------------------------------------------------
// the single baseline embed point (review §3.7)
// ---------------------------------------------------------------------------

/// Check identifier of the single-baseline-embed scan.
pub const CHECK_EMBED: &str = "baseline-embed";
/// The only file allowed to embed the host baseline catalog.
pub const BASELINE_EMBED_FILE: &str = "crates/runtime-migration/src/lib.rs";
/// Source trees the embed scan walks (review §3.7: the scan is not limited to
/// `apps/migration/src`, which is where the old fitness line stopped).
pub const EMBED_SCAN_ROOTS: [&str; 2] = ["apps", "crates"];

/// One `sqlx::migrate!` call site the scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedSite {
    /// Repository-relative POSIX path of the file.
    pub file: String,
    /// 1-based line of the call.
    pub line: usize,
    /// The literal path argument exactly as written.
    pub argument: String,
    /// Where that argument resolves, repository-relative.
    pub resolves_to: String,
    /// `baseline` for a call that embeds the host catalog, `crate` for a call
    /// that embeds a migration directory owned by the crate itself.
    pub kind: &'static str,
}

/// What the embed scan observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedReport {
    /// Rust source files scanned.
    pub scanned_files: usize,
    /// Every `sqlx::migrate!` call site, in path order.
    pub sites: Vec<EmbedSite>,
}

impl EmbedReport {
    /// The call sites that embed the host baseline catalog.
    #[must_use]
    pub fn baseline_sites(&self) -> Vec<&EmbedSite> {
        self.sites
            .iter()
            .filter(|site| site.kind == "baseline")
            .collect()
    }

    /// The call sites that embed a crate-owned migration directory.
    #[must_use]
    pub fn crate_sites(&self) -> Vec<&EmbedSite> {
        self.sites
            .iter()
            .filter(|site| site.kind == "crate")
            .collect()
    }
}

/// Scans `apps/` and `crates/` for embedded migration catalogs (review §3.7).
///
/// The rule is deliberately narrow: the host baseline may be embedded in exactly
/// one place, `crates/runtime-migration/src/lib.rs`. A crate may still embed its
/// *own* migration directory (the `SQLite` adapters do), which is a different
/// catalog. Anything the scan cannot resolve — a macro argument that is not a
/// plain literal, a path that does not exist, a spelling that is not
/// `sqlx::migrate!` — is refused by name instead of being guessed around.
pub fn verify_embed_point(root: &Path) -> ToolResult<EmbedReport> {
    let canonical_root = canonical_root(root, CHECK_EMBED)?;
    let baseline = canonicalize_or_refuse(
        &canonical_root.join(BASELINE_DIRECTORY),
        &canonical_root,
        "host baseline migration directory",
    )?;
    let mut files: Vec<PathBuf> = Vec::new();
    for directory in EMBED_SCAN_ROOTS {
        let top = root.join(directory);
        if !top.exists() {
            continue;
        }
        read_regular_directory(CHECK_EMBED, &top, "scanned source root")?;
        collect_rust_files(&canonical_root, &top, &mut files)?;
    }
    files.sort_by(|left, right| {
        left.as_os_str()
            .to_string_lossy()
            .as_bytes()
            .cmp(right.as_os_str().to_string_lossy().as_bytes())
    });
    let mut sites: Vec<EmbedSite> = Vec::new();
    for file in &files {
        let shown = relative_posix(&canonical_root, file, CHECK_EMBED)?;
        let bytes = read_regular_file(CHECK_EMBED, file, "Rust source file")?;
        let text = String::from_utf8(bytes).map_err(|_| {
            refuse(
                CHECK_EMBED,
                format!("'{shown}' is not UTF-8; the embed scan cannot read it"),
            )
        })?;
        for (line, argument) in migrate_macro_calls(&shown, &text)? {
            let (resolves_to, kind) =
                resolve_embed_argument(&canonical_root, &baseline, file, &shown, line, &argument)?;
            sites.push(EmbedSite {
                file: shown.clone(),
                line,
                argument,
                resolves_to,
                kind,
            });
        }
    }
    let baseline_sites: Vec<&EmbedSite> = sites
        .iter()
        .filter(|site| site.kind == "baseline")
        .collect();
    if baseline_sites.is_empty() {
        return Err(refuse(
            CHECK_EMBED,
            format!(
                "no file embeds the host baseline catalog; the single embed point \
                 '{BASELINE_EMBED_FILE}' is missing, so the platform would silently lose its \
                 migrations"
            ),
        ));
    }
    if baseline_sites.len() > 1 {
        let where_seen: Vec<String> = baseline_sites
            .iter()
            .map(|site| format!("'{}':{}", site.file, site.line))
            .collect();
        return Err(refuse(
            CHECK_EMBED,
            format!(
                "{} call sites embed the host baseline catalog ({}); the only allowed baseline \
                 embed point is '{BASELINE_EMBED_FILE}'",
                baseline_sites.len(),
                where_seen.join(", ")
            ),
        ));
    }
    if baseline_sites[0].file != BASELINE_EMBED_FILE {
        let site = baseline_sites[0];
        return Err(refuse(
            CHECK_EMBED,
            format!(
                "'{}':{} embeds the host baseline catalog; the only allowed baseline embed point \
                 is '{BASELINE_EMBED_FILE}' (the crate's own migrator is the shared seam \
                 `runtime_migration::MIGRATOR`)",
                site.file, site.line
            ),
        ));
    }
    Ok(EmbedReport {
        scanned_files: files.len(),
        sites,
    })
}

/// Walks one source root, refusing links and collecting `*.rs` files.
fn collect_rust_files(
    canonical_root: &Path,
    directory: &Path,
    out: &mut Vec<PathBuf>,
) -> ToolResult<()> {
    assert_canonical_within(
        canonical_root,
        directory,
        CHECK_EMBED,
        "scanned source directory",
    )?;
    let read = std::fs::read_dir(directory).map_err(|source| ToolError::Io {
        path: directory.display().to_string(),
        source,
    })?;
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|source| ToolError::Io {
            path: directory.display().to_string(),
            source,
        })?;
        entries.push((
            entry.file_name().to_string_lossy().into_owned(),
            entry.path(),
        ));
    }
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    for (name, path) in entries {
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if is_reparse_or_link(&metadata) {
            return Err(refuse(
                CHECK_EMBED,
                format!("'{}' is a symbolic link or reparse point", path.display()),
            ));
        }
        if metadata.is_dir() {
            if matches!(name.as_str(), "target" | ".git" | "node_modules") {
                continue;
            }
            collect_rust_files(canonical_root, &path, out)?;
            continue;
        }
        if Path::new(&name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        {
            // The canonical form is what the scan reports, so a call site is
            // always named by a path inside the checkout under review.
            out.push(canonicalize_or_refuse(
                &path,
                canonical_root,
                "Rust source file",
            )?);
        }
    }
    Ok(())
}

/// Finds every `sqlx::migrate!` call in one Rust file.
///
/// Comments are removed first (they are where review P-4's explanatory text
/// legitimately quotes the macro), and every call must be spelled
/// `sqlx::migrate!` with exactly one plain string literal argument.
fn migrate_macro_calls(shown: &str, text: &str) -> ToolResult<Vec<(usize, String)>> {
    let code: Vec<char> = strip_rust_comments(text).chars().collect();
    let needle: Vec<char> = "migrate".chars().collect();
    let mut found: Vec<(usize, String)> = Vec::new();
    let mut start = 0_usize;
    while let Some(offset) = find_subslice(&code[start..], &needle).map(|index| index + start) {
        start = offset + needle.len();
        let before_identifiable = offset > 0 && is_ident_char(code[offset - 1]);
        let after_identifiable = code
            .get(start)
            .is_some_and(|&character| is_ident_char(character));
        if before_identifiable || after_identifiable {
            continue;
        }
        if !ends_with_before_ws(&code, offset, &["sqlx", "::"]) {
            continue;
        }
        let mut cursor = skip_ws(&code, start);
        if code.get(cursor) != Some(&'!') {
            continue;
        }
        cursor = skip_ws(&code, cursor + 1);
        if code.get(cursor) != Some(&'(') {
            return Err(cannot_decide(
                shown,
                offset,
                &code,
                "is not followed by '('",
            ));
        }
        cursor = skip_ws(&code, cursor + 1);
        if code.get(cursor) != Some(&'"') {
            return Err(cannot_decide(
                shown,
                offset,
                &code,
                "does not have a plain string literal as its only argument",
            ));
        }
        cursor += 1;
        let mut argument = String::new();
        let mut closed = false;
        while let Some(&character) = code.get(cursor) {
            cursor += 1;
            match character {
                '"' => {
                    closed = true;
                    break;
                }
                '\\' => {
                    if let Some(&escaped) = code.get(cursor) {
                        cursor += 1;
                        argument.push(escaped);
                    }
                }
                other => argument.push(other),
            }
        }
        if !closed {
            return Err(cannot_decide(
                shown,
                offset,
                &code,
                "has an unterminated string literal argument",
            ));
        }
        cursor = skip_ws(&code, cursor);
        if code.get(cursor) != Some(&')') {
            return Err(cannot_decide(
                shown,
                offset,
                &code,
                "has more than one argument, which the scan cannot attribute",
            ));
        }
        let line = code[..offset].iter().filter(|&&c| c == '\n').count() + 1;
        found.push((line, argument));
    }
    Ok(found)
}

/// Resolves one embed argument against the directory of its own crate.
fn resolve_embed_argument(
    canonical_root: &Path,
    baseline: &Path,
    file: &Path,
    shown: &str,
    line: usize,
    argument: &str,
) -> ToolResult<(String, &'static str)> {
    let at = format!("'{shown}:{line}'");
    if argument.trim().is_empty() {
        return Err(refuse(
            CHECK_EMBED,
            format!("the embed at {at} has an empty migration directory argument"),
        ));
    }
    let written = Path::new(argument);
    if written.is_absolute() || argument.contains(':') || argument.starts_with('\\') {
        return Err(refuse(
            CHECK_EMBED,
            format!(
                "the embed at {at} names '{argument}'; an absolute migration path cannot be \
                 reviewed against the checkout, so the scan refuses to decide it"
            ),
        ));
    }
    let crate_directory = owning_crate(canonical_root, file, &at)?;
    let joined = crate_directory.join(written);
    let resolved = canonicalize_or_refuse(&joined, canonical_root, &format!("the embed at {at}"))?;
    if resolved == baseline {
        // Which file it is, and whether that file is the one allowed to do it,
        // is decided once in `verify_embed_point`, so every scanner in this run
        // agrees on a single answer.
        return Ok((BASELINE_DIRECTORY.to_owned(), "baseline"));
    }
    let crate_canonical = canonicalize_or_refuse(
        &crate_directory,
        canonical_root,
        &format!("the crate directory of {at}"),
    )?;
    if !resolved.starts_with(&crate_canonical) {
        return Err(refuse(
            CHECK_EMBED,
            format!(
                "the embed at {at} names '{argument}', which resolves to '{}' and leaves the \
                 owning crate '{}'; only the host baseline and a crate's own migration \
                 directory may be embedded",
                resolved.display(),
                crate_canonical.display()
            ),
        ));
    }
    Ok((
        relative_posix(canonical_root, &resolved, CHECK_EMBED)?,
        "crate",
    ))
}

/// Nearest ancestor directory (up to the checkout root) that holds a
/// `Cargo.toml`, which is what makes a relative embed path decidable.
fn owning_crate(canonical_root: &Path, file: &Path, at: &str) -> ToolResult<PathBuf> {
    let canonical = std::fs::canonicalize(file).map_err(|source| ToolError::Io {
        path: file.display().to_string(),
        source,
    })?;
    let mut cursor = canonical.parent().map(PathBuf::from);
    while let Some(directory) = cursor {
        if directory.join("Cargo.toml").is_file() {
            return Ok(directory);
        }
        if directory == *canonical_root {
            break;
        }
        cursor = directory.parent().map(PathBuf::from);
    }
    Err(refuse(
        CHECK_EMBED,
        format!(
            "the embed at {at} is in '{}', for which no ancestor up to the checkout \
             root holds a 'Cargo.toml'; the scan cannot decide where a relative migration path \
             resolves",
            canonical.display()
        ),
    ))
}

/// Canonicalises a path the scan needs, refusing instead of guessing when it
/// does not exist.
fn canonicalize_or_refuse(path: &Path, canonical_root: &Path, what: &str) -> ToolResult<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|_| {
        refuse(
            CHECK_EMBED,
            format!(
                "{what} resolves to '{}' inside '{}', which does not exist; the scan refuses to \
                 decide a migration path that is not there",
                path.display(),
                canonical_root.display()
            ),
        )
    })?;
    Ok(canonical)
}

/// Repository-relative POSIX form of a path below the checkout root.
fn relative_posix(canonical_root: &Path, path: &Path, check: &str) -> ToolResult<String> {
    let relative = path.strip_prefix(canonical_root).map_err(|_| {
        refuse(
            check,
            format!(
                "'{}' is not inside the checked-out root '{}'",
                path.display(),
                canonical_root.display()
            ),
        )
    })?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/"))
}

/// True for the characters that continue an identifier. `:` is deliberately
/// excluded: `sqlx::migrate!` must not look like one long identifier.
fn is_ident_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn skip_ws(code: &[char], mut index: usize) -> usize {
    while code.get(index).is_some_and(|c| c.is_whitespace()) {
        index += 1;
    }
    index
}

/// True when `tokens`, read in order, end exactly at `index` with only
/// whitespace between them — the spelling check for `sqlx :: migrate`. The
/// token before the first one must not continue an identifier, so a hypothetical
/// `xsqlx::migrate!` is not mistaken for the real macro.
fn ends_with_before_ws(code: &[char], index: usize, tokens: &[&str]) -> bool {
    let mut cursor = index;
    for token in tokens.iter().rev() {
        while cursor > 0 && code[cursor - 1].is_whitespace() {
            cursor -= 1;
        }
        let chars: Vec<char> = token.chars().collect();
        if chars.len() > cursor || code[cursor - chars.len()..cursor] != chars[..] {
            return false;
        }
        cursor -= chars.len();
    }
    cursor == 0 || !is_ident_char(code[cursor - 1]) || code[cursor - 1] == ':'
}

fn find_subslice(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&start| &haystack[start..start + needle.len()] == needle)
}

/// The named "cannot decide" refusal of review §3.7.
fn cannot_decide(shown: &str, offset: usize, code: &[char], reason: &str) -> ToolError {
    let line = code[..offset].iter().filter(|&&c| c == '\n').count() + 1;
    refuse(
        CHECK_EMBED,
        format!("the 'migrate' macro in '{shown}:{line}' {reason}"),
    )
}

/// Removes `//` and block comments, keeping the byte positions of everything
/// else so line numbers stay truthful.
fn strip_rust_comments(text: &str) -> String {
    let code: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0_usize;
    while index < code.len() {
        let character = code[index];
        if character == '/' && code.get(index + 1) == Some(&'/') {
            while index < code.len() && code[index] != '\n' {
                out.push(' ');
                index += 1;
            }
            continue;
        }
        if character == '/' && code.get(index + 1) == Some(&'*') {
            let mut depth = 0_usize;
            while index < code.len() {
                if code[index] == '/' && code.get(index + 1) == Some(&'*') {
                    depth += 1;
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    continue;
                }
                if code[index] == '*' && code.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                out.push(if code[index] == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            continue;
        }
        if character == '"' {
            out.push(character);
            index += 1;
            while index < code.len() {
                out.push(code[index]);
                if code[index] == '\\' {
                    index += 1;
                    if index < code.len() {
                        out.push(code[index]);
                        index += 1;
                        continue;
                    }
                }
                if code[index] == '"' {
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        out.push(character);
        index += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// the report
// ---------------------------------------------------------------------------

/// The result of one offline validation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Checks that ran, in the order they ran.
    pub checks: Vec<&'static str>,
    /// Validated namespace catalogs, ordered by namespace.
    pub namespaces: Vec<Namespace>,
    /// Topological namespace order, when the catalog check ran.
    pub order: Vec<String>,
    /// History observation, when the history check ran.
    pub history: Option<HistoryReport>,
    /// Embed scan observation, when the embed check ran.
    pub embed: Option<EmbedReport>,
    /// Freeze observation, when the freeze check ran.
    pub freeze: Option<FreezeReport>,
}

impl Report {
    /// Deterministic log lines for the CLI and the gate.
    ///
    /// Prints the pinned revision, the blob/working-tree modes and the paths
    /// that were hit, and never a connection string or an environment value
    /// (review §3.6 rule 6).
    #[must_use]
    pub fn log_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("checks={}", self.checks.join(","))];
        for namespace in &self.namespaces {
            lines.push(format!(
                "namespace={} owner={} status={} revisions={} depends_on={}",
                namespace.catalog.namespace,
                namespace.catalog.owner,
                namespace.status(),
                namespace
                    .revisions
                    .iter()
                    .map(|revision| format!("{:03}", revision.number))
                    .collect::<Vec<_>>()
                    .join("+"),
                if namespace.catalog.depends_on.is_empty() {
                    "-".to_owned()
                } else {
                    namespace.catalog.depends_on.join("+")
                },
            ));
        }
        if !self.order.is_empty() {
            lines.push(format!("order={}", self.order.join(">")));
        }
        if let Some(history) = &self.history {
            lines.push(format!(
                "history[{}] mode=blob+index+working-tree",
                history.pinned_revision
            ));
            lines.push(format!(
                "history[{}] head={}",
                history.pinned_revision, history.head_revision
            ));
            lines.push(format!(
                "history[{}] baseline_revision={} blob_entries_verified={} \
                 working_tree_entries_verified={} legacy_manifest_form_entries={}",
                history.pinned_revision,
                history.baseline_revision,
                history.blob_entries_verified,
                history.working_tree_entries_verified,
                history.legacy_manifest_form_entries
            ));
            for path in &history.paths {
                lines.push(format!("history[{}] hit={}", history.pinned_revision, path));
            }
        }
        if let Some(embed) = &self.embed {
            lines.push(format!(
                "embed scanned_rs_files={} baseline_embed_points={} crate_relative_embeds={}",
                embed.scanned_files,
                embed.baseline_sites().len(),
                embed.crate_sites().len()
            ));
            for site in &embed.sites {
                lines.push(format!(
                    "embed site={} argument='{}' resolves_to={} kind={}",
                    site.file, site.argument, site.resolves_to, site.kind
                ));
            }
        }
        if let Some(freeze) = &self.freeze {
            lines.push(format!(
                "freeze baseline_sql_files={} subdirectories={}",
                freeze.baseline_sql.len(),
                if freeze.subdirectories.is_empty() {
                    "-".to_owned()
                } else {
                    freeze.subdirectories.join("+")
                }
            ));
        }
        lines
    }
}

/// Runs the requested offline checks over `root`.
///
/// Checks always run in the fixed order catalog → history → embed → freeze so
/// the log and the first reported failure are reproducible, whatever order the
/// caller passed them in.
pub fn validate(root: &Path, checks: &[Check]) -> ToolResult<Report> {
    let mut active: Vec<Check> = checks.to_vec();
    active.sort_unstable();
    active.dedup();
    if active.is_empty() {
        return Err(ToolError::Usage(
            "catalog-check needs at least one check".to_owned(),
        ));
    }
    let table = pinned_baseline_history()?;
    let mut namespaces = Vec::new();
    let mut order = Vec::new();
    let mut history = None;
    let mut embed = None;
    let mut freeze = None;
    let mut ran: Vec<&'static str> = Vec::new();
    for check in &active {
        match check {
            Check::Catalog => {
                namespaces = validate_catalogs(root)?;
                order = resolve_dependencies(&namespaces, table.baseline_revision)?;
                ran.push(check.name());
            }
            Check::History => {
                history = Some(verify_history(root, &table)?);
                ran.push(check.name());
            }
            Check::Embed => {
                embed = Some(verify_embed_point(root)?);
                ran.push(check.name());
            }
            Check::Freeze => {
                freeze = Some(verify_candidate_freeze(root, &table)?);
                ran.push(check.name());
            }
        }
    }
    Ok(Report {
        checks: ran,
        namespaces,
        order,
        history,
        embed,
        freeze,
    })
}
