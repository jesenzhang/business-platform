#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 Round 3 — fault scenarios for the split provider lane.
//!
//! Round 2 proved the split works: a child dies on purpose, the mess is really
//! there, and the cleanup step empties the scope with its own exit code. Round 3
//! closes two things that made the multipart half of that weaker than it read.
//!
//! * **The success was "the listing was not empty", not "the upload I created is
//!   in it."** The child printed its key and upload id into a log line the parent
//!   grepped, and the parent then asserted `!before.is_empty()` on
//!   `ListMultipartUploads`. Any other upload in the scope satisfies that. When
//!   the listing came back empty the scenario printed `PROVIDER-LIMITATION` and
//!   still exited 0 — so a provider that accepts `CreateMultipartUpload` and then
//!   hides the upload produced a green lane, which is the one case worth knowing
//!   about. Round 3 fixes the judgement: the child reports what it created as a
//!   structured [`support::CreatedUpload`] record, the parent asserts the exact
//!   `(key, upload-id)` pair is in the listing, and a listing that does not
//!   contain it **fails the scenario**. The parent still tries to abort what it
//!   knows the id of — leaving an orphan behind would be its own defect — but it
//!   records that compensation separately and does not let it rewrite
//!   "discovery was not verified" into a pass.
//! * **The multipart scenario had exactly one upload.** One upload is one page,
//!   so the cursor rules never ran on a real provider. Scenario (d) now leaves
//!   seven incomplete uploads on a single key at page size 3, which is the shape
//!   that forces a repeated key marker with a moving upload-id, and it checks the
//!   cleanup pass really walked the pages it claimed.
//!
//! The three Round 2 properties still hold and are still tested:
//!
//! * (a) cleanup runs after a failed contract run and empties the scope, while
//!   the contract failure stays reported as the contract failure;
//! * (b) an interrupted upload leaves an *incomplete multipart upload*, which
//!   `ListObjectsV2` never shows, and cleanup finds and aborts it;
//! * (c) an object outside the run prefix is untouched by all of it.
//!
//! The failures injected here are deliberate and confined to these test targets:
//! no production code path is involved, and no provider feature is added to
//! make a scenario easier — see the note on multipart in the contract lane.

mod support;

use support::{
    cleanup_run_prefix, compensating_abort, judge_upload_discovery, key_under_prefix,
    list_all_incomplete_uploads_with_pages, list_all_keys, list_all_keys_with_pages,
    more_than_a_page, port_client, read_sentinel, report_and_fail, run_prefix, sdk_client,
    send_probe_objects, sentinel_prefix, spawn_case, write_sentinel, ChildRun, CreatedUpload,
    DiscoveryOutcome, FAULT_ARMED,
};

/// Body of the object the cleanup must not touch, scenario (c).
const SENTINEL_BODY: &str = "plan0014 wave 4 sentinel — must survive";

/// Selects which injected fault the child process installs.
///
/// The failing cases are reached through one gated entry point rather than
/// being tests of their own: this target runs under `--ignored`, so a test that
/// panics on purpose would fail every real lane run before the scenarios that
/// depend on it had been reported. Unset, the entry point does nothing, which
/// is what the parent's own run sees.
const CHILD_CASE: &str = "PLAN0014_LANE_CHILD_CASE";
const CASE_CONTRACT_THAT_FAILS: &str = "contract-that-fails";
const CASE_UPLOAD_THEN_INTERRUPT: &str = "upload-then-interrupt";
const CASE_MANY_UPLOADS_ONE_KEY_THEN_INTERRUPT: &str = "many-uploads-one-key-then-interrupt";

/// How many incomplete uploads scenario (d) leaves on one key. Three at the
/// lane's page size of 3, so the cursor has to move twice.
const MANY_UPLOADS: usize = 7;

/// The key scenario (d) piles every upload onto.
fn many_uploads_key() -> String {
    key_under_prefix("interrupted/many-on-one-key.bin")
}

