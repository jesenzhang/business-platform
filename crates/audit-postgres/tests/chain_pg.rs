//! Wave 3 (PLAN-0014) audit chain conformance on the production backend.
//! Requires running `PostgreSQL` against a dedicated test database. Run with:
//!   cargo test -p audit-postgres --test `chain_pg` --all-features -- --include-ignored --test-threads=1
//!
//! Pins the guarantees stated in `AUDIT_RETENTION_AND_TAMPER_EVIDENCE` on the
//! `PostgreSQL` authority: per-tenant serialized appends form one verified
//! chain under concurrency; mutation and interior deletion are detected; tail
//! truncation is not detectable without a trusted external checkpoint; and
//! the migration-013/015 legacy boundary (`chain_version` = 0) stays readable
//! and unverified.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use audit::{
    AuditAction, AuditActor, AuditActorType, AuditChainScope, AuditEvent, AuditQuery,
    AuditQueryRequest, AuditResource, AuditResult,
};
use audit_postgres::{append_postgres, PostgresAuditStore};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

/// Helper: connect to `PostgreSQL` using `DATABASE_URL` or default.
async fn setup_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/business_platform_test".to_string()
    });

    PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .expect("failed to connect to PostgreSQL")
}

/// Helper: run migrations to ensure schema is up to date.
///
/// The catalog comes from `runtime_migration::MIGRATOR`, the single embed point of the
/// shared baseline: a second `sqlx::migrate!("../../migrations")` here would embed a
/// second copy of the same catalog (PLAN-0014 Wave 5B design §11.2 item 11, review
/// `2026-09-30-wave5b-revision1-independent-review-and-s1-instructions.md` P-4).
async fn run_migrations(pool: &sqlx::PgPool) {
    runtime_migration::MIGRATOR
        .run(pool)
        .await
        .expect("failed to run migrations");
}

fn audit_event(tenant_id: Uuid, resource_id: &str) -> AuditEvent {
    AuditEvent::new(
        Uuid::now_v7(),
        tenant_id,
        AuditActor {
            actor_type: AuditActorType::Service,
            actor_id: Uuid::now_v7(),
        },
        AuditAction::new("governance.probe").expect("fixture action"),
        AuditResource::new("probe", resource_id).expect("fixture resource"),
        Uuid::now_v7(),
        None,
        None,
        None,
        None,
        AuditResult::Succeeded,
        None,
        None,
        None,
        vec!["probe".to_string()],
        serde_json::json!({"probe": resource_id}),
        "audit.v1",
        chrono::Utc::now(),
    )
    .expect("fixture event must be valid")
}

fn scope(tenant_id: Uuid) -> AuditChainScope {
    AuditChainScope {
        tenant_id,
        from: None,
        to: None,
    }
}

fn query(tenant_id: Uuid) -> AuditQueryRequest {
    AuditQueryRequest {
        tenant_id,
        limit: 200,
        ..AuditQueryRequest::default()
    }
}

