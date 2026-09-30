#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Wave 5B-S1 offline catalog gate (design §11.1–§11.3, review §3.4–§3.7).
//!
//! This file is the second reader of the fixture tree under
//! `tests/fixtures/wave5b/`. `scripts/test-plan0014-wave5b.ps1` is the first, and
//! both read the same `EXPECTATIONS.json`, so a rule cannot be true for the
//! repository gate and false for `cargo test` (review §3.2: "Rust 及脚本共用的正负
//! fixture"). Everything here is offline: no database, no network, and the only
//! Git commands are read-only.
//!
//! Case groups:
//! * S-1  shared fixture tree, every case, both verdict direction and reason.
//! * S-2  the pinned history table versus the pinned commit (digest rules, C1).
//! * S-3  the 13 host tables: attribution table versus migration 019.
//! * S-4  the default check set and the repository's own state.
//! * S-5  the single baseline embed point.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use module_release_tool::canonical::{fold_line_endings, sha256_hex, validate_sha256_hex};
use module_release_tool::catalog::{
    self, BASELINE_DIRECTORY, BASELINE_EMBED_FILE, CHECK_CATALOG, CHECK_EMBED, CHECK_FREEZE,
    CHECK_HISTORY, HOST_BASELINE_TABLE_OWNERSHIP, MODULES_DIRECTORY, PINNED_BASELINE_HISTORY_JSON,
    PINNED_BASELINE_REVISION, STATUS_RESERVED, STATUS_VALIDATED,
};
use module_release_tool::git::Git;
use module_release_tool::tempdir::TempDir;

/// Repository root, derived from the crate directory of this test binary.
fn repository() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .expect("tools/<crate>/tests")
        .to_path_buf()
}

fn fixture_root() -> PathBuf {
    repository()
        .join("tools/module-release-tool/tests/fixtures/wave5b/catalogs")
        .canonicalize()
        .expect("the shared fixture tree is checked in")
}

/// One row of the shared expectation table.
#[derive(serde::Deserialize)]
struct Expectation {
    case: String,
    expect: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    contains: Option<String>,
}

#[derive(serde::Deserialize)]
struct Expectations {
    cases: Vec<Expectation>,
}

fn shared_expectations() -> Expectations {
    let path = fixture_root().join("EXPECTATIONS.json");
    let text = fs::read_to_string(&path).expect("EXPECTATIONS.json is checked in");
    serde_json::from_str(&text).expect("EXPECTATIONS.json parses")
}

/// Copies one fixture case into `<work>/migrations/modules/`.
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("create target");
    for entry in fs::read_dir(source)
        .expect("read source")
        .map(Result::unwrap)
    {
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &destination);
        } else {
            fs::copy(&path, &destination).expect("copy");
        }
    }
}

fn workspace_for(case: &str) -> TempDir {
    let work = TempDir::new(&format!("wave5b-{case}")).expect("temp dir");
    let modules = work.path().join(MODULES_DIRECTORY.replace('/', "\\"));
    copy_tree(&fixture_root().join(case), &modules);
    work
}

// ---------------------------------------------------------------------------
// S-1 — the shared fixture tree, read by this second reader as well
// ---------------------------------------------------------------------------