/// The child half of the fault scenarios; a no-op unless [`CHILD_CASE`] is set.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn child_entry_point() {
    let Some(case) = std::env::var_os(CHILD_CASE) else {
        return;
    };
    match case.to_string_lossy().as_ref() {
        // Stands in for the contract step dying part way through: real objects
        // under the run prefix, then a panic.
        CASE_CONTRACT_THAT_FAILS => {
            let client = port_client();
            send_probe_objects(&client, &run_prefix(), "abandoned", more_than_a_page()).await;
            println!("{FAULT_ARMED} contract case failed after writing objects");
            panic!("injected contract failure: the lane must still clean up after this");
        }
        // Stands in for a client interrupted mid-upload: the multipart upload is
        // created and left incomplete, then the process dies. The record goes to
        // the parent as data, not only as a line of narrative log.
        CASE_UPLOAD_THEN_INTERRUPT => {
            let sdk = sdk_client();
            let key = key_under_prefix("interrupted/multipart.bin");
            let created = support::leave_incomplete_uploads(&sdk, &key, 1).await;
            let upload = &created[0];
            println!(
                "{FAULT_ARMED} left incomplete upload {}#{}",
                upload.key, upload.upload_id
            );
            panic!("injected interruption: the upload above was never completed or aborted");
        }
        // The same interruption, seven times, on one key.
        CASE_MANY_UPLOADS_ONE_KEY_THEN_INTERRUPT => {
            let sdk = sdk_client();
            let created =
                support::leave_incomplete_uploads(&sdk, &many_uploads_key(), MANY_UPLOADS).await;
            println!(
                "{FAULT_ARMED} left {} incomplete uploads on one key",
                created.len()
            );
            panic!("injected interruption: the uploads above were never completed or aborted");
        }
        other => panic!("unknown lane child case {other:?}"),
    }
}

/// Scenario (a): a failed contract run is still cleaned up, and cleaning up
/// does not rewrite what the contract step concluded.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn a_failed_contract_run_is_still_cleaned_up() {
    // Make the scope deterministic before measuring what the failure leaves.
    report_clean_start(&cleanup_run_prefix().await);

    let failed = spawn_case(
        "child_entry_point",
        &[(CHILD_CASE, Some(CASE_CONTRACT_THAT_FAILS))],
    );
    assert!(
        !failed.succeeded(),
        "the injected contract failure did not fail the child step\n{}",
        failed.output
    );
    assert!(
        failed.output.contains(FAULT_ARMED),
        "the child failed before it wrote the objects, so there was nothing to clean up\n{}",
        failed.output
    );

    let residue = list_all_keys(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .expect("list after the failure");
    assert!(
        residue.len() > 1,
        "the failed run left {} object(s) under {}; the scenario needs more than one \
         listing page to be worth running",
        residue.len(),
        run_prefix()
    );
    println!(
        "FAULT (a) contract step exit={:?} left {} object(s) under {}",
        failed.code,
        residue.len(),
        run_prefix()
    );

    // The CI `if: always()` cleanup step, run here in this process.
    let report = cleanup_run_prefix().await;
    report_and_fail(&report);
    assert!(
        report.objects_deleted >= residue.len(),
        "cleanup reported {} deletions but the failure left {} objects",
        report.objects_deleted,
        residue.len()
    );
    // Two steps, two exit codes: the cleanup succeeding does not retroactively
    // make the contract step green, which is exactly what the CI split has to
    // preserve. Recorded so the evidence log shows both verdicts.
    println!(
        "FAULT (a) contract step exit={:?} (still a failure) and cleanup step exit=0 with \
         {} object(s) removed",
        failed.code, report.objects_deleted
    );
    assert!(
        !failed.succeeded(),
        "recording the cleanup success must not change the contract step's exit code"
    );
}

/// Read the uploads a child reported creating, and refuse to go on with a
/// record that cannot be trusted.
///
/// The records carry the run id and bucket precisely so a leftover line from a
/// different run cannot be mistaken for this one's expectation. A malformed
/// record is a harness bug, and continuing past it would put the parent back to
/// inferring from prose what it is supposed to know exactly.
fn created_uploads_from(run: &ChildRun) -> Vec<CreatedUpload> {
    let (created, malformed) = support::parse_created_uploads(&run.output);
    assert!(
        malformed.is_empty(),
        "the child printed {} malformed {} record(s), so what it created is unknown: {malformed:?}",
        malformed.len(),
        support::CREATED_UPLOAD_MARKER
    );
    for upload in &created {
        assert_eq!(
            upload.run_id,
            support::run_id(),
            "a creation record from run {} leaked into this run",
            upload.run_id
        );
        assert_eq!(
            upload.bucket,
            support::bucket(),
            "a creation record for bucket {} leaked into the lane's bucket {}",
            upload.bucket,
            support::bucket()
        );
    }
    created
}