/// Insert one row in exactly the shape migrations 013/015 leave behind for
/// pre-chain history: assigned tenant-local sequence, `chain_version` = 0,
/// NULL previous/record hash.
async fn insert_legacy_row(pool: &sqlx::PgPool, tenant_id: Uuid, stream_sequence: i64) -> Uuid {
    let id = Uuid::now_v7();
    let now = chrono::Utc::now();
    sqlx::query(
        r"
        INSERT INTO audit_events
            (id, tenant_id, action, resource_type, resource_id, details, trace_id,
             created_at, occurred_at, recorded_at, stream_sequence, chain_version,
             operation_id, actor_type, actor_id, correlation_id, causation_id,
             reason, result, failure_code, before_hash, after_hash, changed_fields,
             schema_version, previous_hash, record_hash)
        VALUES
            ($1, $2, 'document.created', 'document', $3, '{}'::jsonb, NULL,
             $4, $4, $4, $5, 0,
             $6, 'user', $7, NULL, NULL,
             NULL, 'succeeded', NULL, NULL, NULL, '[]'::jsonb,
             'audit.v1', NULL, NULL)
        ",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(Uuid::now_v7())
    .bind(now)
    .bind(stream_sequence)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(pool)
    .await
    .expect("legacy audit row must insert");
    id
}

async fn cleanup(pool: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query("DELETE FROM audit_events WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to cleanup");
}

/// Concurrent appends for one tenant serialize through the advisory lock and
/// must land as one complete, verified chain (no sequence collisions, no
/// broken links).
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn concurrent_appends_form_one_verified_tenant_chain() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;
    let tenant_id = Uuid::now_v7();

    let mut handles = Vec::new();
    for index in 0..10 {
        let pool = pool.clone();
        let event = audit_event(tenant_id, &format!("probe-{index}"));
        handles.push(tokio::spawn(
            async move { append_postgres(&pool, &event).await },
        ));
    }
    for handle in handles {
        let result = handle.await.expect("append task must not panic");
        assert!(result.is_ok(), "concurrent append must succeed: {result:?}");
    }

    let store = PostgresAuditStore::new(pool.clone());
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("chain must verify after concurrent appends");
    assert!(
        verification.valid && verification.verified_count == 10,
        "concurrent appends must form one contiguous verified chain: {verification:?}"
    );
    let page = store
        .list(query(tenant_id))
        .await
        .expect("audit list must succeed");
    assert_eq!(page.items.len(), 10);
    let mut sequences: Vec<i64> = page
        .items
        .iter()
        .map(audit::AuditEvent::stream_sequence)
        .collect();
    sequences.sort_unstable();
    assert_eq!(sequences, (1..=10).collect::<Vec<i64>>());

    cleanup(&pool, tenant_id).await;
}

/// Mutation and interior deletion must be reported; tail truncation must not
/// claim detection without a trusted checkpoint (honest-limitation pin).
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn chain_detects_mutation_and_interior_deletion_but_not_tail_truncation() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;
    let tenant_id = Uuid::now_v7();
    let store = PostgresAuditStore::new(pool.clone());

    let mut ids = Vec::new();
    for index in 0..3 {
        let event = audit_event(tenant_id, &format!("probe-{index}"));
        ids.push(event.id());
        append_postgres(&pool, &event)
            .await
            .expect("audit append must succeed");
    }
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("fresh chain must verify");
    assert!(verification.valid && verification.verified_count == 3);

    sqlx::query("UPDATE audit_events SET details = $1 WHERE id = $2 AND tenant_id = $3")
        .bind(serde_json::json!({"probe": "tampered"}))
        .bind(ids[1])
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("tampering query must run");
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("verify_chain must answer after tamper");
    assert!(
        !verification.valid && verification.first_broken_id == Some(ids[1]),
        "row mutation must be reported at the mutated row: {verification:?}"
    );

    sqlx::query("DELETE FROM audit_events WHERE id = $1 AND tenant_id = $2")
        .bind(ids[1])
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("delete must run");
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("verify_chain must answer after interior deletion");
    assert!(
        !verification.valid && verification.first_broken_id == Some(ids[2]),
        "interior deletion must surface as a broken link at the successor: {verification:?}"
    );

    sqlx::query("DELETE FROM audit_events WHERE id = $1 AND tenant_id = $2")
        .bind(ids[2])
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("tail delete must run");
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("verify_chain must answer after tail truncation");
    assert!(
        verification.valid && verification.verified_count == 1,
        "tail truncation is not detectable without a trusted checkpoint: {verification:?}"
    );

    cleanup(&pool, tenant_id).await;
}

/// The 013/015 unverified-legacy boundary must stay readable, be reported as
/// a legacy count (not silently trusted), and a new v1 append must continue
/// after it with a fresh genesis link.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn legacy_boundary_rows_stay_readable_and_new_chain_continues_after_them() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;
    let tenant_id = Uuid::now_v7();
    let legacy_id = insert_legacy_row(&pool, tenant_id, 1).await;

    let store = PostgresAuditStore::new(pool.clone());
    let page = store
        .list(query(tenant_id))
        .await
        .expect("legacy rows must remain readable");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id(), legacy_id);
    assert_eq!(
        page.items[0].chain_version(),
        0,
        "backfilled rows must report the legacy chain version"
    );

    append_postgres(&pool, &audit_event(tenant_id, "probe-1"))
        .await
        .expect("audit append must succeed after legacy rows");
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("chain must verify across the legacy boundary");
    assert!(
        verification.valid && verification.legacy_count == 1 && verification.verified_count == 1,
        "legacy rows are an explicit unverified boundary, new rows protected: {verification:?}"
    );

    cleanup(&pool, tenant_id).await;
}