#[test]
fn every_shared_fixture_case_produces_the_recorded_verdict() {
    let expectations = shared_expectations();
    assert!(
        expectations.cases.len() >= 30,
        "the shared fixture table must keep covering the rule list"
    );
    let mut problems: Vec<String> = Vec::new();
    for case in &expectations.cases {
        let work = workspace_for(&case.case);
        let outcome = catalog::validate(work.path(), &[catalog::Check::Catalog]);
        let detail = format!("fixture '{}' {}", case.case, case.expect);
        match (case.expect.as_str(), outcome) {
            ("pass", Ok(report)) => {
                let expected = case.status.as_deref().unwrap_or(STATUS_VALIDATED);
                let printed = report.log_lines().join("\n");
                assert!(
                    printed.contains(&format!("status={expected}")),
                    "{detail}: expected status {expected} in\n{printed}"
                );
            }
            ("pass", Err(error)) => {
                problems.push(format!("{detail}: unexpectedly refused: {error}"));
            }
            (_, Ok(report)) => {
                let statuses: Vec<&str> = report
                    .namespaces
                    .iter()
                    .map(catalog::Namespace::status)
                    .collect();
                problems.push(format!(
                    "{detail}: expected a refusal mentioning {:?}, but the catalog validated as {}",
                    case.contains,
                    statuses.join("+")
                ));
            }
            (_, Err(error)) => {
                let message = error.to_string();
                assert!(
                    message.contains(&format!("[{CHECK_CATALOG}]")),
                    "{}: the refusal must name the catalog check, got: {message}",
                    case.case
                );
                if let Some(needle) = &case.contains {
                    assert!(
                        message.contains(needle),
                        "{}: expected the refusal to mention {needle:?}, got: {message}",
                        case.case
                    );
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "fixture cases that did not behave as recorded:\n{}",
        problems.join("\n")
    );
}

#[test]
fn no_fixture_directory_is_left_uncovered() {
    let covered: Vec<String> = shared_expectations()
        .cases
        .into_iter()
        .map(|case| case.case)
        .collect();
    let mut orphans: Vec<String> = Vec::new();
    for entry in fs::read_dir(fixture_root())
        .expect("read fixtures")
        .map(Result::unwrap)
    {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !covered.contains(&name) {
            orphans.push(name);
        }
    }
    assert!(
        orphans.is_empty(),
        "fixture directories without an expectation row: {orphans:?}"
    );
}

// ---------------------------------------------------------------------------
// S-2 — the pinned history table versus the pinned commit (C1)
// ---------------------------------------------------------------------------

#[test]
fn the_embedded_pinned_table_is_the_checked_in_fixture() {
    let path = repository().join(
        "tools/module-release-tool/tests/fixtures/wave5b/history/\
         pinned-baseline-5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601.json",
    );
    let on_disk = fs::read_to_string(&path).expect("the pinned table is versioned");
    assert_eq!(
        on_disk.replace("\r\n", "\n"),
        PINNED_BASELINE_HISTORY_JSON,
        "the compiled-in pinned history table has drifted from the versioned fixture"
    );
}

#[test]
fn every_pinned_entry_is_the_blob_of_the_pinned_commit() {
    let table = catalog::pinned_baseline_history().expect("pinned table");
    assert_eq!(table.pinned_revision, PINNED_BASELINE_REVISION);
    assert_eq!(table.baseline_revision, 19);
    let git = Git::open(&repository()).expect("this test runs inside the repository");
    assert!(
        git.has_commit(PINNED_BASELINE_REVISION)
            .expect("git answered"),
        "the pinned commit must be reachable from this checkout"
    );
    let mut sql = 0_usize;
    let mut manifest = 0_usize;
    for entry in &table.entries {
        let blob = git
            .blob(PINNED_BASELINE_REVISION, &entry.path)
            .expect("git answers for the pinned blob");
        assert!(
            blob.is_some(),
            "the pinned commit does not contain {}",
            entry.path
        );
        let blob = blob.expect("checked above");
        assert_eq!(
            sha256_hex(&blob),
            entry.blob_sha256,
            "{} is not the pinned bytes",
            entry.path
        );
        match entry.kind.as_str() {
            "baseline-sql" => sql += 1,
            "baseline-manifest" => manifest += 1,
            other => {
                let known = ["baseline-sql", "baseline-manifest"];
                assert!(known.contains(&other), "unknown pinned entry kind {other}");
            }
        }
    }
    assert_eq!(
        (sql, manifest),
        (19, 1),
        "the pinned table covers 19 SQL + manifest"
    );
}

#[test]
fn the_seven_c1_entries_are_pinned_by_blob_even_though_the_manifest_is_crlf() {
    // Review §2 C1: seven historical entries match only the CRLF digest. The
    // historical manifest keeps that compatibility, but each file is *also*
    // pinned by its blob digest, so compatibility cannot hide a content change.
    let table = catalog::pinned_baseline_history().expect("pinned table");
    let git = Git::open(&repository()).expect("git");
    let mut accepted = 0_usize;
    let mut crlf_only = 0_usize;
    let mut lf_only = 0_usize;
    for entry in table.sql_entries() {
        let blob = git
            .blob(PINNED_BASELINE_REVISION, &entry.path)
            .expect("git")
            .expect("the pinned commit contains the file");
        let recorded = entry.historical_manifest_digest.as_deref();
        assert!(
            recorded.is_some(),
            "{} has no historical manifest digest",
            entry.path
        );
        let recorded = recorded.expect("checked above");
        assert!(
            catalog::manifest_digest_accepts(&blob, recorded),
            "{}: the historical manifest digest {} matches neither form",
            entry.path,
            recorded
        );
        let raw = recorded == sha256_hex(&blob);
        let lf = recorded == sha256_hex(&fold_line_endings(&blob));
        let crlf = recorded == sha256_hex(&catalog::to_crlf(&fold_line_endings(&blob)));
        if crlf && !raw && !lf {
            crlf_only += 1;
        } else if raw || lf {
            lf_only += 1;
        } else {
            unreachable!("accepted above");
        }
        accepted += 1;
    }
    assert_eq!(accepted, 19);
    assert_eq!(
        crlf_only, 7,
        "review §2 C1 measured exactly seven CRLF entries"
    );
    assert_eq!(lf_only, 12);
}

#[test]
fn a_git_object_id_is_never_a_content_digest() {
    // The failure mode C1 warns about is recording `git show <sha>:<path>` (a
    // 40-hex blob OID) where a SHA-256 content digest belongs.
    let table = catalog::pinned_baseline_history().expect("pinned table");
    let git = Git::open(&repository()).expect("git");
    let entry = table
        .entries
        .iter()
        .find(|entry| entry.path.ends_with("001_initial.sql"))
        .expect("001 is pinned");
    let entries = git
        .tree(PINNED_BASELINE_REVISION, "migrations")
        .expect("git")
        .into_iter()
        .filter(|item| item.kind == "blob")
        .collect::<Vec<_>>();
    let listed = entries.iter().find(|item| item.path == entry.path);
    assert!(
        listed.is_some(),
        "{} is not listed in the pinned tree",
        entry.path
    );
    let oid = listed.expect("checked above").object.clone();
    assert_eq!(oid.len(), 40, "a Git blob OID is 40 hex characters");
    assert_ne!(oid, entry.blob_sha256);
    assert_eq!(entry.blob_sha256.len(), 64);
    validate_sha256_hex(&entry.blob_sha256, "pinned blob digest").expect("64 lowercase hex");
    validate_sha256_hex(&oid, "a git blob OID").expect_err("a 40-hex OID is not a SHA-256");
}

#[test]
fn head_and_index_still_hold_the_pinned_baseline() {
    // The fixed-history proof at the current HEAD: pinned bytes == HEAD bytes ==
    // index bytes, and the work tree may only differ by line endings.
    let table = catalog::pinned_baseline_history().expect("pinned table");
    let root = repository();
    let git = Git::open(&root).expect("git");
    let head = git.head().expect("HEAD");
    for entry in &table.entries {
        let pinned = git
            .blob(PINNED_BASELINE_REVISION, &entry.path)
            .expect("git")
            .expect("pinned path");
        let at_head = git
            .blob(&head, &entry.path)
            .expect("git")
            .expect("HEAD path");
        assert_eq!(sha256_hex(&at_head), entry.blob_sha256, "{entry:?}");
        let staged = git.blob("", &entry.path).expect("git");
        assert!(staged.is_some(), "{} is not in the index", entry.path);
        let in_index = staged.expect("checked above");
        assert_eq!(sha256_hex(&in_index), entry.blob_sha256, "{}", entry.path);
        let on_disk = fs::read(root.join(entry.path.replace('/', "\\"))).expect("work tree");
        assert_eq!(
            sha256_hex(&fold_line_endings(&on_disk)),
            sha256_hex(&fold_line_endings(&pinned)),
            "{} differs from the pinned baseline beyond line endings",
            entry.path
        );
    }
}

#[test]
fn a_repository_without_the_pinned_commit_is_refused_without_a_fallback() {
    let work = TempDir::new("no-pinned-commit").expect("temp dir");
    fs::create_dir_all(work.path().join("migrations")).expect("migrations");
    fs::write(
        work.path().join("migrations/001_initial.sql"),
        "-- not the pinned baseline\n",
    )
    .expect("write");
    let initialized = Command::new("git")
        .args(["init", "-q"])
        .current_dir(work.path())
        .status()
        .expect("git init");
    assert!(initialized.success(), "git init failed");
    for arguments in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=wave5b@example.invalid",
            "-c",
            "user.name=wave5b",
            "commit",
            "-q",
            "-m",
            "a history without the pinned commit",
        ],
    ] {
        let ran = Command::new("git")
            .args(&arguments)
            .current_dir(work.path())
            .status()
            .expect("git");
        assert!(ran.success(), "git {arguments:?} failed");
    }
    let error = catalog::validate(work.path(), &[catalog::Check::History])
        .expect_err("a history that cannot see the pinned commit must fail closed");
    let message = error.to_string();
    assert!(
        message.contains(&format!("[{CHECK_HISTORY}]")),
        "the refusal must name the history check, got: {message}"
    );
    assert!(
        message.contains("does not fall back"),
        "the refusal must state that there is no fallback, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// S-3 — the 13 host tables
// ---------------------------------------------------------------------------

#[test]
fn the_attribution_table_is_migration_019_itself() {
    let path = repository().join("migrations/019_identity_authorization_foundation.sql");
    let raw = fs::read_to_string(&path).expect("migration 019 is checked in");
    // The same scanner the module catalog uses, over the statements that are
    // inside the module SQL subset. 019 also renames four v0 demo tables, a form
    // §11.3 keeps out of module revisions on purpose; 019_is_not_a_module_catalog
    // asserts that refusal instead of quietly widening the subset for it.
    let text: String = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("ALTER TABLE IF EXISTS"))
        .collect::<Vec<_>>()
        .join("\n");
    let (declarations, events) = catalog::analyse_sql(
        &text,
        "migrations/019_identity_authorization_foundation.sql",
    )
    .expect("019's table and index DDL parses with the owner scanner");
    let created: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            catalog::SqlEvent::Create(table) => Some(table.clone()),
            catalog::SqlEvent::Reference(_) => None,
        })
        .collect();
    let owned: Vec<&str> = HOST_BASELINE_TABLE_OWNERSHIP
        .iter()
        .map(|(table, _)| *table)
        .collect();
    assert_eq!(created.len(), 13, "019 creates exactly the 13 host tables");
    for table in &owned {
        assert!(
            created.iter().any(|c| c == *table),
            "{table} is not created by 019"
        );
    }
    for table in &created {
        assert!(
            owned.contains(&table.as_str()),
            "019 creates {table}, which is not attributed"
        );
    }
    assert!(
        declarations.is_empty(),
        "the host baseline carries no module ownership marker: {declarations:?}"
    );
    // Design §3.1 attributes ten indexes to 019. Every index statement is read
    // back out of the file (statements span lines, so splitting on ';' first)
    // and each one's `ON <table>` target must appear as a reference the scanner
    // produced. A foreign key also yields a reference, so this asserts coverage
    // of the index targets rather than an exact reference count.
    let index_targets: Vec<String> = raw
        .split(';')
        .filter(|statement| {
            let upper = statement.to_ascii_uppercase();
            upper.contains("CREATE") && upper.contains("INDEX")
        })
        .filter_map(|statement| {
            let upper = statement.to_ascii_uppercase();
            let rest = &upper[(upper.find(" ON ")? + 4)..];
            let token: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!token.is_empty()).then_some(token.to_lowercase())
        })
        .collect();
    assert_eq!(
        index_targets.len(),
        10,
        "design §3.1 attributes ten indexes to 019"
    );
    let referenced: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            catalog::SqlEvent::Reference(table) => Some(table.clone()),
            catalog::SqlEvent::Create(_) => None,
        })
        .collect();
    for table in &index_targets {
        assert!(
            referenced.contains(table),
            "019 indexes '{table}' but the owner scanner did not see it"
        );
    }
    for table in &referenced {
        assert!(
            created.contains(table),
            "019 references '{table}', which it does not create: the host baseline \
             would depend on a table outside itself"
        );
    }
}

