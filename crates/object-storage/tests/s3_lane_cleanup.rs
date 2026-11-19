#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! PLAN-0014 Wave 4 Round 2 — the S3 provider lane's **final cleanup step**.
//!
//! Round 1 ran cleanup inside the contract run: one process, one exit code, and
//! three consequences that made the "no residue" claim worth less than it
//! looked.
//!
//! * `lane_leaves_no_objects_under_the_run_prefix` listed the prefix once. A
//!   single `ListObjectsV2` answers at most 1000 keys, so a run that left more
//!   than a page behind deleted the first page and still reported an empty
//!   prefix. Incomplete multipart uploads were not looked at at all, and they
//!   are invisible to `ListObjectsV2`.
//! * Cleanup shared the contract step's exit code, so a cleanup failure was
//!   either invisible (a later capability line passed) or blamed on the
//!   contract.
//! * The bucket teardown test deleted `WAVE4_S3_BUCKET` on request, and CI's
//!   bucket is the `contract-test-bucket` the job provisions for every run.
//!
//! This file is the cleanup step on its own. CI runs it as a separate step with
//! `if: always()` and the same `WAVE4_S3_RUN_ID`, so it executes after the
//! contract step whatever that step concluded, and its own exit code is its
//! own.
//!
//! Round 3 closed the gap this step was still open on: a listing that did not
//! run to completion used to yield an empty remainder, so `remaining_objects=0`
//! could have meant "we stopped looking". It cannot now — a truncated page
//! without a usable cursor, a repeated cursor, or a missing `IsTruncated` answer
//! is an error, and `CleanupReport::ok()` refuses a report whose listing was
//! incomplete. This step exits non-zero rather than reporting zero residue it
//! never finished measuring.
//!
//! Everything is scoped to `plan0014-wave4/<WAVE4_S3_RUN_ID>/`. Nothing here
//! lists, deletes or touches a key outside that prefix, and nothing deletes a
//! bucket on the protected list.

mod support;

use support::{
    cleanup_run_prefix, env, key_under_prefix, maybe_delete_bucket, port_client, report_and_fail,
    run_id, run_prefix, sdk_client, send_probe_objects, sentinel_prefix, spawn_case,
    write_sentinel, ChildRun,
};

/// Run the cleanup step in this process. The parent tests below invoke it
/// through a child so they can assert on the exit code, which is the property
/// CI actually depends on.
async fn final_cleanup_step() {
    // Intent first: a request to delete a bucket the workflow shares with every
    // other job is refused before a single object is touched.
    support::refuse_protected_bucket_deletion();
    let report = cleanup_run_prefix().await;
    report_and_fail(&report);
    maybe_delete_bucket().await;
}

/// The child entry point that stands in for the CI cleanup step.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn child_final_cleanup_step() {
    final_cleanup_step().await;
}

