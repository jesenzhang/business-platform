#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Wave 3 (PLAN-0014) audit chain conformance for `SQLite` deployments.
//!
//! A `SQLite` business-api deployment serves `/admin/audit-events` through
//! `audit_sqlite::SqliteAuditStore` while document creation writes its audit
//! row through this crate's unit of work. These tests pin that both write
//! regimes land in the same tenant chain format (A-01/A-02) and that the
//! tamper-evidence guarantees stated in `AUDIT_RETENTION_AND_TAMPER_EVIDENCE`
//! hold exactly: mutation and interior deletion are detected, tail truncation
//! is not detectable without a trusted external checkpoint.

use audit::{
    AuditAction, AuditActor, AuditActorType, AuditChainScope, AuditEvent, AuditQuery,
    AuditQueryRequest, AuditResource, AuditResult,
};
use audit_sqlite::{append_sqlite, SqliteAuditStore};
use document::domain::DocumentMetadata;
use document::ports::{CreateDocumentUnitOfWork, PersistNewDocument};
use document_sqlite::{SqliteCreateDocumentUnitOfWork, MIGRATOR};
use sqlx::migrate::MigrateDatabase;
use uuid::Uuid;

fn file_url() -> String {
    let path = std::env::temp_dir().join(format!("business-platform-{}.db", Uuid::now_v7()));
    format!("sqlite://{}", path.to_string_lossy().replace('\\', "/"))
}

/// Mirror the business-api `SQLite` deployment (`composition.rs`): the
/// document catalog pool plus the processing catalog migrations own the
/// shared `audit_events` schema in one local file.
async fn setup() -> (String, sqlx::SqlitePool) {
    let url = file_url();
    assert!(sqlx::Sqlite::create_database(&url).await.is_ok());
    let pool = document_sqlite::connect(&url, 4).await;
    let Ok(pool) = pool else {
        unreachable!("temporary SQLite pool must connect");
    };
    let migration = MIGRATOR.run(&pool).await;
    assert!(
        migration.is_ok(),
        "document SQLite migrations must apply: {migration:?}"
    );
    let migration = document_processing_sqlite::run_migrations(&pool).await;
    assert!(
        migration.is_ok(),
        "processing SQLite migrations must apply: {migration:?}"
    );
    (url, pool)
}

fn persist_command(tenant_id: Uuid, actor_id: Uuid) -> PersistNewDocument {
    let document = DocumentMetadata::create(
        tenant_id,
        "chain.pdf".to_string(),
        "application/pdf".to_string(),
        "chain.pdf".to_string(),
        actor_id,
        Some(1),
    );
    let Ok(document) = document else {
        unreachable!("fixture must be valid");
    };
    PersistNewDocument {
        document,
        idempotency_key: "chain-key".to_string(),
        request_fingerprint: "chain-fingerprint".to_string(),
        fingerprint_version: 1,
        initial_revision_sha256: None,
    }
}

fn audit_event(tenant_id: Uuid, resource_id: &str) -> AuditEvent {
    let event = AuditEvent::new(
        Uuid::now_v7(),
        tenant_id,
        AuditActor {
            actor_type: AuditActorType::Service,
            actor_id: Uuid::now_v7(),
        },
        AuditAction::new("governance.probe").unwrap_or_else(|_| unreachable!()),
        AuditResource::new("probe", resource_id).unwrap_or_else(|_| unreachable!()),
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
    );
    event.unwrap_or_else(|_| unreachable!("fixture event must be valid"))
}

fn query(tenant_id: Uuid) -> AuditQueryRequest {
    AuditQueryRequest {
        tenant_id,
        limit: 50,
        ..AuditQueryRequest::default()
    }
}

fn scope(tenant_id: Uuid) -> AuditChainScope {
    AuditChainScope {
        tenant_id,
        from: None,
        to: None,
    }
}

fn temp_file(url: &str) {
    let _ = std::fs::remove_file(url.trim_start_matches("sqlite://"));
}

