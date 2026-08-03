//! Durable business worker for the fixed document-processing pipeline.

mod config;

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use config::{AiMode, BusinessWorkerConfig, WorkerDatabaseBackend};
use document::domain::DocumentRepository;
use document_processing::ports::{
    AiTask, AiTaskPort, CandidateStore, ProcessingJobClaimPort, ProcessingJobCommandPort,
    ProcessingStepStore, StepCheckpoint,
};
use document_processing::{DeterministicLocalExtractor, FixedPipelineRunner, ProcessingSource};
use object_storage::{LocalStorageClient, ObjectKey, ObjectStorageClient, S3Client, StorageError};
use tokio::time::{sleep, Instant};
use uuid::Uuid;

struct WorkerServices {
    commands: Arc<dyn ProcessingJobCommandPort>,
    claims: Arc<dyn ProcessingJobClaimPort>,
    steps: Arc<dyn ProcessingStepStore>,
    candidates: Arc<dyn CandidateStore>,
    ai_tasks: Arc<dyn AiTaskPort>,
}

struct StorageSource {
    documents: Arc<dyn DocumentRepository>,
    storage: Arc<dyn ObjectStorageClient>,
}

#[async_trait::async_trait]
impl ProcessingSource for StorageSource {
    async fn read_source(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
        content_revision: i64,
    ) -> Result<(String, Vec<u8>), document_processing::ExtractionError> {
        let document = self
            .documents
            .load(tenant_id, document_id)
            .await
            .map_err(|_| document_processing::ExtractionError::Internal)?
            .ok_or(document_processing::ExtractionError::SourceNotFound)?;
        if document.content_revision().value() != content_revision {
            return Err(document_processing::ExtractionError::SourceRevisionMismatch);
        }
        let key = ObjectKey::new(document.object_key())
            .map_err(|_| document_processing::ExtractionError::SourceNotFound)?;
        let bytes = self
            .storage
            .get_object(&key)
            .await
            .map_err(|error| match error {
                StorageError::NotFound(_) => document_processing::ExtractionError::SourceNotFound,
                StorageError::TooLarge(_) => document_processing::ExtractionError::ContentTooLarge,
                _ => document_processing::ExtractionError::Internal,
            })?;
        Ok((document.content_type().to_string(), bytes.to_vec()))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = BusinessWorkerConfig::load()?;
    config
        .validate()
        .map_err(|error| anyhow::anyhow!("worker configuration invalid: {error}"))?;
    let _guard = observability::init_tracing(
        "business-worker",
        &config.observability.log_level,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    let storage = build_storage(&config.storage).await?;

    let (claims, commands, steps, candidates, ai_tasks, documents) =
        match config.database.backend {
            WorkerDatabaseBackend::Sqlite => {
                let url =
                    config.database.url.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("BUSINESS_WORKER__DATABASE__URL is required")
                    })?;
                let pool = document_sqlite::connect(url.expose(), 1).await?;
                document_sqlite::MIGRATOR.run(&pool).await?;
                document_processing_sqlite::run_migrations(&pool).await?;
                let processing = Arc::new(document_processing_sqlite::SqliteProcessingStore::new(
                    pool.clone(),
                ));
                let document = Arc::new(document_sqlite::SqliteCreateDocumentUnitOfWork::new(pool));
                (
                    processing.clone() as Arc<dyn ProcessingJobClaimPort>,
                    processing.clone() as Arc<dyn ProcessingJobCommandPort>,
                    processing.clone() as Arc<dyn ProcessingStepStore>,
                    processing.clone() as Arc<dyn CandidateStore>,
                    processing as Arc<dyn AiTaskPort>,
                    document as Arc<dyn DocumentRepository>,
                )
            }
            WorkerDatabaseBackend::Postgres => {
                let url =
                    config.database.url.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("BUSINESS_WORKER__DATABASE__URL is required")
                    })?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(config.concurrency.max(1))
                    .connect(url.expose())
                    .await?;
                let processing = Arc::new(
                    document_processing_postgres::PostgresProcessingStore::new(pool.clone()),
                );
                let document = Arc::new(document_postgres::PostgresCreateDocumentUnitOfWork::new(
                    pool,
                ));
                (
                    processing.clone() as Arc<dyn ProcessingJobClaimPort>,
                    processing.clone() as Arc<dyn ProcessingJobCommandPort>,
                    processing.clone() as Arc<dyn ProcessingStepStore>,
                    processing.clone() as Arc<dyn CandidateStore>,
                    processing as Arc<dyn AiTaskPort>,
                    document as Arc<dyn DocumentRepository>,
                )
            }
        };
    let source = StorageSource { documents, storage };
    let services = WorkerServices {
        commands,
        claims,
        steps,
        candidates,
        ai_tasks,
    };

    tracing::info!(worker_id = %config.worker_id, concurrency = config.concurrency, "business-worker ready");
    let mut shutdown = Box::pin(shutdown_signal());
    let mut next_poll = Instant::now();
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!(worker_id = %config.worker_id, "business-worker graceful shutdown");
                break;
            }
            () = sleep(next_poll.saturating_duration_since(Instant::now())) => {
                let now = Utc::now();
                let _ = services.claims.reclaim_expired(now).await;
                if let Some(claimed) = services.claims.claim_next(&config.worker_id, now, config.lease_duration_secs).await? {
                    tracing::info!(job_id = %claimed.job.id(), document_id = %claimed.job.document_id(), step = %claimed.job.current_step(), fence = claimed.fence_version, "processing job claimed");
                    process_claimed(&services, &source, claimed, config.max_content_bytes, &config.worker_id, config.lease_duration_secs, config.ai_mode).await;
                }
                next_poll = Instant::now() + Duration::from_millis(config.poll_interval_millis);
            }
        }
    }
    Ok(())
}

