#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 Round 3 — the cleanup listing's pagination **rules**, on
//! deterministic provider answers.
//!
//! Wave 4 Round 2 added the pagination *loop* and claimed it worked. What it
//! actually proved was weaker than it looked:
//!
//! * `list_all_incomplete_uploads` advanced on **the key alone** — it rejected a
//!   page whose marker key equalled the previous one. S3 orders multipart
//!   uploads by `(key, upload-id)`, so a key with more uploads than one page
//!   *has* to repeat its key marker and move only the upload-id. Seven
//!   incomplete uploads on one key therefore ended the listing with
//!   "did not advance the marker", and upload ids are opaque so they cannot be
//!   ordered locally as a substitute.
//! * `list_all_keys` returned `Ok` when the provider said `IsTruncated=true` but
//!   handed back no continuation token. That is the worst possible direction to
//!   fail in: cleanup deletes the first page, believes the list was complete,
//!   and reports a clean prefix that still holds objects.
//! * Neither loop noticed a cursor that repeated or cycled, so a provider doing
//!   that was indistinguishable from one making progress.
//!
//! Round 2's real lane never saw any of this because its multipart case had
//! exactly **one** upload — one page, no cursor, and the defect never ran.
//!
//! These tests drive the real `support::list_all_keys_with_pages` /
//! `support::list_all_incomplete_uploads_with_pages` through a scripted SDK
//! transport ([`support::ScriptedProvider`]): the SDK parses ordinary S3 XML and
//! the parsed response goes into the production pagination code. There is no
//! second copy of the algorithm here to keep in sync. They need no provider, no
//! `WAVE4_S3_*` environment and no host trust store, so they run in the ordinary
//! `cargo test` pass rather than only in the ignored lane.
//!
//! The last group covers the "created but never listed" judgement from the fault
//! lane, including the negative fixtures: a provider that lists nothing must not
//! be able to produce a green scenario.

mod support;

use support::{
    judge_upload_discovery, list_all_incomplete_uploads_with_pages, list_all_keys_with_pages,
    parse_created_uploads, report_and_fail, CleanupReport, CreatedUpload, DiscoveryOutcome,
    ObjectsPage, ScriptedProvider, Truncation, UploadsPage, CREATED_UPLOAD_MARKER, MAX_LIST_PAGES,
};

const BUCKET: &str = "scripted-bucket";
const PREFIX: &str = "plan0014-wave4/scripted/";

/// The key every upload in the single-key scenarios shares.
fn shared_key() -> String {
    format!("{PREFIX}interrupted/multipart.bin")
}

fn shared_uploads(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|index| (shared_key(), format!("upload-{index:03}")))
        .collect()
}

/// `key-marker=…` as the SDK put it on the wire for one request.
fn key_marker_of(request: &str) -> Option<String> {
    query_value(request, "key-marker")
}

fn upload_id_marker_of(request: &str) -> Option<String> {
    query_value(request, "upload-id-marker")
}

fn continuation_token_of(request: &str) -> Option<String> {
    query_value(request, "continuation-token")
}

fn query_value(request: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    request
        .split('?')
        .nth(1)?
        .split('&')
        .find(|pair| pair.starts_with(&prefix))
        .map(|pair| pair[prefix.len()..].to_owned())
}

// ---------------------------------------------------------------------------
// ListMultipartUploads: the cursor is the whole (key, upload-id) pair
// ---------------------------------------------------------------------------

