#![allow(clippy::expect_used, clippy::too_many_lines, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use business_api::auth::AuthMiddlewareConfig;
use business_api::config::{
    AuthConfig, BusinessApiConfig, DatabaseBackend, DatabaseConfig, ObservabilityConfig,
    ServerConfig, StorageConfig,
};
use business_api::routes;
use business_api::state::{AppState, DocumentServices, PostgresReadinessProbe, StorageServices};
use bytes::Bytes;
use chrono::{Duration, Utc};
use document::application::{CreateDocumentCommand, CreateDocumentMetadata};
use document::domain::{DocumentMetadata, DocumentRepository, RepositoryError};
use document_postgres::PostgresCreateDocumentUnitOfWork;
use document_processing::ports::{
    ClassifiedProcessingFailure, ExecutionFence, ProcessingFailureDisposition,
    ProcessingJobCommandPort,
};
use document_processing::{
    Evidence, ProcessingArtifact, ProcessingExecutionUnitOfWork, ProcessingJob,
    ProcessingJobClaimPort, ProcessingRun,
};
use document_processing_postgres::PostgresProcessingStore;
use futures_util::stream;
use http_body_util::BodyExt;
use object_storage::{ObjectKey, ObjectStorageClient, S3Client};
use runtime_config::{RuntimeEnvironment, Secret, SecretUrl};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

async fn put_content(
    storage: &S3Client,
    key: &ObjectKey,
    content: Bytes,
    content_type: &str,
) -> String {
    let length = u64::try_from(content.len()).unwrap_or(u64::MAX);
    let sha256 = format!("{:x}", Sha256::digest(content.as_ref()));
    let mut metadata = BTreeMap::new();
    metadata.insert("sha256".to_string(), sha256.clone());
    storage
        .put_stream(
            key,
            Box::pin(stream::once(async move {
                Ok::<Bytes, object_storage::StorageError>(content)
            })),
            length,
            content_type,
            &metadata,
        )
        .await
        .expect("MinIO put must succeed");
    sha256
}

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for the PostgreSQL + MinIO contract");
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&url)
        .await
        .expect("PostgreSQL must be reachable in CI");
    runtime_migration::MIGRATOR
        .run(&pool)
        .await
        .expect("PLAN-0008 migrations must apply");
    pool
}

fn storage() -> S3Client {
    S3Client::new(
        &env_or("MINIO_ENDPOINT", "http://127.0.0.1:9000"),
        &env_or("MINIO_ACCESS_KEY", "minioadmin"),
        &env_or("MINIO_SECRET_KEY", "minioadmin"),
        &env_or("MINIO_BUCKET", "contract-test-bucket"),
        "us-east-1",
    )
}

fn upload_router(
    pool: PgPool,
    objects: Arc<dyn ObjectStorageClient>,
    tenant: Uuid,
    user: Uuid,
) -> axum::Router {
    let config = BusinessApiConfig {
        env: RuntimeEnvironment::Development,
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            request_timeout_secs: 30,
            cors_origins: Vec::new(),
            body_limit_bytes: 1024 * 1024,
        },
        database: DatabaseConfig {
            backend: DatabaseBackend::Postgres,
            url: SecretUrl::parse("postgres://localhost/test").expect("test URL must parse"),
            max_connections: 20,
            min_connections: 0,
            acquire_timeout_secs: 2,
        },
        observability: ObservabilityConfig {
            service_name: "plan-0008-postgres-minio-upload".to_string(),
            otlp_endpoint: None,
            log_level: "info".to_string(),
        },
        storage: StorageConfig::default(),
        auth: AuthConfig {
            issuer_url: String::new(),
            audience: None,
            dev_secret: Some(Secret::new("local-pg-e2e-only".to_string())),
            dev_auth_enabled: true,
            dev_permissions: Default::default(),
            dev_tenant_id: Some(tenant),
            dev_user_id: Some(user),
            dev_subject: Some("plan-0008-upload-test".to_string()),
            dev_roles: Default::default(),
        },
    };
    let document_store = Arc::new(PostgresCreateDocumentUnitOfWork::new(pool.clone()));
    let state = Arc::new(AppState {
        documents: DocumentServices {
            create: Arc::new(CreateDocumentMetadata::new(document_store)),
            detail: Arc::new(document_postgres::PostgresDocumentDetailQuery::new(
                pool.clone(),
            )),
            list: Arc::new(document_postgres::PostgresDocumentListQuery::new(
                pool.clone(),
            )),
        },
        processing: None,
        governance: None,
        readiness: Arc::new(PostgresReadinessProbe::new(pool)),
        storage: Some(StorageServices { objects }),
    });
    routes::create_router(
        state,
        AuthMiddlewareConfig {
            dev_auth_enabled: true,
            dev_secret: Some("local-pg-e2e-only".to_string()),
            dev_permissions: Default::default(),
            dev_tenant_id: Some(tenant),
            dev_user_id: Some(user),
            dev_subject: Some("plan-0008-upload-test".to_string()),
            dev_roles: Default::default(),
        },
        &config.server,
    )
}