/// Report what the listing did not contain, try to clean up after ourselves, and
/// then fail — in that order.
///
/// Aborting by known id is hygiene: an upload nobody can list is a permanent
/// capacity leak, and "the test failed" is not a reason to leave it. It is
/// printed as `COMPENSATION` and nothing more. The scenario still fails, because
/// what the scenario exists to check is whether a client can *find* what an
/// interrupted client left behind, and aborting it from the outside is exactly
/// the capability the provider failed to demonstrate.
async fn compensate_and_fail(created: &[CreatedUpload], outcome: &DiscoveryOutcome) -> ! {
    let missing: Vec<(String, String)> = match outcome {
        DiscoveryOutcome::NotDiscovered { missing, .. } => missing.clone(),
        DiscoveryOutcome::Unreported => created.iter().map(CreatedUpload::pair).collect(),
        DiscoveryOutcome::Visible => Vec::new(),
    };
    for upload in created
        .iter()
        .filter(|upload| missing.contains(&upload.pair()))
    {
        match compensating_abort(upload).await {
            Ok(()) => println!(
                "FAULT COMPENSATION aborted {}#{} without the listing finding it",
                upload.key, upload.upload_id
            ),
            Err(error) => println!(
                "FAULT COMPENSATION FAILED for {}#{}: {error}",
                upload.key, upload.upload_id
            ),
        }
    }
    panic!(
        "SCENARIO-BLOCKED upload discovery is not verified ({outcome:?}); {} known \
         upload(s) were compensated separately and that compensation is NOT a pass. \
         If this is the provider refusing to list what it accepted, record it as a \
         provider limitation and leave the scenario non-green — do not widen the \
         assertion back to 'the listing was not empty'.",
        missing.len()
    );
}

/// Scenario (b): cleanup must see what `ListObjectsV2` cannot.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn an_interrupted_multipart_upload_is_found_and_aborted() {
    report_clean_start(&cleanup_run_prefix().await);

    let failed = spawn_case(
        "child_entry_point",
        &[(CHILD_CASE, Some(CASE_UPLOAD_THEN_INTERRUPT))],
    );
    assert!(
        !failed.succeeded() && failed.output.contains(FAULT_ARMED),
        "the injected upload interruption did not happen as designed\n{}",
        failed.output
    );
    let created = created_uploads_from(&failed);
    assert_eq!(
        created.len(),
        1,
        "the child case creates one upload; it reported {:?}",
        created.iter().map(CreatedUpload::pair).collect::<Vec<_>>()
    );

    let before =
        list_all_incomplete_uploads_with_pages(&sdk_client(), &support::bucket(), &run_prefix())
            .await
            .expect("list incomplete uploads");
    println!(
        "FAULT (b) ListMultipartUploads walked {} page(s) and listed {} entry(s); created {}",
        before.pages,
        before.items.len(),
        created[0].upload_id
    );

    let created_pairs: Vec<(String, String)> = created.iter().map(CreatedUpload::pair).collect();
    let outcome = judge_upload_discovery(&created_pairs, &before.items);
    if !outcome.counts_as_pass() {
        compensate_and_fail(&created, &outcome).await;
    }
    println!("FAULT (b) the exact upload the child created is in the listing: {created_pairs:?}");

    let report = cleanup_run_prefix().await;
    report_and_fail(&report);
    assert_run_prefix_is_empty("FAULT (b)").await;
    assert!(
        report.uploads_aborted >= created.len(),
        "the provider listed {} incomplete upload(s) but cleanup aborted {}",
        before.items.len(),
        report.uploads_aborted
    );
    // Visible → cleaned → gone: the three claims the scenario has to make, in
    // the order it makes them.
    println!(
        "FAULT (b) verified {}/{0} upload(s) visible, aborted, and no longer listed",
        created.len()
    );
}