/// Seven incomplete uploads on **one** key, page size 3.
///
/// This is the case Round 2's real run never reached: the first two pages are
/// truncated with the *same* key marker and a *different* upload-id marker, and
/// the only legal reading is that the pair advanced.
#[tokio::test]
async fn seven_uploads_on_one_key_are_all_listed_across_three_pages() {
    let uploads = shared_uploads(7);
    let provider = ScriptedProvider::uploads(vec![
        UploadsPage::truncated_to(&uploads[0..3], (&uploads[2].0, &uploads[2].1)),
        UploadsPage::truncated_to(&uploads[3..6], (&uploads[5].0, &uploads[5].1)),
        UploadsPage::complete(&uploads[6..7]),
    ]);
    let client = provider.client();

    let listing = list_all_incomplete_uploads_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect("a same-key multi-upload listing is valid pagination, not an error");

    assert_eq!(
        listing.items, uploads,
        "the listing lost or reordered uploads that the provider returned"
    );
    assert_eq!(
        listing.pages, 3,
        "7 uploads at the lane page size must take three pages"
    );
    assert_eq!(
        provider.unanswered(),
        0,
        "the listing asked for pages the scripted provider never offered"
    );

    let requests = provider.requests();
    assert_eq!(requests.len(), 3, "one request per page");
    assert!(
        key_marker_of(&requests[0]).is_none() && upload_id_marker_of(&requests[0]).is_none(),
        "the first page must not send a marker: {}",
        requests[0]
    );
    // The pair, not just the key: identical key, advanced upload-id.
    assert_eq!(
        key_marker_of(&requests[1]),
        key_marker_of(&requests[2]),
        "both later pages page through the same key, so the key marker must repeat: {:?} / {:?}",
        requests[1],
        requests[2]
    );
    assert_eq!(
        upload_id_marker_of(&requests[1]).as_deref(),
        Some("upload-002"),
        "page 2 must resume after the last upload of page 1: {}",
        requests[1]
    );
    assert_eq!(
        upload_id_marker_of(&requests[2]).as_deref(),
        Some("upload-005"),
        "page 3 must resume after the last upload of page 2: {}",
        requests[2]
    );
    assert!(
        key_marker_of(&requests[1]).is_some(),
        "the key half of the cursor was never sent: {}",
        requests[1]
    );
}

/// Several keys spread over pages still ends with every upload, once.
#[tokio::test]
async fn uploads_spread_over_several_keys_are_listed_exactly_once() {
    let alpha = format!("{PREFIX}alpha.bin");
    let beta = format!("{PREFIX}beta.bin");
    let page1 = vec![
        (alpha.clone(), "a-1".to_string()),
        (alpha.clone(), "a-2".to_string()),
        (beta.clone(), "b-1".to_string()),
    ];
    let page2 = vec![(beta.clone(), "b-2".to_string())];
    let provider = ScriptedProvider::uploads(vec![
        UploadsPage::truncated_to(&page1, (&beta, "b-1")),
        UploadsPage::complete(&page2),
    ]);
    let client = provider.client();

    let listing = list_all_incomplete_uploads_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect("a key change mid-scan is ordinary pagination");

    let mut expected = page1.clone();
    expected.extend(page2.clone());
    assert_eq!(listing.items, expected);
    assert_eq!(listing.pages, 2);
}

/// A cursor pair that repeats exactly is not progress. Reporting `Ok` there
/// would tell cleanup it had the whole list when it had the same page twice.
#[tokio::test]
async fn a_repeated_cursor_pair_is_an_error_not_a_complete_listing() {
    let uploads = shared_uploads(3);
    let provider = ScriptedProvider::uploads(vec![
        UploadsPage::truncated_to(&uploads, (&uploads[0].0, &uploads[0].1)),
        // Same pair again: the provider is stuck.
        UploadsPage::truncated_to(&uploads, (&uploads[0].0, &uploads[0].1)),
        UploadsPage::complete(&[]),
    ]);
    let client = provider.client();

    let error = list_all_incomplete_uploads_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect_err("a repeated cursor cannot be reported as a complete listing");
    assert!(
        error.contains("upload"),
        "the failure must be about the upload listing, got: {error}"
    );
    assert!(
        provider.unanswered() > 0,
        "the loop kept requesting pages after the cursor repeated"
    );
}

/// A pair that cycles A → B → A is the same lie in a longer dress.
#[tokio::test]
async fn a_cycling_cursor_pair_is_an_error_not_a_complete_listing() {
    let uploads = shared_uploads(3);
    let cursor_a = (uploads[0].0.clone(), uploads[0].1.clone());
    let cursor_b = (uploads[1].0.clone(), uploads[1].1.clone());
    let provider = ScriptedProvider::uploads(vec![
        UploadsPage::truncated_to(&uploads, (&cursor_a.0, &cursor_a.1)),
        UploadsPage::truncated_to(&uploads, (&cursor_b.0, &cursor_b.1)),
        UploadsPage::truncated_to(&uploads, (&cursor_a.0, &cursor_a.1)),
        UploadsPage::complete(&[]),
    ]);
    let client = provider.client();

    let error = list_all_incomplete_uploads_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect_err("a cycling cursor cannot be reported as a complete listing");
    assert!(
        error.to_lowercase().contains("cursor") || error.contains("marker"),
        "the failure must name the cursor, got: {error}"
    );
}