#[tokio::test]
async fn document_created_audit_row_is_chain_verifiable_and_readable() {
    let (url, pool) = setup().await;
    let tenant_id = Uuid::now_v7();
    let command = persist_command(tenant_id, Uuid::now_v7());
    let uow = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    let result = uow.execute(command.clone()).await;
    assert!(result.is_ok(), "document create must succeed: {result:?}");

    let store = SqliteAuditStore::new(pool.clone());
    let page = store.list(query(tenant_id)).await;
    assert!(
        page.is_ok(),
        "audit list must read document-owned audit rows: {page:?}"
    );
    let page = page.unwrap_or_else(|_| unreachable!());
    assert_eq!(page.items.len(), 1);

    let verification = store.verify_chain(scope(tenant_id)).await;
    assert!(
        verification.is_ok(),
        "verify_chain must succeed on document-owned audit rows: {verification:?}"
    );
    let verification = verification.unwrap_or_else(|_| unreachable!());
    assert!(
        verification.valid && verification.verified_count == 1,
        "document audit row must be a protected chain member: {verification:?}"
    );

    // An append through the canonical audit adapter must continue the same chain.
    append_sqlite(&pool, &audit_event(tenant_id, "probe-1"))
        .await
        .expect("audit append must succeed");
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("chain must verify after append");
    assert!(
        verification.valid && verification.verified_count == 2,
        "document and audit rows must share one tenant chain: {verification:?}"
    );
    let page = store
        .list(query(tenant_id))
        .await
        .expect("audit list must succeed after append");
    assert_eq!(page.items.len(), 2);

    // Idempotent replay through the same writer must keep succeeding.
    let replay = uow.execute(command).await;
    assert!(replay.is_ok(), "same-key replay must succeed: {replay:?}");
    let replay = replay.unwrap_or_else(|_| unreachable!());
    assert!(replay.replayed);

    pool.close().await;
    temp_file(&url);
}