/// Scenario (d): seven incomplete uploads on **one** key, at page size 3.
///
/// This is the multipart shape Round 2's evidence did not contain — its
/// `uploads_aborted=1` line is one upload on one page, which walks no cursor at
/// all. Here the provider has to page through one key with a repeated key marker
/// and a moving upload-id, and cleanup has to abort all seven and leave nothing.
///
/// If the provider cannot page this correctly the scenario does not soften:
/// `compensate_and_fail` aborts what it can and leaves the step red, and the
/// evidence records a real-provider multi-page result of BLOCKED.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn many_incomplete_uploads_on_one_key_are_all_found_and_aborted() {
    report_clean_start(&cleanup_run_prefix().await);
    let sentinel_key = write_sentinel(SENTINEL_BODY).await;

    let failed = spawn_case(
        "child_entry_point",
        &[(CHILD_CASE, Some(CASE_MANY_UPLOADS_ONE_KEY_THEN_INTERRUPT))],
    );
    assert!(
        !failed.succeeded() && failed.output.contains(FAULT_ARMED),
        "the injected multi-upload interruption did not happen as designed\n{}",
        failed.output
    );
    let created = created_uploads_from(&failed);
    assert_many_uploads_reported(&created);

    let before =
        list_all_incomplete_uploads_with_pages(&sdk_client(), &support::bucket(), &run_prefix())
            .await
            .expect("list incomplete uploads");
    println!(
        "FAULT (d) {} upload(s) on one key listed across {} page(s) at page size {}",
        before.items.len(),
        before.pages,
        support::page_size()
    );

    let created_pairs: Vec<(String, String)> = created.iter().map(CreatedUpload::pair).collect();
    let outcome = judge_upload_discovery(&created_pairs, &before.items);
    if !outcome.counts_as_pass() {
        compensate_and_fail(&created, &outcome).await;
    }
    assert!(
        before.pages > 1,
        "the provider answered the whole scope in one page ({} upload(s), page size {}), \
         so this run did not exercise multipart pagination and must not be read as if it \
         did",
        before.items.len(),
        support::page_size()
    );

    let report = cleanup_run_prefix().await;
    report_and_fail(&report);
    assert!(
        report.upload_pages > 1,
        "cleanup's own upload listing stayed on one page ({}), so the multi-page cleanup \
         path was not the path taken",
        report.upload_pages
    );
    assert!(
        report.uploads_aborted >= MANY_UPLOADS,
        "cleanup aborted {} upload(s) but the scenario created {MANY_UPLOADS}",
        report.uploads_aborted
    );
    assert_run_prefix_is_empty("FAULT (d)").await;
    assert_eq!(
        read_sentinel(&sentinel_key).await,
        SENTINEL_BODY,
        "the multi-upload cleanup touched an object outside its prefix"
    );
    assert_eq!(
        list_all_keys(&sdk_client(), &support::bucket(), &sentinel_prefix())
            .await
            .expect("list sentinel prefix"),
        vec![sentinel_key.clone()],
        "the sentinel scope changed"
    );
    println!(
        "FAULT (d) {} upload(s) on one key: listed across {} page(s), aborted {}, none left",
        created.len(),
        before.pages,
        report.uploads_aborted
    );

    sdk_client()
        .delete_object()
        .bucket(support::bucket())
        .key(&sentinel_key)
        .send()
        .await
        .expect("remove the scenario's own sentinel");
}

