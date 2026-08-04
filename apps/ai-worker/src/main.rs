//! Independent AI-task worker for the fixed document-processing pipeline.
//!
//! The MVP provider is deterministic and local. The worker still exercises a
//! separate `PostgreSQL` task claim/lease/fence boundary so a future provider can
//! be substituted without changing job ownership.

mod config;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use config::{AiWorkerConfig, WorkerDatabaseBackend};
use document::domain::DocumentRepository;
use document_processing::ports::{
    AiTask, ClassifiedProcessingFailure, CompleteAiTaskCommand, ExecutionFence,
    ProcessingExecutionUnitOfWork, ProcessingFailureDisposition, ProcessingJobQuery,
};
use document_processing::{
    DeterministicLocalExtractor, DocumentFieldExtractor, ExtractionError, ExtractionRequest,
    ProcessingSource,
};
use object_storage::{LocalStorageClient, ObjectKey, ObjectStorageClient, S3Client, StorageError};
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::sleep;
use uuid::Uuid;

struct StorageSource {
    documents: Arc<dyn DocumentRepository>,
    storage: Arc<dyn ObjectStorageClient>,
}

struct AiWorkerServices {
    execution: Arc<dyn ProcessingExecutionUnitOfWork>,
    queries: Arc<dyn ProcessingJobQuery>,
}

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

impl StorageSource {
    async fn read_artifact(&self, key: &str) -> Result<Vec<u8>, ExtractionError> {
        let key = ObjectKey::new(key).map_err(|_| ExtractionError::SourceNotFound)?;
        self.storage
            .get_object(&key)
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| match error {
                StorageError::NotFound(_) => ExtractionError::SourceNotFound,
                StorageError::TooLarge(_) => ExtractionError::ContentTooLarge,
                _ => ExtractionError::Internal,
            })
    }
}

struct LeaseHeartbeatGuard {
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    lost: Arc<AtomicBool>,
}

impl LeaseHeartbeatGuard {
    fn start(
        execution: Arc<dyn ProcessingExecutionUnitOfWork>,
        tenant_id: Uuid,
        task_id: Uuid,
        fence: ExecutionFence,
        heartbeat_interval_secs: i64,
        lease_duration_secs: i64,
    ) -> Self {
        let (stop, mut stop_rx) = oneshot::channel();
        let lost = Arc::new(AtomicBool::new(false));
        let lost_for_task = Arc::clone(&lost);
        let handle = tokio::spawn(async move {
            let interval_secs = u64::try_from(heartbeat_interval_secs.max(1)).unwrap_or(1);
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if execution.heartbeat_ai_task(tenant_id, task_id, &fence, Utc::now(), lease_duration_secs).await.is_err() {
                            lost_for_task.store(true, Ordering::Release);
                            break;
                        }
                    }
                    _ = &mut stop_rx => break,
                }
            }
        });
        Self {
            stop: Some(stop),
            task: handle,
            lost,
        }
    }

    fn ensure_alive(&self) -> Result<(), ExtractionError> {
        if self.lost.load(Ordering::Acquire) {
            Err(ExtractionError::LeaseLost)
        } else {
            Ok(())
        }
    }

    async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = self.task.await;
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

    let (execution, queries, documents) = match config.database.backend {
        WorkerDatabaseBackend::Postgres => {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections((config.concurrency.max(1) * 4).max(8))
                .connect(url.expose())
                .await?;
            let processing = Arc::new(document_processing_postgres::PostgresProcessingStore::new(
                pool.clone(),
            ));
            (
                processing.clone() as Arc<dyn ProcessingExecutionUnitOfWork>,
                processing as Arc<dyn ProcessingJobQuery>,
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
    let services = Arc::new(AiWorkerServices { execution, queries });
    let source = Arc::new(source);
    let permits = Arc::new(tokio::sync::Semaphore::new(config.concurrency as usize));
    let mut task_set = JoinSet::new();

    tracing::info!(worker_id = %config.worker_id, concurrency = config.concurrency, "ai-worker ready");
    let mut shutdown = Box::pin(shutdown_signal());
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!(worker_id = %config.worker_id, "ai-worker graceful shutdown");
                break;
            }
            () = sleep(Duration::from_millis(config.poll_interval_millis)) => {
                let now = Utc::now();
                let _ = services.execution.reclaim_expired_ai_tasks(now).await;
                if let Ok(permit) = Arc::clone(&permits).try_acquire_owned() {
                    if let Some(task) = services.execution.claim_next_ai_task(&config.worker_id, now, config.lease_duration_secs).await? {
                        let services_for_task = Arc::clone(&services);
                        let source_for_task = Arc::clone(&source);
                        let config_for_task = config.clone();
                        task_set.spawn(async move {
                            let _permit = permit;
                            process_task(&services_for_task, &source_for_task, task, &config_for_task).await;
                        });
                    } else {
                        drop(permit);
                    }
                }
            }
        }
    }
    while task_set.join_next().await.is_some() {}
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

