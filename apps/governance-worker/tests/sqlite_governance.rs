#![allow(clippy::expect_used, clippy::too_many_lines)]

use chrono::Utc;
use data_integrity::FindingStatus;
use data_repair::{
    RepairCommand, RepairPersistencePort, RepairRun, RepairRunStatus, RepairStep, RepairStepStatus,
    RepairTarget,
};
use document_processing_sqlite::{
    run_migrations as run_processing_migrations, SqliteProcessingStore,
};
use governance_worker::{GovernanceWorker, RepairWorker};
use runtime_governance::processing_repairs::ProcessingRepairRegistry;
use runtime_governance_sqlite::SqliteGovernanceStore;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_scan_and_requeue_repair_are_durable() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite pool");
    document_sqlite::MIGRATOR
        .run(&pool)
        .await
        .expect("document migrations");
    run_processing_migrations(&pool)
        .await
        .expect("processing migrations");

    let tenant_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO documents (id,tenant_id,original_filename,content_type,object_key,status,version,size_bytes,created_by,created_at,updated_at,content_revision) VALUES (?1,?2,'fixture.txt','text/plain','fixture-key','active',1,12,?3,?4,?4,1)")
        .bind(document_id.to_string())
        .bind(tenant_id.to_string())
        .bind(actor_id.to_string())
        .bind(&now)
        .execute(&pool)
        .await
        .expect("document fixture");
    sqlx::query("INSERT INTO document_processing_jobs (id,tenant_id,document_id,content_revision,request_key,status,current_step,attempt_count,max_attempts,next_attempt_at,version,created_by,created_at,updated_at) VALUES (?1,?2,?3,1,'governance-fixture','waiting_for_ai','extract_fields',0,3,?4,1,?5,?4,?4)")
        .bind(job_id.to_string())
        .bind(tenant_id.to_string())
        .bind(document_id.to_string())
        .bind(&now)
        .bind(actor_id.to_string())
        .execute(&pool)
        .await
        .expect("job fixture");
    sqlx::query("INSERT INTO document_processing_steps (job_id,tenant_id,step_kind,status,attempt_number,finished_at,checkpoint_json,created_at,updated_at) VALUES (?1,?2,'extract_text','succeeded',0,?3,?4,?3,?3)")
        .bind(job_id.to_string())
        .bind(tenant_id.to_string())
        .bind(&now)
        .bind(serde_json::json!({
            "content_hash": "hash-fixture",
            "content_revision": 1,
            "byte_count": 12,
            "line_count": 1,
            "character_count": 12,
            "text_artifact_reference": "processing/fixture/text"
        }).to_string())
        .execute(&pool)
        .await
        .expect("text checkpoint fixture");

    let governance = Arc::new(SqliteGovernanceStore::new(pool.clone()));
    let scanner = GovernanceWorker::new(Arc::clone(&governance), Arc::clone(&governance));
    let report = scanner
        .run_explicit_scan(
            data_integrity::IntegrityScanScope {
                tenant_id: Some(tenant_id),
                resource_type: Some("processing_job".to_string()),
                resource_id: Some(job_id.to_string()),
            },
            actor_id,
        )
        .await
        .expect("integrity scan");
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.rule_id == "PROC-INT-001"));
    let finding_id = report
        .findings
        .iter()
        .find(|finding| finding.rule_id == "PROC-INT-001")
        .map_or_else(|| unreachable!(), |finding| finding.id);

    let command = RepairCommand {
        idempotency_key: "sqlite-governance-requeue-1".to_string(),
        tenant_id,
        integrity_finding_id: finding_id,
        target: RepairTarget {
            resource_type: "processing_job".to_string(),
            resource_id: job_id.to_string(),
            expected_resource_version: Some(1),
        },
        repair_type: "requeue_missing_ai_task.v1".to_string(),
        repair_version: 1,
        requested_by: actor_id,
        reason: "restore the missing durable AI task".to_string(),
        batch_limit: 1,
    };
    let run = RepairRun {
        id: Uuid::new_v4(),
        tenant_id,
        finding_id,
        command,
        status: RepairRunStatus::Queued,
        created_by: actor_id,
        approved_by: None,
        approval_note: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 0,
    };
    let step = RepairStep {
        id: Uuid::new_v4(),
        run_id: run.id,
        finding_id,
        status: RepairStepStatus::Queued,
        attempt_count: 0,
        checkpoint: None,
        lease_owner: None,
        lease_token: None,
        fence_version: 0,
        lease_expires_at: None,
        next_attempt_at: Utc::now(),
    };
    governance
        .create_repair_run(&run, &step)
        .await
        .expect("create repair run and step atomically");

    // Model a crashed worker: the first lease expires, a replacement claims a
    // higher fence, and the stale worker's completion is rejected.
    let crash_run = RepairRun {
        id: Uuid::new_v4(),
        tenant_id,
        finding_id,
        command: RepairCommand {
            idempotency_key: "sqlite-governance-crash-recovery".to_string(),
            tenant_id,
            integrity_finding_id: finding_id,
            target: RepairTarget {
                resource_type: "processing_job".to_string(),
                resource_id: job_id.to_string(),
                expected_resource_version: Some(1),
            },
            repair_type: "requeue_missing_ai_task.v1".to_string(),
            repair_version: 1,
            requested_by: actor_id,
            reason: "exercise lease recovery".to_string(),
            batch_limit: 1,
        },
        status: RepairRunStatus::Queued,
        created_by: actor_id,
        approved_by: None,
        approval_note: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 0,
    };
    governance
        .save_run(&crash_run)
        .await
        .expect("save crash-recovery run");
    let crash_step = RepairStep {
        id: Uuid::new_v4(),
        run_id: crash_run.id,
        finding_id,
        status: RepairStepStatus::Queued,
        attempt_count: 0,
        checkpoint: None,
        lease_owner: None,
        lease_token: None,
        fence_version: 0,
        lease_expires_at: None,
        next_attempt_at: Utc::now() - chrono::Duration::seconds(1),
    };
    governance
        .save_step(&crash_step)
        .await
        .expect("save crash-recovery step");
    let crashed_at = Utc::now();
    let stale = governance
        .claim_step("crashed-worker", crashed_at, 1)
        .await
        .expect("claim crashed step")
        .expect("crashed step claimed");
    let reclaimed = governance
        .claim_step(
            "replacement-worker",
            crashed_at + chrono::Duration::seconds(2),
            60,
        )
        .await
        .expect("reclaim crashed step")
        .expect("expired step reclaimed");
    assert!(reclaimed.fence_version > stale.fence_version);
    let mut stale_completion = stale.clone();
    stale_completion.status = RepairStepStatus::Succeeded;
    assert!(matches!(
        governance
            .save_step_fenced(&stale_completion, stale.fence_version)
            .await,
        Err(data_repair::RepairError::LeaseLost)
    ));
    let mut wrong_token = reclaimed.clone();
    wrong_token.status = RepairStepStatus::Succeeded;
    wrong_token.lease_token = Some("wrong-token".to_string());
    assert!(matches!(
        governance
            .save_step_fenced(&wrong_token, reclaimed.fence_version)
            .await,
        Err(data_repair::RepairError::LeaseLost)
    ));
    // Remove the recovery fixture from the claim queue; the real repair run
    // below remains the only executable step.
    let mut recovered_cleanup = reclaimed;
    recovered_cleanup.status = RepairStepStatus::Failed;
    recovered_cleanup.lease_owner = None;
    recovered_cleanup.lease_token = None;
    governance
        .save_step(&recovered_cleanup)
        .await
        .expect("cleanup recovery fixture");

    let processing = Arc::new(SqliteProcessingStore::new(pool.clone()));
    let worker = RepairWorker {
        persistence: Arc::clone(&governance),
        handlers: Arc::new(ProcessingRepairRegistry::new(processing)),
        rule_registry: None,
        worker_id: "sqlite-governance-worker".to_string(),
        lease_duration_secs: 60,
        heartbeat_seconds: 5,
    };
    assert!(worker.execute_one().await.expect("execute repair"));

    let finding = governance
        .load_finding(finding_id)
        .await
        .expect("read repaired finding")
        .expect("repaired finding exists");
    assert_eq!(finding.status, FindingStatus::Repaired);

    let task_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_ai_tasks WHERE tenant_id=?1 AND job_id=?2 AND status='queued'",
    )
    .bind(tenant_id.to_string())
    .bind(job_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("read restored task");
    assert_eq!(task_count, 1);
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE tenant_id=?1 AND action='document_processing.ai_task_requeued'",
    )
    .bind(tenant_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("read repair audit");
    assert_eq!(audit_count, 1);
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id=?1 AND event_type='document.processing.waiting-for-ai.v1'",
    )
    .bind(tenant_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("read repair outbox");
    assert_eq!(outbox_count, 1);
    let run_status: String = sqlx::query_scalar("SELECT status FROM data_repair_runs WHERE id=?1")
        .bind(run.id.to_string())
        .fetch_one(&pool)
        .await
        .expect("read repair run");
    assert_eq!(run_status, "succeeded");
    let ledger_count: i64 =
        sqlx::query("SELECT COUNT(*) AS count FROM data_repair_events WHERE repair_run_id=?1")
            .bind(run.id.to_string())
            .fetch_one(&pool)
            .await
            .expect("read ledger")
            .get("count");
    assert_eq!(ledger_count, 1);
}