/// Scenario (c): the sentinel outside the prefix survives a failure and the
/// cleanup that follows it.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn a_failure_and_its_cleanup_leave_other_prefixes_untouched() {
    report_clean_start(&cleanup_run_prefix().await);
    let sentinel_key = write_sentinel(SENTINEL_BODY).await;
    let sibling = format!("plan0014-wave4-sibling/{}", support::run_id());
    let sibling_key = format!("{sibling}sibling.txt");
    sdk_client()
        .put_object()
        .bucket(support::bucket())
        .key(&sibling_key)
        .body(aws_smithy_types::byte_stream::ByteStream::from(
            b"sibling prefix body".to_vec(),
        ))
        .send()
        .await
        .expect("write the sibling-prefix object");

    let failed = spawn_case(
        "child_entry_point",
        &[(CHILD_CASE, Some(CASE_CONTRACT_THAT_FAILS))],
    );
    assert!(
        !failed.succeeded(),
        "the injected failure must fail the run"
    );

    report_and_fail(&cleanup_run_prefix().await);

    assert_eq!(
        read_sentinel(&sentinel_key).await,
        SENTINEL_BODY,
        "cleanup modified an object outside its prefix"
    );
    let sentinel_listing = list_all_keys(&sdk_client(), &support::bucket(), &sentinel_prefix())
        .await
        .expect("list sentinel prefix");
    assert_eq!(
        sentinel_listing.len(),
        1,
        "sentinel scope changed: {sentinel_listing:?}"
    );
    let sibling_listing = list_all_keys(&sdk_client(), &support::bucket(), &sibling)
        .await
        .expect("list sibling");
    assert_eq!(
        sibling_listing,
        vec![sibling_key.clone()],
        "cleanup reached a prefix it was never given"
    );
    assert!(
        list_all_keys(&sdk_client(), &support::bucket(), &run_prefix())
            .await
            .expect("list run prefix")
            .is_empty(),
        "the run prefix is not empty after cleanup"
    );

    for key in [sentinel_key, sibling_key] {
        sdk_client()
            .delete_object()
            .bucket(support::bucket())
            .key(&key)
            .send()
            .await
            .expect("remove the scenario's own out-of-prefix objects");
    }
}

/// What the child's own records have to say before the scenario can measure
/// anything: the right number, all on one key, all distinct.
///
/// Duplicate upload ids would be the quiet failure mode — seven records that
/// describe six uploads still look like a multi-upload scenario.
fn assert_many_uploads_reported(created: &[CreatedUpload]) {
    assert_eq!(
        created.len(),
        MANY_UPLOADS,
        "the child case creates {MANY_UPLOADS} uploads; it reported {}",
        created.len()
    );
    assert!(
        created
            .iter()
            .all(|upload| upload.key == many_uploads_key()),
        "every upload must share one key, or this is the multi-key case again: {:?}",
        created.iter().map(|upload| &upload.key).collect::<Vec<_>>()
    );
    let distinct_ids: std::collections::BTreeSet<&String> =
        created.iter().map(|upload| &upload.upload_id).collect();
    assert_eq!(
        distinct_ids.len(),
        MANY_UPLOADS,
        "the provider handed back duplicate upload ids, so the scenario measures nothing"
    );
    assert!(
        MANY_UPLOADS > usize::try_from(support::page_size()).unwrap_or(3),
        "the scenario needs more uploads than one page to be about pagination at all"
    );
}

/// After cleanup, nothing is left under the run prefix — objects or uploads.
///
/// Shared by every scenario that ends with a cleanup, so "and then it was empty"
/// means the same measured thing each time: a fresh provider listing of both
/// kinds, not a recount of the deletes cleanup issued. The page counts go to the
/// log because a claim about a multi-page scope has to be backed by them.
async fn assert_run_prefix_is_empty(label: &str) {
    let uploads =
        list_all_incomplete_uploads_with_pages(&sdk_client(), &support::bucket(), &run_prefix())
            .await
            .unwrap_or_else(|error| panic!("{label}: listing uploads after cleanup: {error}"));
    let objects = list_all_keys_with_pages(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .unwrap_or_else(|error| panic!("{label}: listing objects after cleanup: {error}"));
    assert!(
        uploads.items.is_empty(),
        "{label}: uploads still incomplete after cleanup: {:?}",
        uploads.items
    );
    assert!(
        objects.items.is_empty(),
        "{label}: objects still under the prefix after cleanup: {:?}",
        objects.items
    );
    println!(
        "{label} post-cleanup scope empty (checked over {} upload page(s) and {} object \
         page(s))",
        uploads.pages, objects.pages
    );
}

/// Cleanup checks between steps rather than at the end of a test: a prefix that
/// is not empty at the start makes every later measurement in this file
/// meaningless, so it is a hard stop where it happens.
fn report_clean_start(report: &support::CleanupReport) {
    print!("{}", report.render());
    assert!(
        report.ok(),
        "the run prefix was not empty before the scenario started (see the CLEANUP lines above)"
    );
}