/// `IsTruncated=true` with neither marker: there is nothing to send next, so the
/// only honest answer is an error.
#[tokio::test]
async fn a_truncated_upload_page_without_a_cursor_is_an_error() {
    let uploads = shared_uploads(3);
    let provider = ScriptedProvider::uploads(vec![UploadsPage {
        uploads: uploads.clone(),
        truncated: Truncation::Answered(true),
        next_key_marker: None,
        next_upload_id_marker: None,
    }]);
    let client = provider.client();

    let error = list_all_incomplete_uploads_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect_err("a truncated page with no cursor is not the end of the listing");
    assert!(
        error.contains("marker") || error.contains("cursor"),
        "the failure must say the cursor was missing, got: {error}"
    );
}

/// Half a cursor is worse than none: sending the key marker alone restarts the
/// whole key, and an upload-id alone has no key to belong to.
#[tokio::test]
async fn a_truncated_upload_page_with_only_half_a_cursor_is_an_error() {
    let uploads = shared_uploads(3);
    for (label, key_marker, upload_id_marker) in [
        (
            "key marker and an empty upload-id marker",
            Some(uploads[2].0.clone()),
            Some(String::new()),
        ),
        ("upload-id marker only", None, Some(uploads[2].1.clone())),
    ] {
        let provider = ScriptedProvider::uploads(vec![UploadsPage {
            uploads: uploads.clone(),
            truncated: Truncation::Answered(true),
            next_key_marker: key_marker,
            next_upload_id_marker: upload_id_marker,
        }]);
        let client = provider.client();
        let error = list_all_incomplete_uploads_with_pages(&client, BUCKET, PREFIX)
            .await
            .expect_err(&format!(
                "a truncated page with a {label} is not a usable cursor"
            ));
        assert!(
            !error.is_empty(),
            "the {label} case failed with an empty message"
        );
    }
}

/// A provider that never says "not truncated" must not spin forever. The bound
/// is a failure, never a success with a partial list.
#[tokio::test]
async fn an_upload_listing_that_never_terminates_fails_on_the_page_bound() {
    let uploads = shared_uploads(3);
    let pages = (0..MAX_LIST_PAGES + 5)
        .map(|page| {
            UploadsPage::truncated_to(&uploads, (&shared_key(), &format!("upload-{page:05}")))
        })
        .collect();
    let provider = ScriptedProvider::uploads(pages);
    let client = provider.client();

    let error = list_all_incomplete_uploads_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect_err("an endless listing must stop with an error, not with Ok");
    assert!(
        error.contains("page"),
        "the failure must name the page bound, got: {error}"
    );
}

// ---------------------------------------------------------------------------
// ListObjectsV2: only IsTruncated=false ends the scan
// ---------------------------------------------------------------------------

/// The normal ending, and the multi-page walk that leads to it.
#[tokio::test]
async fn an_objects_listing_ends_on_is_truncated_false_after_every_page() {
    let first = [
        format!("{PREFIX}probe-0"),
        format!("{PREFIX}probe-1"),
        format!("{PREFIX}probe-2"),
    ];
    let second = [format!("{PREFIX}probe-3")];
    let provider = ScriptedProvider::objects(vec![
        ObjectsPage::truncated_to(&first, "token-2"),
        ObjectsPage::complete(&second),
    ]);
    let client = provider.client();

    let listing = list_all_keys_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect("a two-page object listing is normal");

    let mut expected = first.to_vec();
    expected.extend(second);
    assert_eq!(listing.items, expected);
    assert_eq!(listing.pages, 2);

    let requests = provider.requests();
    assert!(
        continuation_token_of(&requests[0]).is_none(),
        "the first page must not send a token: {}",
        requests[0]
    );
    assert_eq!(
        continuation_token_of(&requests[1]).as_deref(),
        Some("token-2"),
        "page 2 must send the token the provider handed back: {}",
        requests[1]
    );
}

/// The Round 2 defect, kept as a regression: `IsTruncated=true` and no token
/// used to `return Ok(keys)`, i.e. "we have the whole list".
#[tokio::test]
async fn objects_truncated_without_a_token_is_an_error_not_a_full_listing() {
    let provider = ScriptedProvider::objects(vec![ObjectsPage {
        keys: vec![format!("{PREFIX}probe-0")],
        truncated: Truncation::Answered(true),
        next_token: None,
    }]);
    let client = provider.client();

    let error = list_all_keys_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect_err("a truncated object listing with no token is not complete");
    assert!(
        error.contains("token") || error.contains("truncated"),
        "the failure must say why the listing is incomplete, got: {error}"
    );
}