fn multipart_upload_request(
    idempotency_key: &str,
    tenant: Uuid,
    user: Uuid,
    content: &[u8],
) -> Request<Body> {
    let boundary = "plan-0008-upload-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"plan-0008.txt\"\r\nContent-Type: text/plain\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/v1/documents/upload")
        .header("authorization", "Bearer local-pg-e2e-only")
        .header("x-tenant-id", tenant.to_string())
        .header("x-user-id", user.to_string())
        .header("x-request-id", "plan-0008-upload-request")
        .header("idempotency-key", idempotency_key)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("multipart upload request must build")
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(
        &response
            .into_body()
            .collect()
            .await
            .expect("response body must collect")
            .to_bytes(),
    )
    .expect("response must be JSON")
}

#[tokio::test]
#[ignore = "requires real PostgreSQL, PLAN-0008 migrations, and MinIO"]
async fn plan_0008_postgres_minio_multipart_upload_is_idempotent() {
    let pool = pool().await;
    let storage: Arc<dyn ObjectStorageClient> = Arc::new(storage());
    let tenant = Uuid::now_v7();
    let user = Uuid::now_v7();
    let idempotency_key = format!("plan-0008-multipart-upload-{tenant}");
    let content = b"PLAN-0008 real multipart upload\n";
    let router = upload_router(pool.clone(), Arc::clone(&storage), tenant, user);

    let created = router
        .clone()
        .oneshot(multipart_upload_request(
            &idempotency_key,
            tenant,
            user,
            content,
        ))
        .await
        .expect("multipart upload must respond");
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_json = response_json(created).await;
    let document_id = Uuid::parse_str(
        created_json["data"]["id"]
            .as_str()
            .expect("created document id"),
    )
    .expect("created document id must be UUID");
    let revision_id = Uuid::parse_str(
        created_json["data"]["revision_id"]
            .as_str()
            .expect("created revision id"),
    )
    .expect("created revision id must be UUID");

    let replayed = router
        .oneshot(multipart_upload_request(
            &idempotency_key,
            tenant,
            user,
            content,
        ))
        .await
        .expect("multipart replay must respond");
    assert_eq!(replayed.status(), StatusCode::OK);
    let replayed_json = response_json(replayed).await;
    assert_eq!(replayed_json["data"]["id"], created_json["data"]["id"]);
    assert_eq!(
        replayed_json["data"]["revision_id"],
        created_json["data"]["revision_id"]
    );

    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM documents WHERE tenant_id = $1 AND id = $2), (SELECT COUNT(*) FROM document_revisions WHERE tenant_id = $1 AND document_id = $2)",
    )
    .bind(tenant)
    .bind(document_id)
    .fetch_one(&pool)
    .await
    .expect("upload counts must be queryable");
    assert_eq!(counts, (1, 1));

    let expected_sha256 = format!("{:x}", Sha256::digest(content));
    let source_object_ref: String = sqlx::query_scalar(
        "SELECT source_object_ref FROM document_revisions WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(revision_id)
    .fetch_one(&pool)
    .await
    .expect("uploaded revision must be queryable");
    let object_key = ObjectKey::new(source_object_ref).expect("uploaded object key");
    let metadata: (String, i64, String, String) = sqlx::query_as(
        "SELECT sha256, size_bytes, content_type, source_object_ref FROM document_revisions WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(revision_id)
    .fetch_one(&pool)
    .await
    .expect("uploaded metadata must be queryable");
    assert_eq!(metadata.0, expected_sha256);
    assert_eq!(metadata.1, content.len() as i64);
    assert_eq!(metadata.2, "text/plain");
    assert_eq!(metadata.3, object_key.as_str());

    let head = storage
        .head(&object_key)
        .await
        .expect("uploaded object head");
    assert_eq!(head.content_length, content.len() as u64);
    assert_eq!(head.content_type.as_deref(), Some("text/plain"));
    assert_eq!(head.metadata.get("sha256"), Some(&expected_sha256));
    let fetched = storage
        .get_object(&object_key)
        .await
        .expect("uploaded object");
    assert_eq!(fetched.as_ref(), content);
    storage.delete(&object_key).await.expect("upload cleanup");
}

#[tokio::test]
#[ignore = "requires real PostgreSQL, PLAN-0008 migrations, and MinIO"]
async fn plan_0008_postgres_minio_revision_evidence_contract() {
    let pool = pool().await;
    let storage = storage();
    let document_store = Arc::new(PostgresCreateDocumentUnitOfWork::new(pool.clone()));
    let create = CreateDocumentMetadata::new(document_store.clone());
    let tenant = Uuid::now_v7();
    let user = Uuid::now_v7();
    let document_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let first_content = Bytes::from_static(b"PLAN-0008 source revision one\n");
    let first_sha256 = format!("{:x}", Sha256::digest(first_content.as_ref()));
    let initial = DocumentMetadata::create_with_revision_id(
        document_id,
        tenant,
        "contract.txt".to_string(),
        "text/plain".to_string(),
        "uploads/contract.txt".to_string(),
        user,
        Some(i64::try_from(first_content.len()).unwrap_or(i64::MAX)),
        revision_id,
    )
    .expect("document fixture must be valid");
    let source_key = ObjectKey::new(initial.object_key()).expect("revision source key is valid");
    let stored_sha256 =
        put_content(&storage, &source_key, first_content.clone(), "text/plain").await;
    assert_eq!(stored_sha256, first_sha256);

    let command = CreateDocumentCommand {
        tenant_id: tenant,
        user_id: user,
        original_filename: "contract.txt".to_string(),
        content_type: "text/plain".to_string(),
        object_key: "uploads/contract.txt".to_string(),
        size_bytes: Some(i64::try_from(first_content.len()).unwrap_or(i64::MAX)),
        sha256: Some(first_sha256.clone()),
        revision_id: Some(revision_id),
        idempotency_key: format!("plan-0008-upload-{tenant}"),
    };
    let created = create
        .execute_with_id(Some(document_id), command.clone())
        .await
        .expect("initial document create must succeed");
    assert!(!created.replayed);

    // The same immutable key may be safely retried. The object is overwritten
    // with identical bytes and the DB UoW returns the original revision.
    let replay = create
        .execute_with_id(Some(document_id), command)
        .await
        .expect("same upload must replay");
    assert!(replay.replayed);
    assert_eq!(replay.document.current_revision_id(), revision_id);
    let revision_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_revisions WHERE tenant_id = $1 AND document_id = $2",
    )
    .bind(tenant)
    .bind(document_id)
    .fetch_one(&pool)
    .await
    .expect("revision count must be queryable");
    assert_eq!(revision_count, 1, "idempotent upload must not duplicate R1");

    let source_head = storage
        .head(&source_key)
        .await
        .expect("source object must be present");
    assert_eq!(source_head.content_length, first_content.len() as u64);
    assert_eq!(source_head.content_type.as_deref(), Some("text/plain"));
    assert_eq!(source_head.metadata.get("sha256"), Some(&first_sha256));
    assert_eq!(
        storage.get_object(&source_key).await.expect("source get"),
        first_content
    );
    let stored_revision: (String, String, i64, String) = sqlx::query_as(
        "SELECT source_object_ref, sha256, size_bytes, content_type FROM document_revisions WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(revision_id)
    .fetch_one(&pool)
    .await
    .expect("revision metadata must be queryable");
    assert_eq!(stored_revision.0, source_key.as_str());
    assert_eq!(stored_revision.1, first_sha256);
    assert_eq!(
        stored_revision.2,
        i64::try_from(first_content.len()).unwrap_or(i64::MAX)
    );
    assert_eq!(stored_revision.3, "text/plain");

    // R2 creation and current selection are one Document aggregate save.
    let current = document_store
        .load(tenant, document_id)
        .await
        .expect("document load")
        .expect("document must exist");
    let expected_version = current.aggregate_version();
    let second_content = Bytes::from_static(b"PLAN-0008 source revision two\n");
    let second_sha256 = format!("{:x}", Sha256::digest(second_content.as_ref()));
    let mut second = current.clone();
    let second_revision = second
        .replace_content_revision_with_sha256(
            "contract-v2.txt".to_string(),
            Some("contract update".to_string()),
            Some(second_sha256.clone()),
        )
        .expect("R2 creation must succeed");
    let second_key = ObjectKey::new(second_revision.source_object_ref()).expect("R2 key");
    put_content(&storage, &second_key, second_content.clone(), "text/plain").await;
    document_store
        .save(&second, Some(&second_revision), expected_version)
        .await
        .expect("R2 current selection must commit");
    let reloaded = document_store
        .load(tenant, document_id)
        .await
        .expect("reloaded document")
        .expect("reloaded document must exist");
    assert_eq!(reloaded.current_revision_id(), second_revision.id());
    assert_eq!(reloaded.content_revision().value(), 2);
    assert_eq!(reloaded.object_key(), second_key.as_str());

    let history: Vec<(i64, Uuid, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT revision_no, id, parent_revision_id, sha256 FROM document_revisions WHERE tenant_id = $1 AND document_id = $2 ORDER BY revision_no",
    )
    .bind(tenant)
    .bind(document_id)
    .fetch_all(&pool)
    .await
    .expect("revision history must be queryable");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].0, 1);
    assert_eq!(history[0].1, revision_id);
    assert_eq!(history[0].3, first_sha256);
    assert_eq!(history[1].0, 2);
    assert_eq!(history[1].1, second_revision.id());
    assert_eq!(history[1].2, Some(revision_id));
    assert_eq!(history[1].3, second_sha256);

    // A historical revision is database-enforced immutable, not merely
    // conventionally read-only through the Rust adapter.
    let immutable_update = sqlx::query(
        "UPDATE document_revisions SET change_reason = 'tampered' WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(revision_id)
    .execute(&pool)
    .await;
    assert!(
        immutable_update.is_err(),
        "historical revision mutation must fail closed"
    );

    // Two writers derived from the same aggregate version race. Exactly one
    // may select R3; the losing source object is an orphan candidate that the
    // caller must compensate because MinIO is outside the PostgreSQL tx.
    let stale_a = reloaded.clone();
    let stale_b = reloaded.clone();
    let expected_stale_version = reloaded.aggregate_version();
    let bytes_a = Bytes::from_static(b"stale candidate A\n");
    let bytes_b = Bytes::from_static(b"stale candidate B\n");
    let hash_a = format!("{:x}", Sha256::digest(bytes_a.as_ref()));
    let hash_b = format!("{:x}", Sha256::digest(bytes_b.as_ref()));
    let mut candidate_a = stale_a;
    let revision_a =
        candidate_a.replace_content_revision_with_sha256("a.txt".to_string(), None, Some(hash_a));
    let mut candidate_b = stale_b;
    let revision_b =
        candidate_b.replace_content_revision_with_sha256("b.txt".to_string(), None, Some(hash_b));
    let revision_a = revision_a.expect("candidate A revision");
    let revision_b = revision_b.expect("candidate B revision");
    let key_a = ObjectKey::new(revision_a.source_object_ref()).expect("candidate A key");
    let key_b = ObjectKey::new(revision_b.source_object_ref()).expect("candidate B key");
    put_content(&storage, &key_a, bytes_a, "text/plain").await;
    put_content(&storage, &key_b, bytes_b, "text/plain").await;
    let (result_a, result_b) = tokio::join!(
        document_store.save(&candidate_a, Some(&revision_a), expected_stale_version),
        document_store.save(&candidate_b, Some(&revision_b), expected_stale_version),
    );
    assert!(matches!(
        (&result_a, &result_b),
        (Err(RepositoryError::Conflict), Ok(())) | (Ok(()), Err(RepositoryError::Conflict))
    ));
    assert_eq!(u8::from(result_a.is_ok()) + u8::from(result_b.is_ok()), 1);
    let failed_key = if result_a.is_err() { &key_a } else { &key_b };
    storage
        .delete(failed_key)
        .await
        .expect("orphan compensation must be executable");
    assert!(!storage.exists(failed_key).await.expect("orphan check"));
    let final_document = document_store
        .load(tenant, document_id)
        .await
        .expect("final document load")
        .expect("final document must exist");
    assert_eq!(final_document.content_revision().value(), 3);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM document_revisions WHERE tenant_id = $1 AND document_id = $2",
        )
        .bind(tenant)
        .bind(document_id)
        .fetch_one(&pool)
        .await
        .expect("final revision count"),
        3
    );

    let processing_store = PostgresProcessingStore::new(pool.clone());
    let process_now = Utc::now() - Duration::seconds(1);
    let job = ProcessingJob::queue_for_revision(
        tenant,
        document_id,
        final_document.current_revision_id(),
        final_document.content_revision().value(),
        format!("plan-0008-job-{tenant}"),
        user,
        3,
        process_now,
    )
    .expect("revision-bound processing job");
    processing_store
        .create(&job)
        .await
        .expect("processing job create");
    processing_store
        .create(&job)
        .await
        .expect("processing job replay");
    let job_row: (Uuid, i64) = sqlx::query_as(
        "SELECT document_revision_id, content_revision FROM document_processing_jobs WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(job.id())
    .fetch_one(&pool)
    .await
    .expect("processing binding must be queryable");
    assert_eq!(job_row, (final_document.current_revision_id(), 3));

    let claimed = processing_store
        .claim_next("plan-0008-retry-worker", Utc::now(), 30)
        .await
        .expect("job claim")
        .expect("job must be claimable");
    let fence = ExecutionFence::new(
        "plan-0008-retry-worker",
        claimed.lease_token.clone(),
        claimed.fence_version,
    );
    processing_store
        .start_step(
            tenant,
            job.id(),
            document_processing::ProcessingStepKind::ValidateSource,
            &fence,
            Utc::now(),
        )
        .await
        .expect("retry step start");
    let retried = processing_store
        .retry_or_fail_step(
            tenant,
            job.id(),
            document_processing::ProcessingStepKind::ValidateSource,
            ClassifiedProcessingFailure {
                code: "test_transient".to_string(),
                message: Some("contract retry".to_string()),
                disposition: ProcessingFailureDisposition::Retry {
                    backoff: Duration::zero(),
                },
            },
            &fence,
            Utc::now(),
        )
        .await
        .expect("retry transition");
    assert_eq!(
        retried.status(),
        document_processing::ProcessingJobStatus::Queued
    );
    assert_eq!(retried.attempt_count(), 1);
    let reclaimed = processing_store
        .claim_next("plan-0008-retry-worker-2", Utc::now(), 30)
        .await
        .expect("retry reclaim")
        .expect("retried job must converge back to claimable");
    assert_eq!(reclaimed.job.id(), job.id());

    let run = ProcessingRun::start(
        tenant,
        final_document.current_revision_id(),
        "plan-0008.pipeline.v1".to_string(),
        "contract-test".to_string(),
        "1".to_string(),
        None,
        None,
        user,
        Utc::now(),
    )
    .expect("processing run");
    let artifact_content = Bytes::from_static(b"artifact for revision three\n");
    let artifact_object_key = ObjectKey::new(format!(
        "tenants/{tenant}/documents/{document_id}/revisions/{}/artifacts/ocr.txt",
        final_document.current_revision_id()
    ))
    .expect("artifact key");
    let artifact_hash = put_content(
        &storage,
        &artifact_object_key,
        artifact_content.clone(),
        "text/plain",
    )
    .await;
    let artifact_head = storage
        .head(&artifact_object_key)
        .await
        .expect("artifact object must be present");
    assert_eq!(artifact_head.content_length, artifact_content.len() as u64);
    assert_eq!(artifact_head.content_type.as_deref(), Some("text/plain"));
    assert_eq!(artifact_head.metadata.get("sha256"), Some(&artifact_hash));
    assert_eq!(
        storage
            .get_object(&artifact_object_key)
            .await
            .expect("artifact get"),
        artifact_content
    );
    let artifact_key = artifact_object_key.as_str().to_string();
    let artifact = ProcessingArtifact::new(
        tenant,
        run.id(),
        document_processing::ArtifactKind::OcrText,
        artifact_key.clone(),
        artifact_hash.clone(),
        "ocr.v1".to_string(),
        Utc::now(),
    )
    .expect("processing artifact");
    let evidence = Evidence::new(
        tenant,
        final_document.current_revision_id(),
        &run,
        &artifact,
        serde_json::json!({"line_start": 1, "line_end": 1}),
        final_document
            .current_revision_id()
            .to_string()
            .replace('-', "0"),
        Utc::now(),
    );
    assert!(
        evidence.is_err(),
        "evidence checksum validation must reject non-SHA256"
    );
    let source_checksum = sqlx::query_scalar::<_, String>(
        "SELECT sha256 FROM document_revisions WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(final_document.current_revision_id())
    .fetch_one(&pool)
    .await
    .expect("current source checksum");
    let evidence = Evidence::new(
        tenant,
        final_document.current_revision_id(),
        &run,
        &artifact,
        serde_json::json!({"line_start": 1, "line_end": 1}),
        source_checksum.clone(),
        Utc::now(),
    )
    .expect("revision-bound evidence");

    sqlx::query("INSERT INTO document_processing_runs (id,tenant_id,document_revision_id,pipeline_version,parser_name,parser_version,status,started_at,created_by,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$8) ON CONFLICT (tenant_id,id) DO NOTHING")
        .bind(run.id())
        .bind(run.tenant_id())
        .bind(run.document_revision_id())
        .bind(run.pipeline_version())
        .bind(run.parser_name())
        .bind(run.parser_version())
        .bind("running")
        .bind(run.created_at())
        .bind(run.created_by())
        .execute(&pool)
        .await
        .expect("processing run insert");
    sqlx::query("INSERT INTO document_processing_artifacts (id,tenant_id,processing_run_id,kind,storage_ref,checksum,schema_version,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant_id,processing_run_id,kind) DO NOTHING")
        .bind(artifact.id())
        .bind(artifact.tenant_id())
        .bind(artifact.processing_run_id())
        .bind(artifact.kind().as_str())
        .bind(artifact.storage_ref())
        .bind(artifact.checksum())
        .bind(artifact.schema_version())
        .bind(artifact.created_at())
        .execute(&pool)
        .await
        .expect("artifact insert");
    sqlx::query("INSERT INTO document_processing_evidence (id,tenant_id,document_revision_id,processing_run_id,artifact_id,location_json,source_checksum,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant_id,id) DO NOTHING")
        .bind(evidence.id())
        .bind(evidence.tenant_id())
        .bind(evidence.document_revision_id())
        .bind(evidence.processing_run_id())
        .bind(evidence.artifact_id())
        .bind(evidence.location())
        .bind(evidence.source_checksum())
        .bind(evidence.created_at())
        .execute(&pool)
        .await
        .expect("evidence insert");
    // Replaying the same persisted identities is a no-op at every evidence
    // layer; no second revision/artifact/evidence row is created.
    sqlx::query("INSERT INTO document_processing_runs (id,tenant_id,document_revision_id,pipeline_version,parser_name,parser_version,status,started_at,created_by,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$8) ON CONFLICT (tenant_id,id) DO NOTHING")
        .bind(run.id())
        .bind(run.tenant_id())
        .bind(run.document_revision_id())
        .bind(run.pipeline_version())
        .bind(run.parser_name())
        .bind(run.parser_version())
        .bind("running")
        .bind(run.created_at())
        .bind(run.created_by())
        .execute(&pool)
        .await
        .expect("run replay");
    sqlx::query("INSERT INTO document_processing_artifacts (id,tenant_id,processing_run_id,kind,storage_ref,checksum,schema_version,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant_id,processing_run_id,kind) DO NOTHING")
        .bind(artifact.id())
        .bind(artifact.tenant_id())
        .bind(artifact.processing_run_id())
        .bind(artifact.kind().as_str())
        .bind(artifact.storage_ref())
        .bind(artifact.checksum())
        .bind(artifact.schema_version())
        .bind(artifact.created_at())
        .execute(&pool)
        .await
        .expect("artifact replay");
    sqlx::query("INSERT INTO document_processing_evidence (id,tenant_id,document_revision_id,processing_run_id,artifact_id,location_json,source_checksum,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant_id,id) DO NOTHING")
        .bind(evidence.id())
        .bind(evidence.tenant_id())
        .bind(evidence.document_revision_id())
        .bind(evidence.processing_run_id())
        .bind(evidence.artifact_id())
        .bind(evidence.location())
        .bind(evidence.source_checksum())
        .bind(evidence.created_at())
        .execute(&pool)
        .await
        .expect("evidence replay");
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM document_processing_runs WHERE tenant_id=$1 AND id=$2), (SELECT COUNT(*) FROM document_processing_artifacts WHERE tenant_id=$1 AND id=$3), (SELECT COUNT(*) FROM document_processing_evidence WHERE tenant_id=$1 AND id=$4)",
    )
    .bind(tenant)
    .bind(run.id())
    .bind(artifact.id())
    .bind(evidence.id())
    .fetch_one(&pool)
    .await
    .expect("evidence counts");
    assert_eq!(counts, (1, 1, 1));

    let wrong_binding = sqlx::query("INSERT INTO document_processing_evidence (id,tenant_id,document_revision_id,processing_run_id,artifact_id,location_json,source_checksum,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(Uuid::now_v7())
        .bind(tenant)
        .bind(second_revision.id())
        .bind(run.id())
        .bind(artifact.id())
        .bind(serde_json::json!({"line_start": 99}))
        .bind(source_checksum)
        .bind(Utc::now())
        .execute(&pool)
        .await;
    assert!(
        wrong_binding.is_err(),
        "DB must reject cross-revision evidence"
    );

    // PostgreSQL and MinIO do not share a distributed transaction. A source
    // object can exist after a rolled-back metadata attempt; current behavior
    // is immediate best-effort compensation, not database+object atomicity.
    let orphan_key = ObjectKey::new(format!(
        "tenants/{tenant}/documents/{document_id}/revisions/{}/orphan/source",
        Uuid::now_v7()
    ))
    .expect("orphan key");
    put_content(
        &storage,
        &orphan_key,
        Bytes::from_static(b"orphan"),
        "text/plain",
    )
    .await;
    let rollback_link_id = Uuid::now_v7();
    let rollback_resource_id = Uuid::now_v7();
    let mut transaction = pool.begin().await.expect("rollback transaction");
    sqlx::query("INSERT INTO document_links (id,tenant_id,document_id,resource_kind,resource_id,role,created_by) VALUES ($1,$2,$3,'contract',$4,'evidence',$5)")
        .bind(rollback_link_id)
        .bind(tenant)
        .bind(document_id)
        .bind(rollback_resource_id)
        .bind(user)
        .execute(&mut *transaction)
        .await
        .expect("rollback fixture insert");
    transaction
        .rollback()
        .await
        .expect("database rollback must succeed");
    let rolled_back_links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM document_links WHERE tenant_id=$1 AND id=$2")
            .bind(tenant)
            .bind(rollback_link_id)
            .fetch_one(&pool)
            .await
            .expect("rolled-back link check");
    assert_eq!(rolled_back_links, 0);
    let orphan_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_revisions WHERE tenant_id=$1 AND source_object_ref=$2",
    )
    .bind(tenant)
    .bind(orphan_key.as_str())
    .fetch_one(&pool)
    .await
    .expect("orphan DB check");
    assert_eq!(orphan_rows, 0);
    assert!(storage
        .exists(&orphan_key)
        .await
        .expect("orphan object check"));
    storage
        .delete(&orphan_key)
        .await
        .expect("current compensation path must remove orphan");
    assert!(!storage
        .exists(&orphan_key)
        .await
        .expect("orphan removal check"));

    // Avoid retaining test objects in a shared CI bucket.
    for key in [source_key, second_key, key_a, key_b, artifact_object_key] {
        let _ = storage.delete(&key).await;
    }
}
