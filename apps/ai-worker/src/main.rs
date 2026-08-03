//! Independent AI-task worker for the fixed document-processing pipeline.
//!
//! The MVP provider is deterministic and local. The worker still exercises a
//! separate `PostgreSQL` task claim/lease/fence boundary so a future provider can
//! be substituted without changing job ownership.

mod config;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use config::{AiWorkerConfig, WorkerDatabaseBackend};
use document::domain::DocumentRepository;
use document_processing::ports::{
    AiTask, AiTaskPort, CandidateStore, ProcessingJobCommandPort, ProcessingJobQuery,
    ProcessingStepStore,
};
use document_processing::{DeterministicLocalExtractor, FixedPipelineRunner, ProcessingSource};
use object_storage::{LocalStorageClient, ObjectKey, ObjectStorageClient, S3Client, StorageError};
use tokio::time::sleep;
use uuid::Uuid;

struct StorageSource {
    documents: Arc<dyn DocumentRepository>,
    storage: Arc<dyn ObjectStorageClient>,
}

type AiWorkerPorts = (
    Arc<dyn AiTaskPort>,
    Arc<dyn CandidateStore>,
    Arc<dyn ProcessingJobCommandPort>,
    Arc<dyn ProcessingJobQuery>,
    Arc<dyn ProcessingStepStore>,
    Arc<dyn DocumentRepository>,
);

#[async_trait]
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
    let config = AiWorkerConfig::load()?;
    config
        .validate()
        .map_err(|error| anyhow::anyhow!("AI worker configuration invalid: {error}"))?;
    let _guard = observability::init_tracing(
        "ai-worker",
        &config.observability.log_level,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    let url = config
        .database
        .url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("AI_WORKER__DATABASE__URL is required"))?;
    let storage = build_storage(&config.storage).await?;

    let (tasks, candidates, commands, queries, steps, documents): AiWorkerPorts =
        match config.database.backend {
            WorkerDatabaseBackend::Postgres => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(4)
                    .connect(url.expose())
                    .await?;
                let processing = Arc::new(
                    document_processing_postgres::PostgresProcessingStore::new(pool.clone()),
                );
                (
                    processing.clone(),
                    processing.clone(),
                    processing.clone(),
                    processing.clone(),
                    processing,
                    Arc::new(document_postgres::PostgresCreateDocumentUnitOfWork::new(
                        pool,
                    )),
                )
            }
            WorkerDatabaseBackend::Sqlite => {
                return Err(anyhow::anyhow!("AI worker requires PostgreSQL"));
            }
        };
    let source = StorageSource { documents, storage };

    tracing::info!(worker_id = %config.worker_id, "ai-worker ready");
    let mut shutdown = Box::pin(shutdown_signal());
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!(worker_id = %config.worker_id, "ai-worker graceful shutdown");
                break;
            }
            () = sleep(Duration::from_millis(config.poll_interval_millis)) => {
                let now = Utc::now();
                if let Some(task) = tasks.claim_next(&config.worker_id, now, config.lease_duration_secs).await? {
                    process_task(&tasks, &candidates, &commands, &queries, &steps, &source, task, &config).await;
                }
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
            let base_dir = config
                .base_dir
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("AI_WORKER__STORAGE__BASE_DIR is required"))?;
            Ok(Arc::new(LocalStorageClient::new(base_dir).await?))
        }
        "s3" => {
            let endpoint = config
                .endpoint
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("S3 storage endpoint is required"))?;
            let bucket = config
                .bucket
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("S3 storage bucket is required"))?;
            let access_key = config
                .access_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("S3 storage access key is required"))?;
            let secret_key = config
                .secret_key
                .as_ref()
                .map(|value| value.expose().as_str())
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

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn process_task(
    tasks: &Arc<dyn AiTaskPort>,
    candidates: &Arc<dyn CandidateStore>,
    commands: &Arc<dyn ProcessingJobCommandPort>,
    queries: &Arc<dyn ProcessingJobQuery>,
    steps: &Arc<dyn ProcessingStepStore>,
    source: &StorageSource,
    task: AiTask,
    config: &AiWorkerConfig,
) {
    let Some(token) = task.lease_token.as_deref() else {
        return;
    };
    tracing::info!(task_id = %task.id, job_id = %task.job_id, step = %task.step_kind, fence = task.fence_version, "AI task claimed");
    if let Err(error) = tasks
        .heartbeat(
            task.id,
            &config.worker_id,
            token,
            task.fence_version,
            Utc::now(),
            config.lease_duration_secs,
        )
        .await
    {
        tracing::warn!(task_id = %task.id, error = %error, "AI task heartbeat lost");
        return;
    }
    let result = async {
        let detail = queries
            .detail(task.tenant_id, task.job_id)
            .await
            .map_err(|_| document_processing::ExtractionError::Internal)?
            .ok_or(document_processing::ExtractionError::SourceNotFound)?;
        let (content_type, bytes) = source
            .read_source(
                detail.job.tenant_id(),
                detail.job.document_id(),
                detail.job.document_content_revision(),
            )
            .await?;
        FixedPipelineRunner
            .run_inline(
                &detail.job,
                &content_type,
                &bytes,
                config.max_content_bytes,
                &DeterministicLocalExtractor,
            )
            .await
    }
    .await;
    match result {
        Ok(run) => {
            if let Err(error) = candidates.save_candidate(&run.candidate).await {
                tracing::warn!(task_id = %task.id, error = %error, "failed to persist AI candidate");
                let _ = tasks
                    .fail(
                        task.id,
                        &config.worker_id,
                        token,
                        task.fence_version,
                        "internal_error",
                        Utc::now(),
                    )
                    .await;
                return;
            }
            if let Err(error) = tasks
                .complete(
                    task.id,
                    &config.worker_id,
                    token,
                    task.fence_version,
                    run.candidate.id(),
                    Utc::now(),
                )
                .await
            {
                tracing::warn!(task_id = %task.id, error = %error, "AI task completion was fenced");
            } else if let Ok(Some(mut job)) = commands.load(task.tenant_id, task.job_id).await {
                if job.status() == document_processing::ProcessingJobStatus::WaitingForAi {
                    let expected = job.aggregate_version().value();
                    let now = Utc::now();
                    let resume_token = Uuid::now_v7().to_string();
                    if job
                        .resume_from_ai(
                            config.worker_id.clone(),
                            resume_token,
                            now + chrono::Duration::seconds(config.lease_duration_secs),
                            now,
                        )
                        .is_ok()
                    {
                        if let Err(error) = commands.save(&job, expected).await {
                            tracing::warn!(task_id = %task.id, error = %error, "AI job resume was fenced");
                        } else if let Err(error) = steps
                            .complete(
                                task.job_id,
                                task.tenant_id,
                                task.step_kind,
                                job.attempt_count(),
                                job.aggregate_version().value(),
                                now,
                            )
                            .await
                        {
                            tracing::warn!(task_id = %task.id, error = %error, "AI step completion failed");
                        }
                    }
                }
            }
        }
        Err(error) => {
            tracing::warn!(task_id = %task.id, failure_code = error.code(), "AI task failed");
            let _ = tasks
                .fail(
                    task.id,
                    &config.worker_id,
                    token,
                    task.fence_version,
                    error.code(),
                    Utc::now(),
                )
                .await;
        }
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
}