/// The cleanup step, run for real against the provider.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn final_cleanup_leaves_no_objects_and_no_incomplete_uploads() {
    // Leave something to clean up so the assertions below are about a cleanup
    // that did work, not about a prefix that happened to be empty.
    let client = port_client();
    send_probe_objects(
        &client,
        &run_prefix(),
        "prefill",
        support::more_than_a_page(),
    )
    .await;
    let before = support::list_all_keys(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .expect("list before cleanup");
    assert!(
        !before.is_empty(),
        "the prefill wrote nothing under {}, so this test would prove nothing",
        run_prefix()
    );

    final_cleanup_step().await;

    let objects = support::list_all_keys(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .expect("list after cleanup");
    let uploads =
        support::list_all_incomplete_uploads(&sdk_client(), &support::bucket(), &run_prefix())
            .await
            .expect("list uploads after cleanup");
    assert!(
        objects.is_empty(),
        "objects left under the run prefix: {objects:?}"
    );
    assert!(
        uploads.is_empty(),
        "incomplete multipart uploads left under the run prefix: {uploads:?}"
    );
}

/// A missing, empty or whitespace-only run id must stop the step. Without the
/// guard the prefix degenerates to `plan0014-wave4/`, which is every other
/// lane run's scope.
#[test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
fn cleanup_refuses_to_run_without_a_run_id() {
    for (label, value) in [
        ("unset", None),
        ("empty", Some("")),
        ("whitespace", Some("   ")),
        ("path-shaped", Some("../elsewhere")),
    ] {
        let run: ChildRun = spawn_case("child_final_cleanup_step", &[("WAVE4_S3_RUN_ID", value)]);
        assert!(
            !run.succeeded(),
            "cleanup accepted a {label} run id and exited with {:?}\n{}",
            run.code,
            run.output
        );
        assert!(
            run.output.contains("WAVE4_S3_RUN_ID"),
            "cleanup with a {label} run id failed for some other reason\n{}",
            run.output
        );
    }
}

/// A cleanup failure must be a cleanup failure.
///
/// The capability discovery in the contract lane deliberately records provider
/// answers instead of asserting them, and that discipline must not leak into
/// cleanup: an operation that could not be performed has to fail the step. The
/// injected fault here is a flag the cleanup engine reads, so the property
/// under test — "residue or failure, step exits non-zero" — is exercised
/// without depending on a provider misbehaving on schedule.
#[test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
fn a_cleanup_failure_fails_the_step_and_is_not_reported_as_success() {
    let failing = spawn_case(
        "child_final_cleanup_step",
        &[("WAVE4_S3_CLEANUP_SIMULATE_FAILURE", Some("true"))],
    );
    assert!(
        !failing.succeeded(),
        "the cleanup step exited {} despite an injected cleanup failure — \
         cleanup success would have masked it\n{}",
        failing.code.unwrap_or(-1),
        failing.output
    );
    assert!(
        failing.output.contains("INJECTED FAULT"),
        "the failing step did not report why it failed\n{}",
        failing.output
    );

    let passing = spawn_case(
        "child_final_cleanup_step",
        &[("WAVE4_S3_CLEANUP_SIMULATE_FAILURE", None)],
    );
    assert!(
        passing.succeeded(),
        "the same step without the injected fault must pass, so the assertion above \
         is about the fault and not about the harness\n{}",
        passing.output
    );
}

/// CI's bucket is shared by every job in the workflow.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn cleanup_never_deletes_a_protected_bucket() {
    let protected = support::PROTECTED_BUCKETS[0];
    // Whether the shared bucket happens to exist on this provider is not the
    // point — the point is that the step cannot change that.
    let existed_before = sdk_client()
        .head_bucket()
        .bucket(protected)
        .send()
        .await
        .is_ok();
    let run = spawn_case(
        "child_final_cleanup_step",
        &[
            ("WAVE4_S3_DELETE_BUCKET", Some("true")),
            ("WAVE4_S3_BUCKET", Some(protected)),
        ],
    );
    assert!(
        !run.succeeded(),
        "cleanup deleted, or was prepared to delete, the shared bucket {protected}\n{}",
        run.output
    );
    assert!(
        run.output.contains("protected list"),
        "the step did not refuse because the bucket is protected\n{}",
        run.output
    );
    // Refusal is not enough: the bucket's existence must be unchanged.
    let exists_after = sdk_client()
        .head_bucket()
        .bucket(protected)
        .send()
        .await
        .is_ok();
    assert_eq!(
        existed_before, exists_after,
        "the protected bucket {protected} was created or removed by the cleanup step"
    );
}

/// Cleanup must not wander outside the scope it was given.
#[tokio::test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
async fn cleanup_leaves_objects_outside_the_run_prefix_untouched() {
    let sentinel_key = write_sentinel("plan0014 wave 4 sentinel").await;
    let client = port_client();
    send_probe_objects(
        &client,
        &run_prefix(),
        "inside",
        support::more_than_a_page(),
    )
    .await;

    final_cleanup_step().await;

    assert_eq!(
        support::read_sentinel(&sentinel_key).await,
        "plan0014 wave 4 sentinel",
        "cleanup changed an object outside its prefix"
    );
    let leftovers = support::list_all_keys(&sdk_client(), &support::bucket(), &run_prefix())
        .await
        .expect("list after cleanup");
    assert!(leftovers.is_empty(), "run prefix still holds {leftovers:?}");
    let sentinels = support::list_all_keys(&sdk_client(), &support::bucket(), &sentinel_prefix())
        .await
        .expect("list sentinels");
    assert_eq!(
        sentinels.len(),
        1,
        "the sentinel outside the prefix should be the only object under {}",
        sentinel_prefix()
    );
    sdk_client()
        .delete_object()
        .bucket(support::bucket())
        .key(&sentinel_key)
        .send()
        .await
        .expect("remove the sentinel after the assertion");
}

/// Print the scope this run owns, for the evidence log.
#[test]
#[ignore = "requires a running S3-compatible provider (WAVE4_S3_* environment)"]
fn report_the_cleanup_scope() {
    println!(
        "CLEANUP SCOPE bucket={} prefix={} page_size={} endpoint={}",
        env("WAVE4_S3_BUCKET"),
        run_prefix(),
        support::page_size(),
        env("WAVE4_S3_ENDPOINT"),
    );
    println!("CLEANUP SCOPE run_id={}", run_id());
    println!(
        "CLEANUP SCOPE key_under_prefix_example={}",
        key_under_prefix("example.bin")
    );
}