async fn build_storage(
    config: &config::WorkerStorageConfig,
) -> anyhow::Result<Arc<dyn ObjectStorageClient>> {
    match config.backend.as_str() {
        "local" => {
            let directory = config.base_dir.as_deref().ok_or_else(|| {
                anyhow::anyhow!("BUSINESS_WORKER__STORAGE__BASE_DIR is required for local storage")
            })?;
            Ok(Arc::new(LocalStorageClient::new(directory).await?))
        }
        "s3" => {
            let endpoint = config
                .endpoint
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("S3 storage endpoint is required"))?;
            let bucket = config
                .bucket
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("S3 storage bucket is required"))?;
            let access_key = config
                .access_key
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("S3 storage access key is required"))?;
            let secret_key = config
                .secret_key
                .as_ref()
                .map(|value| value.expose().as_str())
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("S3 storage secret key is required"))?;
            Ok(Arc::new(S3Client::new(
                endpoint.expose(),
                access_key,
                secret_key,
                bucket,
                &config.region,
            )))
        }
        other => Err(anyhow::anyhow!("unsupported storage backend: {other}")),
    }
}

#[allow(clippy::too_many_lines)]
async fn process_claimed(
    services: &WorkerServices,
    source: &StorageSource,
    claimed: document_processing::ports::ClaimedProcessingJob,
    max_content_bytes: usize,
    worker_id: &str,
    lease_duration_secs: i64,
    ai_mode: AiMode,
) {
    let mut job = claimed.job;
    let token = claimed.lease_token;
    let fence = claimed.fence_version;
    let result = async {
        let (content_type, bytes) = source
            .read_source(
                job.tenant_id(),
                job.document_id(),
                job.document_content_revision(),
            )
            .await?;
        let pipeline = FixedPipelineRunner;
        let candidate = pipeline
            .run_inline(
                &job,
                &content_type,
                &bytes,
                max_content_bytes,
                &DeterministicLocalExtractor,
            )
            .await?;
        let steps = if ai_mode == AiMode::Separate {
            vec![
                document_processing::ProcessingStepKind::ValidateSource,
                document_processing::ProcessingStepKind::DetectType,
                document_processing::ProcessingStepKind::ExtractText,
            ]
        } else {
            vec![
                document_processing::ProcessingStepKind::ValidateSource,
                document_processing::ProcessingStepKind::DetectType,
                document_processing::ProcessingStepKind::ExtractText,
                document_processing::ProcessingStepKind::ExtractFields,
            ]
        };
        for step in steps {
            let now = Utc::now();
            let expected = job.aggregate_version().value();
            job.heartbeat(
                worker_id,
                &token,
                fence,
                now + chrono::Duration::seconds(lease_duration_secs),
                now,
            )
            .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            services
                .commands
                .save(&job, expected)
                .await
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            let expected = job.aggregate_version().value();
            job.start_step(worker_id, &token, fence, step, now)
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            services
                .commands
                .save(&job, expected)
                .await
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            let step_version = job.aggregate_version().value();
            services
                .steps
                .start(
                    &StepCheckpoint {
                        job_id: job.id(),
                        tenant_id: job.tenant_id(),
                        step_kind: step,
                        attempt_number: job.attempt_count(),
                        checkpoint_json: serde_json::json!({}),
                        updated_at: now,
                    },
                    step_version,
                )
                .await
                .map_err(|_| document_processing::ExtractionError::Internal)?;
            let expected = job.aggregate_version().value();
            job.complete_step(worker_id, &token, fence, step, Utc::now())
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            services
                .commands
                .save(&job, expected)
                .await
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            let step_version = job.aggregate_version().value();
            services
                .steps
                .complete(
                    job.id(),
                    job.tenant_id(),
                    step,
                    job.attempt_count(),
                    step_version,
                    Utc::now(),
                )
                .await
                .map_err(|_| document_processing::ExtractionError::Internal)?;
        }
        if ai_mode == AiMode::Separate {
            let now = Utc::now();
            let expected = job.aggregate_version().value();
            job.start_step(
                worker_id,
                &token,
                fence,
                document_processing::ProcessingStepKind::ExtractFields,
                now,
            )
            .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            services
                .commands
                .save(&job, expected)
                .await
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            let step_version = job.aggregate_version().value();
            services
                .steps
                .start(
                    &StepCheckpoint {
                        job_id: job.id(),
                        tenant_id: job.tenant_id(),
                        step_kind: document_processing::ProcessingStepKind::ExtractFields,
                        attempt_number: job.attempt_count(),
                        checkpoint_json: serde_json::json!({
                            "text_artifact_reference": format!("processing/{}/text", job.id())
                        }),
                        updated_at: now,
                    },
                    step_version,
                )
                .await
                .map_err(|_| document_processing::ExtractionError::Internal)?;
            services
                .ai_tasks
                .enqueue(&AiTask {
                    id: Uuid::now_v7(),
                    tenant_id: job.tenant_id(),
                    job_id: job.id(),
                    step_kind: document_processing::ProcessingStepKind::ExtractFields,
                    status: "queued".to_string(),
                    input_artifact_id: Some(format!("processing/{}/text", job.id())),
                    attempt_count: 0,
                    max_attempts: job.max_attempts(),
                    lease_token: None,
                    fence_version: 0,
                    lease_expires_at: None,
                })
                .await
                .map_err(|_| document_processing::ExtractionError::Internal)?;
            let expected = job.aggregate_version().value();
            job.wait_for_ai(worker_id, &token, fence, Utc::now())
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            services
                .commands
                .save(&job, expected)
                .await
                .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
            return Ok::<(), document_processing::ExtractionError>(());
        }
        services
            .candidates
            .save_candidate(&candidate.candidate)
            .await
            .map_err(|_| document_processing::ExtractionError::Internal)?;
        let now = Utc::now();
        let expected = job.aggregate_version().value();
        job.start_step(
            worker_id,
            &token,
            fence,
            document_processing::ProcessingStepKind::ValidateCandidate,
            now,
        )
        .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
        services
            .commands
            .save(&job, expected)
            .await
            .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
        let step_version = job.aggregate_version().value();
        services
            .steps
            .start(
                &StepCheckpoint {
                    job_id: job.id(),
                    tenant_id: job.tenant_id(),
                    step_kind: document_processing::ProcessingStepKind::ValidateCandidate,
                    attempt_number: job.attempt_count(),
                    checkpoint_json: serde_json::json!({}),
                    updated_at: now,
                },
                step_version,
            )
            .await
            .map_err(|_| document_processing::ExtractionError::Internal)?;
        let expected = job.aggregate_version().value();
        job.complete_step(
            worker_id,
            &token,
            fence,
            document_processing::ProcessingStepKind::ValidateCandidate,
            Utc::now(),
        )
        .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
        services
            .commands
            .save(&job, expected)
            .await
            .map_err(|_| document_processing::ExtractionError::LeaseLost)?;
        let step_version = job.aggregate_version().value();
        services
            .steps
            .complete(
                job.id(),
                job.tenant_id(),
                document_processing::ProcessingStepKind::ValidateCandidate,
                job.attempt_count(),
                step_version,
                Utc::now(),
            )
            .await
            .map_err(|_| document_processing::ExtractionError::Internal)?;
        Ok::<(), document_processing::ExtractionError>(())
    }
    .await;
    if let Err(error) = result {
        let now = Utc::now();
        let expected = job.aggregate_version().value();
        if job
            .fail_permanent(
                worker_id,
                &token,
                fence,
                error.code().to_string(),
                None,
                now,
            )
            .is_ok()
        {
            let _ = services.commands.save(&job, expected).await;
        }
        tracing::warn!(job_id = %job.id(), failure_code = error.code(), "processing job failed");
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
}