#[test]
fn migration_019_is_not_a_module_catalog() {
    // The refusal above is a statement about the module subset, not a defect in
    // the frozen baseline: the module scan only ever reads migrations/modules/.
    let raw = fs::read_to_string(
        repository().join("migrations/019_identity_authorization_foundation.sql"),
    )
    .expect("migration 019 is checked in");
    let outcome =
        catalog::analyse_sql(&raw, "migrations/019_identity_authorization_foundation.sql");
    assert!(
        outcome.is_err(),
        "the whole of 019 is outside the module SQL subset"
    );
    let reason = match outcome {
        Ok((declarations, _)) => {
            format!(
                "unexpectedly validated with {} declarations",
                declarations.len()
            )
        }
        Err(error) => error.to_string(),
    };
    assert!(
        reason.contains("'RENAME TO'"),
        "unexpected refusal: {reason}"
    );
}
#[test]
fn host_table_ownership_is_unique_and_inside_the_authoritative_contexts() {
    let mut seen: Vec<&str> = Vec::new();
    for (table, owner) in HOST_BASELINE_TABLE_OWNERSHIP {
        assert!(!seen.contains(table), "{table} is attributed twice");
        seen.push(table);
        assert!(
            catalog::AUTHORITATIVE_NAMESPACE_CONTEXTS.contains(owner),
            "{table} is owned by '{owner}', which is not an authoritative context"
        );
        assert_eq!(
            catalog::host_table_owner(table),
            Some(*owner),
            "{table} resolves to a different owner"
        );
    }
    assert_eq!(catalog::host_table_owner("documents"), None);
}

