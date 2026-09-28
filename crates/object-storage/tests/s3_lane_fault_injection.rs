#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 Round 2 — fault scenarios for the split provider lane.
//!
//! Round 1 proved the happy path: the contract cases passed and the prefix was
//! empty afterwards. That says nothing about the case the split exists for —
//! the contract step failing part way through, with objects and half-finished
//! multipart uploads still in the bucket. A cleanup step that only ever runs
//! after success is a step that has never been tested.
//!
//! Each test here drives a **child process that creates a mess and then fails on
//! purpose**, checks the mess is really there, and only then runs the cleanup
//! step the way CI runs it (`if: always()`, same run id, separate exit code).
//! Three properties, one per scenario:
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
    cleanup_run_prefix, key_under_prefix, list_all_incomplete_uploads, list_all_keys,
    more_than_a_page, port_client, read_sentinel, report_and_fail, run_prefix, sdk_client,
    send_probe_objects, sentinel_prefix, spawn_case, write_sentinel, FAULT_ARMED,
};

/// Body of the object the cleanup must not touch, scenario (c).
const SENTINEL_BODY: &str = "plan0014 wave 4 round 2 sentinel — must survive";

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
        // created and left incomplete, then the process dies.
        CASE_UPLOAD_THEN_INTERRUPT => {
            let sdk = sdk_client();
            let key = key_under_prefix("interrupted/multipart.bin");
            let (key, upload_id) = support::leave_an_incomplete_upload(&sdk, &key).await;
            println!("{FAULT_ARMED} left incomplete upload {key}#{upload_id}");
            panic!("injected interruption: the upload above was never completed or aborted");
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
    let created_line = failed
        .output
        .lines()
        .find(|line| line.contains(FAULT_ARMED))
        .unwrap_or_default()
        .to_string();

    let before = list_all_incomplete_uploads(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .expect("list incomplete uploads");
    if before.is_empty() {
        // Recorded, not asserted away: a provider that accepts
        // CreateMultipartUpload but does not report it in ListMultipartUploads
        // makes the upload unfindable by *any* client, so no cleanup can abort
        // it. That is a provider capability finding for the matrix, and the
        // evidence for it is this line plus the contract lane's capability run —
        // not a claim that cleanup aborted something.
        println!(
            "PROVIDER-LIMITATION incomplete uploads are not listed by this provider \
             ({created_line}); cleanup cannot abort what the provider will not list"
        );
    } else {
        println!(
            "FAULT (b) interrupted upload visible to ListMultipartUploads: {} entry(s) \
             ({created_line})",
            before.len()
        );
    }

    let report = cleanup_run_prefix().await;
    report_and_fail(&report);
    let after = list_all_incomplete_uploads(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .expect("list incomplete uploads after cleanup");
    assert!(after.is_empty(), "uploads still incomplete: {after:?}");
    let objects = list_all_keys(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .expect("list objects after cleanup");
    assert!(
        objects.is_empty(),
        "objects still under the prefix: {objects:?}"
    );
    if !before.is_empty() {
        assert!(
            report.uploads_aborted >= 1,
            "the provider listed {} incomplete upload(s) but cleanup aborted none",
            before.len()
        );
    }
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
