#![allow(clippy::expect_used, clippy::too_many_lines)]

use chrono::Utc;
use data_integrity::FindingStatus;
use data_repair::{
    RepairCommand, RepairPersistencePort, RepairRun, RepairRunStatus, RepairStep, RepairStepStatus,
    RepairTarget,
};
use document_processing_postgres::PostgresProcessingStore;
use governance_worker::{GovernanceWorker, RepairWorker};
use runtime_governance::processing_repairs::ProcessingRepairRegistry;
use runtime_governance_postgres::PostgresGovernanceStore;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires CI PostgreSQL and the shared migration catalog"]
async fn postgres_scan_and_requeue_repair_are_durable() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for the PostgreSQL governance E2E");
    let pool = PgPool::connect(&database_url).await.expect("postgres pool");

    let tenant_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query("INSERT INTO documents (id,tenant_id,original_filename,content_type,object_key,status,version,size_bytes,created_by,created_at,updated_at,content_revision) VALUES ($1,$2,'fixture.txt','text/plain','fixture-key','active',1,12,$3,$4,$4,1)")
        .bind(document_id)
        .bind(tenant_id)
        .bind(actor_id)
        .bind(now)
        .execute(&pool)
        .await
        .expect("document fixture");
    sqlx::query("INSERT INTO document_processing_jobs (id,tenant_id,document_id,content_revision,request_key,status,current_step,attempt_count,max_attempts,next_attempt_at,version,created_by,created_at,updated_at) VALUES ($1,$2,$3,1,'governance-fixture','waiting_for_ai','extract_fields',0,3,$4,1,$5,$4,$4)")
        .bind(job_id)
        .bind(tenant_id)
        .bind(document_id)
        .bind(now)
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("job fixture");
    sqlx::query("INSERT INTO document_processing_steps (job_id,tenant_id,step_kind,status,attempt_number,finished_at,checkpoint_json,created_at,updated_at) VALUES ($1,$2,'extract_text','succeeded',0,$3,$4,$3,$3)")
        .bind(job_id)
        .bind(tenant_id)
        .bind(now)
        .bind(serde_json::json!({
            "content_hash": "hash-fixture",
            "content_revision": 1,
            "byte_count": 12,
            "line_count": 1,
            "character_count": 12,
            "text_artifact_reference": "processing/fixture/text"
        }))
        .execute(&pool)
        .await
        .expect("text checkpoint fixture");

    let governance = Arc::new(PostgresGovernanceStore::new(pool.clone()));
    let scanner = GovernanceWorker::new(Arc::clone(&governance), Arc::clone(&governance))
        .expect("register integrity rules");
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
        .any(|finding| finding.rule_id() == "PROC-INT-001"));
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.rule_id() == "PROC-INT-001")
        .unwrap_or_else(|| unreachable!());
    let finding_id = finding.id();

    let run = RepairRun::new(
        Uuid::new_v4(),
        tenant_id,
        finding_id,
        RepairCommand {
            idempotency_key: "postgres-governance-requeue-1".to_string(),
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
        },
        RepairRunStatus::Queued,
        actor_id,
        now,
    )
    .expect("valid repair run");
    governance.save_run(&run).await.expect("save repair run");
    let step = RepairStep::new(
        Uuid::new_v4(),
        run.id(),
        finding_id,
        RepairStepStatus::Queued,
        now,
    )
    .expect("valid repair step");
    governance.save_step(&step).await.expect("save repair step");

    let processing = Arc::new(PostgresProcessingStore::new(pool.clone()));
    let worker = RepairWorker {
        persistence: Arc::clone(&governance),
        handlers: Arc::new(ProcessingRepairRegistry::new(processing)),
        rule_registry: None,
        worker_id: "postgres-governance-worker".to_string(),
        lease_duration_secs: 60,
        heartbeat_seconds: 5,
        max_attempts: 3,
    };
    assert!(worker.execute_one().await.expect("execute repair"));
    let finding = governance
        .load_finding(finding_id)
        .await
        .expect("read repaired finding")
        .expect("repaired finding exists");
    assert_eq!(finding.status(), FindingStatus::Repaired);

    let task_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_ai_tasks WHERE tenant_id=$1 AND job_id=$2 AND status='queued'",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("read restored task");
    assert_eq!(task_count, 1);
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE tenant_id=$1 AND action='document_processing.ai_task_requeued'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("read repair audit");
    assert_eq!(audit_count, 1);
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id=$1 AND event_type='document.processing.waiting-for-ai.v1'",
    )
    .bind(tenant_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("read repair outbox");
    assert_eq!(outbox_count, 1);
    let ledger_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM data_repair_events WHERE repair_run_id=$1")
            .bind(run.id())
            .fetch_one(&pool)
            .await
            .expect("read repair ledger");
    assert_eq!(ledger_count, 1);
}
