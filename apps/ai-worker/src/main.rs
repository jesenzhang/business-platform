//! Independent AI-task worker for the fixed document-processing pipeline.
//!
//! The MVP provider is deterministic and local. The worker still exercises a
//! separate `PostgreSQL` task claim/lease/fence boundary so a future provider can
//! be substituted without changing job ownership.

mod config;
mod extractor;
mod retry;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use config::{AiProviderMode, AiWorkerConfig, WorkerDatabaseBackend};
use document::domain::DocumentRepository;
use document_processing::ports::{
    AiTask, CompleteAiTaskCommand, ExecutionFence, ProcessingExecutionUnitOfWork,
    ProcessingJobQuery,
};
use document_processing::{
    DeterministicLocalExtractor, DocumentFieldExtractor, ExtractionError, ExtractionRequest,
    ProcessingSource,
};
use extractor::ModelBackedExtractor;
use object_storage::{LocalStorageClient, ObjectKey, ObjectStorageClient, S3Client, StorageError};
use retry::classify_failure;
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
    tenant_id: Uuid,
    task_id: Uuid,
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
                        if let Err(error) = execution.heartbeat_ai_task(tenant_id, task_id, &fence, Utc::now(), lease_duration_secs).await {
                            lost_for_task.store(true, Ordering::Release);
                            tracing::error!(tenant_id = %tenant_id, task_id = %task_id, error = %error, "AI lease heartbeat failed; stopping work");
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
            tenant_id,
            task_id,
        }
    }

    fn ensure_alive(&self) -> Result<(), ExtractionError> {
        if self.lost.load(Ordering::Acquire) {
            Err(ExtractionError::LeaseLost)
        } else {
            Ok(())
        }
    }

    async fn stop(mut self) -> bool {
        let mut stopped = true;
        if let Some(stop) = self.stop.take() {
            if stop.send(()).is_err() {
                stopped = false;
                tracing::warn!(tenant_id = %self.tenant_id, task_id = %self.task_id, "AI lease heartbeat task was already stopped");
            }
        }
        if let Err(error) = self.task.await {
            stopped = false;
            tracing::error!(tenant_id = %self.tenant_id, task_id = %self.task_id, error = %error, "AI lease heartbeat task join failed");
        }
        stopped
    }
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    let config = AiWorkerConfig::load()?;
    config
        .validate()
        .map_err(|error| anyhow::anyhow!("AI worker configuration invalid: {error}"))?;
    let log_format =
        observability::LogFormat::parse(&config.observability.log_format).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported observability.log_format: {}",
                config.observability.log_format
            )
        })?;
    let _guard = observability::init_tracing(
        "ai-worker",
        &config.observability.log_level,
        log_format,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    let url = config
        .database
        .url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("AI_WORKER__DATABASE__URL is required"))?;
    let storage = build_storage(&config.storage).await?;

    let extractor: Arc<dyn DocumentFieldExtractor> = match config.ai_provider.mode {
        AiProviderMode::Deterministic => Arc::new(DeterministicLocalExtractor),
        AiProviderMode::Real => Arc::new(ModelBackedExtractor::from_config(&config.ai_provider)?),
    };

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
            shutdown_result = &mut shutdown => {
                shutdown_result?;
                tracing::info!(worker_id = %config.worker_id, "ai-worker graceful shutdown");
                break;
            }
            joined = task_set.join_next(), if !task_set.is_empty() => {
                match joined {
                    Some(Ok(())) | None => {}
                    Some(Err(error)) => tracing::error!(worker_id = %config.worker_id, error = %error, "AI task join failed"),
                }
                let reap = reap_completed_tasks(&mut task_set, &config.worker_id);
                if reap.joined > 0 {
                    tracing::debug!(worker_id = %config.worker_id, completed = reap.completed, join_errors = reap.join_errors, "reaped completed AI tasks");
                }
            }
            () = sleep(Duration::from_millis(config.poll_interval_millis)) => {
                let now = Utc::now();
                match services.execution.reclaim_expired_ai_tasks(now).await {
                    Ok(reclaimed) if reclaimed > 0 => tracing::info!(worker_id = %config.worker_id, reclaimed, "expired AI leases reclaimed"),
                    Ok(_) => {}
                    Err(error) => tracing::error!(worker_id = %config.worker_id, error = %error, "failed to reclaim expired AI leases"),
                }
                if let Ok(permit) = Arc::clone(&permits).try_acquire_owned() {
                    let task = match services.execution.claim_next_ai_task(&config.worker_id, now, config.lease_duration_secs).await {
                        Ok(task) => task,
                        Err(error) => {
                            tracing::error!(worker_id = %config.worker_id, error = %error, "failed to claim AI task");
                            drop(permit);
                            continue;
                        }
                    };
                    if let Some(task) = task {
                        let services_for_task = Arc::clone(&services);
                        let source_for_task = Arc::clone(&source);
                        let extractor_for_task = Arc::clone(&extractor);
                        let config_for_task = config.clone();
                        task_set.spawn(async move {
                            let _permit = permit;
                            process_task(
                                &services_for_task,
                                &source_for_task,
                                task,
                                &config_for_task,
                                extractor_for_task.as_ref(),
                            )
                            .await;
                        });
                    } else {
                        drop(permit);
                    }
                }
            }
        }
    }
    while let Some(joined) = task_set.join_next().await {
        if let Err(error) = joined {
            tracing::error!(worker_id = %config.worker_id, error = %error, "AI task join failed during graceful drain");
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

#[allow(clippy::too_many_lines)]
async fn process_task(
    services: &AiWorkerServices,
    source: &StorageSource,
    task: AiTask,
    config: &AiWorkerConfig,
    extractor: &dyn DocumentFieldExtractor,
) {
    let Some(token) = task.lease_token.clone() else {
        tracing::error!(tenant_id = %task.tenant_id, task_id = %task.id, "claimed AI task has no lease token; refusing work");
        return;
    };
    let fence = ExecutionFence::new(config.worker_id.clone(), token, task.fence_version);
    tracing::info!(task_id = %task.id, job_id = %task.job_id, step = %task.step_kind, fence = task.fence_version, correlation_id = task.correlation_id.as_deref().unwrap_or("-"), "AI task claimed");
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
        let candidate = extractor
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
    let heartbeat_lost_flag = Arc::clone(&heartbeat.lost);
    let heartbeat_stopped = heartbeat.stop().await;
    let heartbeat_lost = heartbeat_lost_flag.load(Ordering::Acquire);
    match result {
        Ok(candidate) if heartbeat_stopped && !heartbeat_lost => {
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
        Ok(_) => {
            tracing::error!(tenant_id = %task.tenant_id, task_id = %task.id, "AI task result discarded because lease state was not proven");
        }
        Err(error) if heartbeat_stopped && !heartbeat_lost => {
            tracing::warn!(task_id = %task.id, failure_code = error.code(), correlation_id = task.correlation_id.as_deref().unwrap_or("-"), "AI task failed");
            if let Err(persistence_error) = services
                .execution
                .fail_ai_task(
                    task.tenant_id,
                    task.job_id,
                    task.id,
                    classify_failure(&error, task.attempt_count),
                    &fence,
                    Utc::now(),
                )
                .await
            {
                tracing::error!(tenant_id = %task.tenant_id, task_id = %task.id, error = %persistence_error, "failed to persist AI task failure");
            }
        }
        Err(error) => {
            tracing::warn!(tenant_id = %task.tenant_id, task_id = %task.id, failure_code = error.code(), correlation_id = task.correlation_id.as_deref().unwrap_or("-"), "AI task failed without a provable lease; state transition skipped");
        }
    }
}

async fn shutdown_signal() -> anyhow::Result<()> {
    tokio::signal::ctrl_c().await.map_err(Into::into)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct JoinSetReapSummary {
    joined: usize,
    completed: usize,
    join_errors: usize,
}

fn reap_completed_tasks(tasks: &mut JoinSet<()>, worker_id: &str) -> JoinSetReapSummary {
    let mut summary = JoinSetReapSummary::default();
    while let Some(joined) = tasks.try_join_next() {
        summary.joined += 1;
        match joined {
            Ok(()) => summary.completed += 1,
            Err(error) => {
                summary.join_errors += 1;
                tracing::error!(worker_id, error = %error, "AI task join failed while reaping");
            }
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::{reap_completed_tasks, JoinSet, JoinSetReapSummary};

    #[tokio::test]
    #[allow(clippy::panic)]
    async fn reaps_completed_and_panicked_tasks_without_accumulation() {
        let mut tasks = JoinSet::new();
        tasks.spawn(async {});
        tasks.spawn(async {
            panic!("regression panic");
        });
        tasks.spawn(async {});

        let mut total = JoinSetReapSummary::default();
        while !tasks.is_empty() {
            let current = reap_completed_tasks(&mut tasks, "test-ai-worker");
            total.joined += current.joined;
            total.completed += current.completed;
            total.join_errors += current.join_errors;
            if !tasks.is_empty() {
                tokio::task::yield_now().await;
            }
        }

        assert_eq!(total.joined, 3);
        assert_eq!(total.completed, 2);
        assert_eq!(total.join_errors, 1);
    }
}