/// Real `PostgreSQL` precision round trip: an event built from a nanosecond
/// `occurred_at` must be signed at microsecond precision, stored by `TIMESTAMPTZ`
/// without loss, and re-verify from the stored row. This is the property that
/// made every `PostgreSQL` chain row unverifiable before the precision fix, and
/// no `SQLite` test can observe it because `SQLite` stores RFC 3339 text.
#[tokio::test]
#[ignore = "requires running PostgreSQL"]
async fn nanosecond_input_verifies_after_a_postgres_precision_round_trip() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;
    let tenant_id = Uuid::now_v7();

    // A tail of 123456789 ns: the last three digits cannot survive TIMESTAMPTZ.
    let requested = chrono::DateTime::parse_from_rfc3339("2026-09-26T12:00:00.123456789Z")
        .expect("fixture instant")
        .with_timezone(&chrono::Utc);
    let event = AuditEvent::new(
        Uuid::now_v7(),
        tenant_id,
        AuditActor {
            actor_type: AuditActorType::Service,
            actor_id: Uuid::now_v7(),
        },
        AuditAction::new("governance.precision-probe").expect("fixture action"),
        AuditResource::new("probe", "precision").expect("fixture resource"),
        Uuid::now_v7(),
        None,
        None,
        None,
        None,
        AuditResult::Succeeded,
        None,
        None,
        None,
        vec!["probe".to_string()],
        serde_json::json!({"probe": "precision"}),
        "audit.v1",
        requested,
    )
    .expect("fixture event must be valid");
    // Normalization happens before signing, not after storage.
    assert_eq!(
        event.occurred_at().timestamp_subsec_nanos(),
        123_456_000,
        "a new event must be signed at the precision every backend can store"
    );

    append_postgres(&pool, &event)
        .await
        .expect("audit append must succeed");

    // Storage kept exactly what was signed for the business timestamp.
    // `append_postgres` assigns `recorded_at` from the adapter clock while signing
    // the row, so the pre-append event's `recorded_at` is deliberately not the
    // stored value; the property that matters is that the stored pair reproduces
    // the stored digest, which the repository check below proves.
    let stored: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT occurred_at, recorded_at FROM audit_events WHERE id = $1 AND tenant_id = $2",
    )
    .bind(event.id())
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("stored row must be readable");
    assert_eq!(
        stored.0,
        event.occurred_at(),
        "TIMESTAMPTZ must round trip the signed occurred_at"
    );
    assert_eq!(
        stored.0.timestamp_subsec_nanos(),
        123_456_000,
        "the stored business timestamp must equal the microsecond value that was signed"
    );
    assert_eq!(
        stored.1.timestamp_subsec_nanos() % 1_000,
        0,
        "the adapter-assigned recorded_at must be signed at stored precision"
    );

    // The repository read reproduces the digest written at append time.
    let store = PostgresAuditStore::new(pool.clone());
    let read_back = store
        .get(tenant_id, event.id())
        .await
        .expect("repository read must succeed")
        .expect("appended row must be readable");
    let signed = read_back
        .record_hash()
        .expect("v1 row must carry a record hash")
        .to_string();
    assert_eq!(
        audit::hash_record(&read_back).expect("hash must compute"),
        signed,
        "the digest recomputed from the stored row must equal the stored digest"
    );

    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("chain verification must run");
    assert!(
        verification.valid && verification.verified_count == 1,
        "a freshly appended row must verify on PostgreSQL: {verification:?}"
    );

    cleanup(&pool, tenant_id).await;
}