// ---------------------------------------------------------------------------
// S-4 — the check set and this repository
// ---------------------------------------------------------------------------

#[test]
fn the_default_gate_excludes_the_candidate_freeze() {
    assert_eq!(
        catalog::default_checks(),
        vec![
            catalog::Check::Catalog,
            catalog::Check::History,
            catalog::Check::Embed
        ],
        "the freeze is a candidate-only assertion until ADR-0029 is accepted"
    );
    for name in [CHECK_CATALOG, CHECK_HISTORY, CHECK_EMBED, CHECK_FREEZE] {
        assert_eq!(
            catalog::Check::parse(name).expect("known check").name(),
            name
        );
    }
    catalog::Check::parse("everything").expect_err("unknown checks are refused");
}

#[test]
fn the_repository_itself_passes_the_default_checks() {
    let report = catalog::validate(&repository(), &catalog::default_checks()).expect("repository");
    let printed = report.log_lines().join("\n");
    assert_eq!(
        report
            .namespaces
            .iter()
            .map(|namespace| (namespace.catalog.namespace.as_str(), namespace.status()))
            .collect::<Vec<_>>(),
        vec![
            ("identity-management", STATUS_RESERVED),
            ("policy", STATUS_RESERVED),
        ],
        "S1 reserves both catalogs; nothing may report an installation state\n{printed}"
    );
    assert_eq!(report.order, vec!["identity-management", "policy"]);
    for namespace in &report.namespaces {
        assert_eq!(
            namespace.catalog.depends_on,
            vec!["platform-baseline@19".to_owned()]
        );
        assert!(
            namespace.revisions.is_empty(),
            "S1 ships no module revisions"
        );
    }
    let history = report.history.expect("history ran");
    assert_eq!(history.baseline_revision, 19);
    assert_eq!(
        history.legacy_manifest_form_entries, 7,
        "the C1 seven stay green"
    );
    assert_eq!(history.blob_entries_verified, 40, "20 paths × HEAD + index");
    assert_eq!(history.working_tree_entries_verified, 20);
}