#[tokio::test]
async fn legacy_null_chain_rows_stay_readable_and_are_counted_as_legacy() {
    let (url, pool) = setup().await;
    let tenant_id = Uuid::now_v7();
    // Exactly the pre-fix writer shape: chain columns NULL, legacy hashes set.
    sqlx::query(
        "INSERT INTO audit_events (id, tenant_id, action, resource_type, resource_id,
            details, created_at, occurred_at, operation_id, actor_type, actor_id,
            result, changed_fields, schema_version, previous_hash, record_hash)
         VALUES (?1, ?2, 'document.created', 'document', ?3, '{}', ?4, ?4, ?5,
            'user', ?6, 'succeeded', '[]', 'audit.v1', ?7, ?7)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(tenant_id.to_string())
    .bind(Uuid::now_v7().to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(Uuid::now_v7().to_string())
    .bind(Uuid::now_v7().to_string())
    .bind("sha256:legacy")
    .execute(&pool)
    .await
    .expect("legacy audit row must insert");

    let store = SqliteAuditStore::new(pool.clone());
    let page = store.list(query(tenant_id)).await;
    assert!(
        page.is_ok(),
        "legacy null-chain rows must remain readable: {page:?}"
    );
    let page = page.unwrap_or_else(|_| unreachable!());
    assert_eq!(page.items.len(), 1);

    append_sqlite(&pool, &audit_event(tenant_id, "probe-1"))
        .await
        .expect("audit append must succeed over legacy rows");
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("chain must verify over legacy boundary");
    assert!(
        verification.valid && verification.legacy_count == 1 && verification.verified_count == 1,
        "legacy rows must be an explicit unverified boundary, new rows protected: {verification:?}"
    );

    pool.close().await;
    temp_file(&url);
}

#[tokio::test]
async fn chain_detects_mutation_and_interior_deletion_but_not_tail_truncation() {
    let (url, pool) = setup().await;
    let tenant_id = Uuid::now_v7();
    let store = SqliteAuditStore::new(pool.clone());
    let mut ids = Vec::new();
    for index in 0..3 {
        let event = audit_event(tenant_id, &format!("probe-{index}"));
        ids.push(event.id());
        append_sqlite(&pool, &event)
            .await
            .expect("audit append must succeed");
    }
    let verification = store
        .verify_chain(scope(tenant_id))
        .await
        .expect("fresh chain must verify");
    assert!(verification.valid && verification.verified_count == 3);

    // Row mutation (details swap) must break the chain at the mutated row.
    sqlx::query("UPDATE audit_events SET details = ?1 WHERE id = ?2")
        .bind(serde_json::json!({"probe": "tampered"}).to_string())
        .bind(ids[1].to_string())
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
    sqlx::query("DELETE FROM audit_events WHERE id = ?1")
        .bind(ids[1].to_string())
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
    // Honest limitation (AUDIT_RETENTION_AND_TAMPER_EVIDENCE): without an
    // external checkpoint, removing only the tail leaves a self-consistent chain.
    sqlx::query("DELETE FROM audit_events WHERE id = ?1")
        .bind(ids[2].to_string())
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

    pool.close().await;
    temp_file(&url);
}

#[tokio::test]
async fn standalone_document_catalog_supports_create_and_replay() {
    let pool = document_sqlite::connect("sqlite::memory:", 1)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let uow = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    let command = persist_command(Uuid::now_v7(), Uuid::now_v7());
    assert!(uow.execute(command.clone()).await.is_ok());
    assert!(uow.execute(command.clone()).await.unwrap().replayed);
    let verification = SqliteAuditStore::new(pool.clone())
        .verify_chain(scope(command.document.tenant_id()))
        .await
        .unwrap();
    assert!(verification.valid);
    assert_eq!(verification.verified_count, 1);
    MIGRATOR.run(&pool).await.unwrap();
    pool.close().await;
}

#[tokio::test]
async fn contradictory_chain_metadata_is_rejected_without_legacy_downgrade() {
    let (url, pool) = setup().await;
    for mutation in [
        "chain_version = 0",
        "chain_version = 2",
        "stream_sequence = NULL",
        "previous_hash = NULL",
        "record_hash = NULL",
    ] {
        let tenant = Uuid::now_v7();
        append_sqlite(&pool, &audit_event(tenant, "protected"))
            .await
            .unwrap();
        sqlx::query(&format!(
            "UPDATE audit_events SET {mutation} WHERE tenant_id = ?1"
        ))
        .bind(tenant.to_string())
        .execute(&pool)
        .await
        .unwrap();
        let result = SqliteAuditStore::new(pool.clone())
            .verify_chain(scope(tenant))
            .await;
        assert!(
            result.is_err() || result.as_ref().is_ok_and(|r| !r.valid),
            "corrupt metadata {mutation} accepted: {result:?}"
        );
    }
    pool.close().await;
    temp_file(&url);
}

#[tokio::test]
async fn old_composed_catalog_upgrade_preserves_verified_rows() {
    let pool = document_sqlite::connect("sqlite::memory:", 1)
        .await
        .unwrap();
    let old = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            MIGRATOR.iter().filter(|m| m.version < 6).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    document_processing_sqlite::run_migrations(&pool)
        .await
        .unwrap();
    let tenant = Uuid::now_v7();
    append_sqlite(&pool, &audit_event(tenant, "before-upgrade"))
        .await
        .unwrap();
    let hash_before: String =
        sqlx::query_scalar("SELECT record_hash FROM audit_events WHERE tenant_id = ?1")
            .bind(tenant.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    document_processing_sqlite::run_migrations(&pool)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let hash_after: String =
        sqlx::query_scalar("SELECT record_hash FROM audit_events WHERE tenant_id = ?1")
            .bind(tenant.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(hash_before, hash_after);
    let command = persist_command(tenant, Uuid::now_v7());
    let uow = SqliteCreateDocumentUnitOfWork::new(pool.clone());
    assert!(uow.execute(command.clone()).await.is_ok());
    assert!(uow.execute(command).await.unwrap().replayed);
    let v = SqliteAuditStore::new(pool.clone())
        .verify_chain(scope(tenant))
        .await
        .unwrap();
    assert!(v.valid);
    assert_eq!(v.verified_count, 2);
    pool.close().await;
}

#[tokio::test]
async fn old_document_only_rows_remain_unverified_after_upgrade() {
    let pool = document_sqlite::connect("sqlite::memory:", 1)
        .await
        .unwrap();
    let old = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            MIGRATOR.iter().filter(|m| m.version < 6).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    let tenant = Uuid::now_v7();
    for _ in 0..3 {
        sqlx::query("INSERT INTO audit_events (id,tenant_id,action,resource_type,resource_id,created_at,occurred_at,previous_hash,record_hash) VALUES (?1,?2,'document.created','document','legacy-document',?3,?3,'old','old')")
            .bind(Uuid::now_v7().to_string()).bind(tenant.to_string())
            .bind(chrono::Utc::now().to_rfc3339()).execute(&pool).await.unwrap();
    }
    // Failure in the backfill must roll back newly adopted columns and must
    // not mark migration 6 applied. The same database must be retryable.
    sqlx::query("CREATE TRIGGER reject_backfill BEFORE UPDATE ON audit_events BEGIN SELECT RAISE(ABORT, 'test-migration-rollback'); END")
        .execute(&pool).await.unwrap();
    assert!(MIGRATOR.run(&pool).await.is_err());
    let column_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('audit_events') WHERE name = 'stream_sequence'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(column_count, 0);
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, 5);
    sqlx::query("DROP TRIGGER reject_backfill")
        .execute(&pool)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    append_sqlite(&pool, &audit_event(tenant, "new"))
        .await
        .unwrap();
    let v = SqliteAuditStore::new(pool.clone())
        .verify_chain(scope(tenant))
        .await
        .unwrap();
    assert!(v.valid);
    assert_eq!((v.legacy_count, v.verified_count), (3, 1));
    sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        MIGRATOR.run(&pool).await,
        Err(sqlx::migrate::MigrateError::VersionMismatch(1))
    ));
    pool.close().await;
}