/// An empty string is not a token.
#[tokio::test]
async fn objects_truncated_with_an_empty_token_is_an_error() {
    let provider = ScriptedProvider::objects(vec![ObjectsPage {
        keys: vec![format!("{PREFIX}probe-0")],
        truncated: Truncation::Answered(true),
        next_token: Some(String::new()),
    }]);
    let client = provider.client();

    assert!(
        list_all_keys_with_pages(&client, BUCKET, PREFIX)
            .await
            .is_err(),
        "an empty continuation token must not be treated as progress"
    );
}

/// The same token twice means the provider is not moving.
#[tokio::test]
async fn objects_with_a_repeated_or_cycling_token_are_an_error() {
    for (label, tokens) in [
        ("repeated", vec!["tok-a", "tok-a"]),
        ("cycling", vec!["tok-a", "tok-b", "tok-a"]),
    ] {
        let pages = tokens
            .iter()
            .map(|token| ObjectsPage::truncated_to(&[format!("{PREFIX}probe")], token))
            .collect();
        let provider = ScriptedProvider::objects(pages);
        let client = provider.client();
        let error = list_all_keys_with_pages(&client, BUCKET, PREFIX)
            .await
            .expect_err(&format!("a {label} continuation token is not progress"));
        assert!(
            !error.is_empty(),
            "the {label} token case failed with an empty message"
        );
    }
}

/// A page that does not answer `IsTruncated` says nothing about whether there is
/// more, so it cannot license a "prefix is empty" claim.
#[tokio::test]
async fn an_objects_page_that_does_not_answer_is_truncated_is_an_error() {
    let provider = ScriptedProvider::objects(vec![ObjectsPage {
        keys: vec![format!("{PREFIX}probe-0")],
        truncated: Truncation::Unanswered,
        next_token: None,
    }]);
    let client = provider.client();

    let error = list_all_keys_with_pages(&client, BUCKET, PREFIX)
        .await
        .expect_err("an unanswered IsTruncated cannot end the scan as complete");
    assert!(
        error.contains("IsTruncated") || error.contains("truncated"),
        "the failure must name the missing answer, got: {error}"
    );
}

// ---------------------------------------------------------------------------
// "Created but never listed" is not a pass
// ---------------------------------------------------------------------------

/// The negative fixture the fault lane needs: the child really created an
/// upload, the provider lists **nothing**. Round 2 accepted this — it only
/// checked the listing was non-empty — and printed a provider-limitation line
/// while the scenario still went green.
#[test]
fn a_provider_that_lists_nothing_does_not_verify_upload_discovery() {
    let created = vec![(shared_key(), "upload-000".to_string())];
    let outcome = judge_upload_discovery(&created, &[]);
    assert_eq!(
        outcome,
        DiscoveryOutcome::NotDiscovered {
            missing: created.clone(),
            listed: 0
        },
        "an empty listing must be reported as 'the upload was not discovered'"
    );
    assert!(
        !outcome.counts_as_pass(),
        "an undiscovered upload must never count as a pass"
    );
}

/// A non-empty listing is not the same as *this* upload being found: the exact
/// `(key, upload-id)` pair has to be there.
#[test]
fn a_different_upload_in_the_listing_is_not_discovery_of_the_created_one() {
    let created = vec![(shared_key(), "upload-000".to_string())];
    let listed = vec![
        (shared_key(), "somebody-elses-upload".to_string()),
        (format!("{PREFIX}other.bin"), "upload-999".to_string()),
    ];
    let outcome = judge_upload_discovery(&created, &listed);
    assert!(
        !outcome.counts_as_pass(),
        "a listing that contains a different upload does not prove discovery: {outcome:?}"
    );
}

/// The same pair under a different key is a different upload.
#[test]
fn the_pair_is_matched_on_both_halves() {
    let key = shared_key();
    let created = vec![(key.clone(), "upload-000".to_string())];
    let listed = vec![(format!("{PREFIX}another.bin"), "upload-000".to_string())];
    assert!(
        !judge_upload_discovery(&created, &listed).counts_as_pass(),
        "matching only the upload-id half would accept the wrong upload"
    );
    let listed = vec![(key, "upload-000".to_string())];
    assert!(
        judge_upload_discovery(&created, &listed).counts_as_pass(),
        "the exact pair present must be a pass"
    );
}