async fn process_task(
    services: &AiWorkerServices,
    source: &StorageSource,
    task: AiTask,
    config: &AiWorkerConfig,
) {
    let Some(token) = task.lease_token.clone() else {
        return;
    };
    let fence = ExecutionFence::new(config.worker_id.clone(), token, task.fence_version);
    tracing::info!(task_id = %task.id, job_id = %task.job_id, step = %task.step_kind, fence = task.fence_version, "AI task claimed");
    let heartbeat = LeaseHeartbeatGuard::start(
        Arc::clone(&services.execution),
        task.tenant_id,
        task.id,
        fence.clone(),
        config.heartbeat_interval_secs,
        config.lease_duration_secs,
    );
    if config.test_task_delay_millis > 0 {
        tokio::time::sleep(Duration::from_millis(config.test_task_delay_millis)).await;
    }
    let result = async {
        heartbeat.ensure_alive()?;
        let artifact_key = task
            .input_artifact_id
            .as_deref()
            .ok_or(ExtractionError::SourceNotFound)?;
        let bytes = source.read_artifact(artifact_key).await?;
        let detail = services
            .queries
            .detail(task.tenant_id, task.job_id)
            .await
            .map_err(|_| ExtractionError::Internal)?
            .ok_or(ExtractionError::SourceNotFound)?;
        if bytes.len() > config.max_content_bytes {
            return Err(ExtractionError::ContentTooLarge);
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| ExtractionError::InvalidTextEncoding)?
            .to_string();
        let candidate = DeterministicLocalExtractor
            .extract(ExtractionRequest {
                tenant_id: task.tenant_id,
                job_id: task.job_id,
                content_revision: detail.job.document_content_revision(),
                content_type: "text/plain".to_string(),
                line_count: if text.is_empty() {
                    0
                } else {
                    u32::try_from(text.lines().count()).unwrap_or(u32::MAX)
                },
                character_count: u64::try_from(text.chars().count()).unwrap_or(u64::MAX),
                text,
            })
            .await?;
        heartbeat.ensure_alive()?;
        Ok::<_, ExtractionError>(candidate)
    }
    .await;
    match result {
        Ok(candidate) => {
            let completion = CompleteAiTaskCommand {
                tenant_id: task.tenant_id,
                job_id: task.job_id,
                task_id: task.id,
                fence: fence.clone(),
                candidate,
            };
            if let Err(error) = services
                .execution
                .complete_ai_and_resume(completion, Utc::now())
                .await
            {
                tracing::warn!(task_id = %task.id, error = %error, "AI task completion was fenced");
            }
        }
        Err(error) => {
            tracing::warn!(task_id = %task.id, failure_code = error.code(), "AI task failed");
            let _ = services
                .execution
                .fail_ai_task(
                    task.tenant_id,
                    task.job_id,
                    task.id,
                    classify_failure(&error, task.attempt_count),
                    &fence,
                    Utc::now(),
                )
                .await;
        }
    }
    heartbeat.stop().await;
}

fn classify_failure(error: &ExtractionError, attempt_count: i32) -> ClassifiedProcessingFailure {
    let disposition = match error {
        ExtractionError::AiProviderUnavailable | ExtractionError::Internal => {
            let backoff_secs = match attempt_count {
                1 => 1,
                2 => 5,
                _ => 30,
            };
            ProcessingFailureDisposition::Retry {
                backoff: chrono::Duration::seconds(backoff_secs),
            }
        }
        ExtractionError::LeaseLost => ProcessingFailureDisposition::LeaseLost,
        ExtractionError::Cancelled => ProcessingFailureDisposition::Cancelled,
        _ => ProcessingFailureDisposition::Permanent,
    };
    ClassifiedProcessingFailure {
        code: error.code().to_string(),
        message: None,
        disposition,
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
}