#[test]
fn the_freeze_only_counts_the_flat_baseline() {
    let report = catalog::verify_candidate_freeze(
        &repository(),
        &catalog::pinned_baseline_history().expect("table"),
    )
    .expect("the working tree is a valid candidate for the freeze");
    assert_eq!(report.baseline_sql.len(), 19);
    assert_eq!(report.subdirectories, vec!["modules".to_owned()]);
    assert!(
        !report
            .baseline_sql
            .iter()
            .any(|name| name.contains("wave5b_fixture")),
        "a module revision must never count as a baseline SQL file"
    );
}

#[test]
fn a_new_namespace_digest_is_canonical_lf_only() {
    // The other half of C1: the historical tolerance must not leak into a new
    // namespace catalog. The `manifest-non-canonical-digest` fixture is refused;
    // this asserts the fixture is meaningful, i.e. the two digests do differ.
    let source = fixture_root().join("validated/policy/001_wave5b_fixture_foundation.sql");
    let bytes = fs::read(&source).expect("fixture SQL");
    assert!(!bytes.contains(&b'\r'), "the positive fixture is LF");
    let crlf = catalog::to_crlf(&bytes);
    assert_ne!(sha256_hex(&bytes), sha256_hex(&crlf));
    let refused = catalog::validate(
        workspace_for("manifest-non-canonical-digest").path(),
        &[catalog::Check::Catalog],
    )
    .expect_err("a CRLF digest in a new catalog is refused");
    assert!(refused.to_string().contains("canonical LF digest"));
    let accepted = catalog::validate(
        workspace_for("crlf-checkout-validated").path(),
        &[catalog::Check::Catalog],
    )
    .expect("a CRLF checkout with a canonical digest is a valid catalog");
    assert_eq!(accepted.namespaces[0].status(), STATUS_VALIDATED);
}