/// A child that reports nothing gives the parent nothing to check.
#[test]
fn a_child_that_reported_nothing_cannot_produce_a_pass() {
    let outcome = judge_upload_discovery(&[], &[(shared_key(), "upload-000".to_string())]);
    assert_eq!(outcome, DiscoveryOutcome::Unreported);
    assert!(
        !outcome.counts_as_pass(),
        "an unverifiable scenario must not be counted as a pass"
    );
}

/// A listing that failed is not a scope that was checked.
///
/// `remaining_objects = 0` means two entirely different things depending on
/// whether the listing finished, and only one of them is evidence. This is the
/// difference between "the prefix is empty" and "we stopped looking".
#[test]
fn a_failed_listing_is_never_reported_as_a_clean_scope() {
    let cleaned = CleanupReport {
        prefix: PREFIX.to_string(),
        errors: vec![format!(
            "list_objects_v2 page 2 under {PREFIX} repeated continuation token"
        )],
        listing_incomplete: true,
        objects_deleted: 3,
        ..Default::default()
    };
    assert!(
        !cleaned.ok(),
        "a pass whose listing failed is not a clean scope, however few residues it counted"
    );
    let rendered = cleaned.render();
    assert!(
        rendered.contains("LISTING INCOMPLETE"),
        "the report must say the listing did not finish, not just print zero residues: \
         {rendered}"
    );

    // The same shape the CI step depends on: the failure reaches the exit code.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let verdict = std::panic::catch_unwind(|| report_and_fail(&cleaned));
    std::panic::set_hook(previous_hook);
    assert!(
        verdict.is_err(),
        "report_and_fail let a pass with a failed listing exit green"
    );

    let control = CleanupReport {
        prefix: PREFIX.to_string(),
        objects_deleted: 3,
        ..Default::default()
    };
    assert!(
        control.ok() && report_and_fail_control_passes(&control),
        "the assertion above would also pass if a genuinely clean scope failed too"
    );
}

/// `report_and_fail` on a clean report must not panic — the negative control the
/// test above needs to mean anything.
#[must_use]
fn report_and_fail_control_passes(report: &CleanupReport) -> bool {
    std::panic::catch_unwind(|| report_and_fail(report)).is_ok()
}

/// The harness writes `test <name> ... ` and the child's first line continues on
/// it, so a line-start rule silently drops the **first** record. The first Round 3
/// lane run hit exactly this: one upload created, none reported.
#[test]
fn a_record_sharing_a_line_with_the_harness_still_parses() {
    let upload = CreatedUpload {
        bucket: BUCKET.to_string(),
        run_id: "r3-scripted".to_string(),
        key: shared_key(),
        upload_id: "upload-000".to_string(),
    };
    let json = serde_json::to_string(&upload).expect("json");
    let (parsed, malformed) = parse_created_uploads(&format!(
        "running 1 test\ntest child_entry_point ... {CREATED_UPLOAD_MARKER} {json}\n\
         {CREATED_UPLOAD_MARKER} {json}\n"
    ));
    assert!(
        malformed.is_empty(),
        "a well-formed record must not be reported as damage: {malformed:?}"
    );
    assert_eq!(
        parsed.len(),
        2,
        "a record must parse whether or not the harness shares its line"
    );
    assert_eq!(parsed[0], parsed[1], "both lines describe the same upload");
}

/// The child→parent record survives a round trip, and a damaged record is
/// surfaced rather than dropped: guessing from the narrative log is what this
/// channel replaces.
#[test]
fn created_upload_records_are_structured_and_garbage_is_reported() {
    let upload = CreatedUpload {
        bucket: BUCKET.to_string(),
        run_id: "r3-scripted".to_string(),
        key: shared_key(),
        upload_id: "upload-000".to_string(),
    };
    let line = format!(
        "{CREATED_UPLOAD_MARKER} {}",
        serde_json::to_string(&upload).expect("json")
    );
    let noisy = format!(
        "running 1 test\ntest child_entry_point - FAILED\n{line}\n{CREATED_UPLOAD_MARKER} not-json\n"
    );
    let (parsed, malformed) = parse_created_uploads(&noisy);
    assert_eq!(parsed, vec![upload], "the record must round-trip intact");
    assert_eq!(
        malformed.len(),
        1,
        "a malformed record must be reported, not skipped"
    );
}