// ---------------------------------------------------------------------------
// S-5 — the single baseline embed point
// ---------------------------------------------------------------------------

#[test]
fn exactly_one_file_embeds_the_host_baseline_catalog() {
    let report = catalog::verify_embed_point(&repository()).expect("embed scan");
    let baseline = report.baseline_sites();
    assert_eq!(
        baseline.len(),
        1,
        "review §3.7 allows exactly one baseline embed"
    );
    assert_eq!(baseline[0].file, BASELINE_EMBED_FILE);
    assert_eq!(baseline[0].resolves_to, BASELINE_DIRECTORY);
    let mut crates: Vec<String> = report
        .crate_sites()
        .into_iter()
        .map(|site| site.resolves_to.clone())
        .collect();
    crates.sort();
    assert_eq!(
        crates,
        vec![
            "crates/document-processing-sqlite/migrations".to_owned(),
            "crates/document-sqlite/migrations".to_owned(),
        ],
        "only the two SQLite adapters may embed their own catalog"
    );
    assert!(
        report.scanned_files > 100,
        "the scan must cover apps/ and crates/"
    );
}

#[test]
fn the_conformance_tests_do_not_embed_a_second_baseline() {
    for site in [
        "crates/messaging/tests/outbox_reliability.rs",
        "crates/audit-postgres/tests/chain_pg.rs",
    ] {
        let read = fs::read_to_string(repository().join(site.replace('/', "\\")));
        assert!(read.is_ok(), "{site} exists");
        let text = read.expect("checked above");
        let code: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        let joined = code.join("\n");
        assert!(
            !joined.contains("sqlx::migrate!"),
            "{site} embeds a migration catalog; use runtime_migration::MIGRATOR"
        );
        assert!(
            joined.contains("runtime_migration::MIGRATOR"),
            "{site} must apply the baseline through the single shared migrator"
        );
    }
}

#[test]
fn unattributable_sql_forms_are_refused_by_name() {
    for (statement, needle) in [
        (
            "ALTER TABLE wave5b_t RENAME TO roles;",
            "'RENAME TO' renames the whole table",
        ),
        (
            "CREATE TABLE wave5b_t (LIKE platform_users INCLUDING ALL);",
            "'LIKE <table>' copies another table's definition",
        ),
        (
            "CREATE TABLE wave5b_t () PARTITION OF roles;",
            "'PARTITION OF' inherits a parent",
        ),
        ("DO $$ BEGIN END $$;", "dollar-quoted"),
        (
            "CREATE SEQUENCE wave5b_s;",
            "is outside the supported SQL subset",
        ),
    ] {
        let outcome = catalog::analyse_sql(statement, "fixture").err();
        let message = outcome.expect("the statement is refused").to_string();
        assert!(
            message.contains(needle),
            "{statement}: expected {needle:?}, got {message}"
        );
    }
}

#[test]
fn dependency_grammar_is_fixed_and_does_not_guess() {
    assert_eq!(
        catalog::parse_dependency("platform-baseline@19").expect("baseline"),
        catalog::Dependency::Baseline(19)
    );
    assert_eq!(
        catalog::parse_dependency("policy@3").expect("namespace"),
        catalog::Dependency::Namespace("policy".to_owned(), 3)
    );
    for entry in [
        "platform-baseline",
        "platform-baseline@0",
        "platform-baseline@019",
        "policy@x",
        "@1",
    ] {
        catalog::parse_dependency(entry).expect_err("the grammar is exact");
    }
}
