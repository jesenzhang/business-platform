//! Durable business worker for the fixed document-processing pipeline.

mod config;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use config::{AiMode, BusinessWorkerConfig, WorkerDatabaseBackend};
use document::domain::DocumentRepository;
use document_processing::ports::{
    ClassifiedProcessingFailure, ExecutionFence, ProcessingExecutionUnitOfWork,
    ProcessingFailureDisposition, ProcessingJobQuery, StepCheckpoint, TextArtifactReference,
};
use document_processing::{
    extract_text_artifact, DeterministicLocalExtractor, DocumentFieldExtractor, ExtractionError,
    ExtractionRequest, ProcessingSource,
};
use object_storage::{LocalStorageClient, ObjectKey, ObjectStorageClient, S3Client, StorageError};
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, Instant};
use uuid::Uuid;

struct WorkerServices {
    execution: Arc<dyn ProcessingExecutionUnitOfWork>,
    queries: Arc<dyn ProcessingJobQuery>,
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

impl StorageSource {
    async fn put_text_artifact(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        artifact: &document_processing::TextArtifact,
    ) -> Result<String, ExtractionError> {
        let key = format!(
            "tenants/{tenant_id}/processing-jobs/{job_id}/artifacts/text/{}.txt",
            artifact.content_hash
        );
        let object_key = ObjectKey::new(&key).map_err(|_| ExtractionError::Internal)?;
        self.storage
            .put_object(
                &object_key,
                Bytes::from(artifact.text.as_bytes().to_vec()),
                "text/plain; charset=utf-8",
            )
            .await
            .map_err(|_| ExtractionError::Internal)?;
        Ok(key)
    }
}

struct LeaseHeartbeatGuard {
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    lost: Arc<AtomicBool>,
    tenant_id: Uuid,
    job_id: Uuid,
}

impl LeaseHeartbeatGuard {
    fn start(
        execution: Arc<dyn ProcessingExecutionUnitOfWork>,
        tenant_id: Uuid,
        job_id: Uuid,
        fence: ExecutionFence,
        heartbeat_interval_secs: i64,
        lease_duration_secs: i64,
    ) -> Self {
        let (stop, mut stop_rx) = oneshot::channel();
        let lost = Arc::new(AtomicBool::new(false));
        let lost_for_task = Arc::clone(&lost);
        let task = tokio::spawn(async move {
            let interval_secs = u64::try_from(heartbeat_interval_secs.max(1)).unwrap_or(1);
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(error) = execution.heartbeat_job(tenant_id, job_id, &fence, Utc::now(), lease_duration_secs).await {
                            lost_for_task.store(true, Ordering::Release);
                            tracing::error!(tenant_id = %tenant_id, job_id = %job_id, error = %error, "processing lease heartbeat failed; stopping work");
                            break;
                        }
                    }
                    _ = &mut stop_rx => break,
                }
            }
        });
        Self {
            stop: Some(stop),
            task,
            lost,
            tenant_id,
            job_id,
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
                tracing::warn!(tenant_id = %self.tenant_id, job_id = %self.job_id, "processing lease heartbeat task was already stopped");
            }
        }
        if let Err(error) = self.task.await {
            stopped = false;
            tracing::error!(tenant_id = %self.tenant_id, job_id = %self.job_id, error = %error, "processing lease heartbeat task join failed");
        }
        stopped
    }
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    let config = BusinessWorkerConfig::load()?;
    config
        .validate()
        .map_err(|error| anyhow::anyhow!("worker configuration invalid: {error}"))?;
    let log_format =
        observability::LogFormat::parse(&config.observability.log_format).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported observability.log_format: {}",
                config.observability.log_format
            )
        })?;
    let _guard = observability::init_tracing(
        "business-worker",
        &config.observability.log_level,
        log_format,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    let storage = build_storage(&config.storage).await?;

    let (execution, queries, documents) =
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
                    processing.clone() as Arc<dyn ProcessingExecutionUnitOfWork>,
                    processing as Arc<dyn ProcessingJobQuery>,
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
                    processing.clone() as Arc<dyn ProcessingExecutionUnitOfWork>,
                    processing as Arc<dyn ProcessingJobQuery>,
                    document as Arc<dyn DocumentRepository>,
                )
            }
        };
    let source = StorageSource { documents, storage };
    let services = WorkerServices { execution, queries };

    tracing::info!(worker_id = %config.worker_id, concurrency = config.concurrency, "business-worker ready");
    let mut shutdown = Box::pin(shutdown_signal());
    let mut next_poll = Instant::now();
    let services = Arc::new(services);
    let source = Arc::new(source);
    let permits = Arc::new(tokio::sync::Semaphore::new(config.concurrency as usize));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            shutdown_result = &mut shutdown => {
                shutdown_result?;
                tracing::info!(worker_id = %config.worker_id, "business-worker graceful shutdown");
                break;
            }
            joined = tasks.join_next(), if !tasks.is_empty() => {
                match joined {
                    Some(Ok(())) | None => {}
                    Some(Err(error)) => tracing::error!(worker_id = %config.worker_id, error = %error, "processing task join failed"),
                }
                let reap = reap_completed_tasks(&mut tasks, &config.worker_id);
                if reap.joined > 0 {
                    tracing::debug!(worker_id = %config.worker_id, completed = reap.completed, join_errors = reap.join_errors, "reaped completed processing tasks");
                }
            }
            () = sleep(next_poll.saturating_duration_since(Instant::now())) => {
                let now = Utc::now();
                match services.execution.reclaim_expired_jobs(now).await {
                    Ok(reclaimed) if reclaimed > 0 => tracing::info!(worker_id = %config.worker_id, reclaimed, "expired processing leases reclaimed"),
                    Ok(_) => {}
                    Err(error) => tracing::error!(worker_id = %config.worker_id, error = %error, "failed to reclaim expired processing leases"),
                }
                if let Ok(permit) = Arc::clone(&permits).try_acquire_owned() {
                    let claimed = match services.execution.claim_next_job(&config.worker_id, now, config.lease_duration_secs).await {
                        Ok(claimed) => claimed,
                        Err(error) => {
                            tracing::error!(worker_id = %config.worker_id, error = %error, "failed to claim processing job");
                            drop(permit);
                            next_poll = Instant::now() + Duration::from_millis(config.poll_interval_millis);
                            continue;
                        }
                    };
                    if let Some(claimed) = claimed {
                        tracing::info!(job_id = %claimed.job.id(), document_id = %claimed.job.document_id(), step = %claimed.job.current_step(), fence = claimed.fence_version, correlation_id = claimed.job.correlation_id().unwrap_or("-"), "processing job claimed");
                        let services_for_task = Arc::clone(&services);
                        let source_for_task = Arc::clone(&source);
                        let config_for_task = config.clone();
                        tasks.spawn(async move {
                            let _permit = permit;
                            process_claimed(&services_for_task, &source_for_task, claimed, &config_for_task).await;
                        });
                    } else {
                        drop(permit);
                    }
                }
                next_poll = Instant::now() + Duration::from_millis(config.poll_interval_millis);
            }
        }
    }
    while let Some(joined) = tasks.join_next().await {
        if let Err(error) = joined {
            tracing::error!(worker_id = %config.worker_id, error = %error, "processing task join failed during graceful drain");
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

async fn process_claimed(
    services: &WorkerServices,
    source: &StorageSource,
    claimed: document_processing::ports::ClaimedProcessingJob,
    config: &BusinessWorkerConfig,
) {
    let job = claimed.job;
    let fence = ExecutionFence::new(
        config.worker_id.clone(),
        claimed.lease_token.clone(),
        claimed.fence_version,
    );
    let heartbeat = LeaseHeartbeatGuard::start(
        Arc::clone(&services.execution),
        job.tenant_id(),
        job.id(),
        fence.clone(),
        config.heartbeat_interval_secs,
        config.lease_duration_secs,
    );
    if config.test_step_delay_millis > 0 {
        sleep(Duration::from_millis(config.test_step_delay_millis)).await;
    }
    let step = job.current_step();
    let result = process_step(services, source, &job, &fence, config, &heartbeat).await;
    let heartbeat_lost_flag = Arc::clone(&heartbeat.lost);
    let heartbeat_stopped = heartbeat.stop().await;
    let heartbeat_lost = heartbeat_lost_flag.load(Ordering::Acquire);
    if let Err(error) = result {
        if !matches!(error, ExtractionError::LeaseLost) && !heartbeat_lost && heartbeat_stopped {
            let failure = classify_failure(&error, job.attempt_count());
            if let Err(persistence_error) = services
                .execution
                .retry_or_fail_step(job.tenant_id(), job.id(), step, failure, &fence, Utc::now())
                .await
            {
                tracing::error!(tenant_id = %job.tenant_id(), job_id = %job.id(), step = %step, error = %persistence_error, "failed to persist processing step failure");
            }
        } else if !heartbeat_stopped || heartbeat_lost {
            tracing::error!(tenant_id = %job.tenant_id(), job_id = %job.id(), step = %step, "processing step result discarded because lease state was not proven");
        }
        tracing::warn!(job_id = %job.id(), step = %step, failure_code = error.code(), correlation_id = job.correlation_id().unwrap_or("-"), "processing job step failed");
    } else if !heartbeat_lost
        && heartbeat_stopped
        && job.status() == document_processing::ProcessingJobStatus::Running
    {
        if let Err(error) = services
            .execution
            .release_job(job.tenant_id(), job.id(), &fence, Utc::now())
            .await
        {
            tracing::error!(tenant_id = %job.tenant_id(), job_id = %job.id(), step = %step, error = %error, "failed to release processing job lease");
        }
    } else if !heartbeat_stopped || heartbeat_lost {
        tracing::error!(tenant_id = %job.tenant_id(), job_id = %job.id(), step = %step, "processing job lease was not released because lease state was not proven");
    }
}

#[allow(clippy::too_many_lines)]
async fn process_step(
    services: &WorkerServices,
    source: &StorageSource,
    job: &document_processing::ProcessingJob,
    fence: &ExecutionFence,
    config: &BusinessWorkerConfig,
    heartbeat: &LeaseHeartbeatGuard,
) -> Result<(), ExtractionError> {
    use document_processing::ProcessingStepKind;
    let now = Utc::now();
    match job.current_step() {
        ProcessingStepKind::ValidateSource => {
            source
                .read_source(
                    job.tenant_id(),
                    job.document_id(),
                    job.document_content_revision(),
                )
                .await?;
            heartbeat.ensure_alive()?;
            services
                .execution
                .start_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::ValidateSource,
                    fence,
                    now,
                )
                .await
                .map_err(map_repository_error)?;
            services
                .execution
                .complete_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::ValidateSource,
                    None,
                    fence,
                    Utc::now(),
                )
                .await
                .map_err(map_repository_error)?;
        }
        ProcessingStepKind::DetectType => {
            let (content_type, _) = source
                .read_source(
                    job.tenant_id(),
                    job.document_id(),
                    job.document_content_revision(),
                )
                .await?;
            if !matches!(
                content_type.as_str(),
                "text/plain" | "text/markdown" | "application/json"
            ) {
                return Err(ExtractionError::UnsupportedContentType);
            }
            heartbeat.ensure_alive()?;
            services
                .execution
                .start_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::DetectType,
                    fence,
                    now,
                )
                .await
                .map_err(map_repository_error)?;
            services
                .execution
                .complete_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::DetectType,
                    None,
                    fence,
                    Utc::now(),
                )
                .await
                .map_err(map_repository_error)?;
        }
        ProcessingStepKind::ExtractText => {
            let (content_type, bytes) = source
                .read_source(
                    job.tenant_id(),
                    job.document_id(),
                    job.document_content_revision(),
                )
                .await?;
            let artifact = extract_text_artifact(
                &content_type,
                job.document_content_revision(),
                &bytes,
                config.max_content_bytes,
            )?;
            let key = source
                .put_text_artifact(job.tenant_id(), job.id(), &artifact)
                .await?;
            let reference = TextArtifactReference {
                key,
                content_hash: artifact.content_hash,
                content_revision: artifact.content_revision,
                byte_count: artifact.byte_count,
                line_count: artifact.line_count,
                character_count: artifact.character_count,
            };
            heartbeat.ensure_alive()?;
            services
                .execution
                .start_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::ExtractText,
                    fence,
                    now,
                )
                .await
                .map_err(map_repository_error)?;
            if config.ai_mode == AiMode::Separate {
                services
                    .execution
                    .enqueue_ai_and_wait(job.tenant_id(), job.id(), reference, fence, Utc::now())
                    .await
                    .map_err(map_repository_error)?;
            } else {
                let checkpoint = StepCheckpoint {
                    job_id: job.id(),
                    tenant_id: job.tenant_id(),
                    step_kind: ProcessingStepKind::ExtractText,
                    attempt_number: job.attempt_count(),
                    checkpoint_json: serde_json::json!({"text_artifact_reference": reference.key, "content_hash": reference.content_hash, "content_revision": reference.content_revision, "byte_count": reference.byte_count, "line_count": reference.line_count, "character_count": reference.character_count}),
                    updated_at: Utc::now(),
                };
                services
                    .execution
                    .complete_step(
                        job.tenant_id(),
                        job.id(),
                        ProcessingStepKind::ExtractText,
                        Some(checkpoint),
                        fence,
                        Utc::now(),
                    )
                    .await
                    .map_err(map_repository_error)?;
            }
        }
        ProcessingStepKind::ExtractFields => {
            if config.ai_mode == AiMode::Separate {
                return Err(ExtractionError::Internal);
            }
            heartbeat.ensure_alive()?;
            services
                .execution
                .start_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::ExtractFields,
                    fence,
                    now,
                )
                .await
                .map_err(map_repository_error)?;
            services
                .execution
                .complete_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::ExtractFields,
                    None,
                    fence,
                    Utc::now(),
                )
                .await
                .map_err(map_repository_error)?;
        }
        ProcessingStepKind::ValidateCandidate => {
            heartbeat.ensure_alive()?;
            services
                .execution
                .start_step(
                    job.tenant_id(),
                    job.id(),
                    ProcessingStepKind::ValidateCandidate,
                    fence,
                    now,
                )
                .await
                .map_err(map_repository_error)?;
            let candidate = match services
                .queries
                .detail(job.tenant_id(), job.id())
                .await
                .map_err(map_repository_error)?
                .and_then(|detail| detail.candidate)
            {
                Some(candidate) => candidate,
                None if config.ai_mode == AiMode::Inline => {
                    let (content_type, bytes) = source
                        .read_source(
                            job.tenant_id(),
                            job.document_id(),
                            job.document_content_revision(),
                        )
                        .await?;
                    let artifact = extract_text_artifact(
                        &content_type,
                        job.document_content_revision(),
                        &bytes,
                        config.max_content_bytes,
                    )?;
                    DeterministicLocalExtractor
                        .extract(ExtractionRequest {
                            tenant_id: job.tenant_id(),
                            job_id: job.id(),
                            content_revision: artifact.content_revision,
                            content_type,
                            text: artifact.text,
                            line_count: artifact.line_count,
                            character_count: artifact.character_count,
                        })
                        .await?
                }
                None => return Err(ExtractionError::Internal),
            };
            candidate
                .payload
                .validate(config.max_content_bytes)
                .map_err(|_| ExtractionError::CandidateValidationFailed)?;
            services
                .execution
                .save_candidate_and_wait_for_review(
                    job.tenant_id(),
                    job.id(),
                    &candidate,
                    fence,
                    Utc::now(),
                )
                .await
                .map_err(map_repository_error)?;
        }
        ProcessingStepKind::AwaitReview => return Ok(()),
    }
    Ok(())
}

fn classify_failure(error: &ExtractionError, attempt_count: i32) -> ClassifiedProcessingFailure {
    let disposition = match error {
        ExtractionError::AiProviderRateLimited { retry_after } => {
            // Honour the provider pacing hint when present (platform-capped),
            // otherwise fall back to this worker's own backoff ladder.
            let backoff = retry_after.map_or_else(
                || {
                    let backoff_secs = match attempt_count {
                        0 => 1,
                        1 => 5,
                        _ => 30,
                    };
                    chrono::Duration::seconds(backoff_secs)
                },
                document_processing::ports::capped_provider_retry_after,
            );
            ProcessingFailureDisposition::Retry { backoff }
        }
        ExtractionError::AiProviderUnavailable | ExtractionError::Internal => {
            let backoff_secs = match attempt_count {
                0 => 1,
                1 => 5,
                _ => 30,
            };
            ProcessingFailureDisposition::Retry {
                backoff: chrono::Duration::seconds(backoff_secs),
            }
        }
        ExtractionError::Cancelled => ProcessingFailureDisposition::Cancelled,
        ExtractionError::LeaseLost => ProcessingFailureDisposition::LeaseLost,
        // Everything else — including `AiProviderRejected` (a configuration
        // fault that a retry would repeat) and invalid responses — is
        // permanent. The deterministic local extractor never emits the AI
        // provider variants, but the table stays consistent with `ai-worker`.
        _ => ProcessingFailureDisposition::Permanent,
    };
    ClassifiedProcessingFailure {
        code: error.code().to_string(),
        message: None,
        disposition,
    }
}

#[allow(clippy::needless_pass_by_value)]
fn map_repository_error(error: document_processing::ProcessingRepositoryError) -> ExtractionError {
    match error {
        document_processing::ProcessingRepositoryError::LeaseLost => ExtractionError::LeaseLost,
        document_processing::ProcessingRepositoryError::NotFound
        | document_processing::ProcessingRepositoryError::TenantMismatch
        | document_processing::ProcessingRepositoryError::Conflict
        | document_processing::ProcessingRepositoryError::IdempotencyConflict => {
            ExtractionError::Internal
        }
        document_processing::ProcessingRepositoryError::Unavailable
        | document_processing::ProcessingRepositoryError::Failed => ExtractionError::Internal,
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
                tracing::error!(worker_id, error = %error, "processing task join failed while reaping");
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
            let current = reap_completed_tasks(&mut tasks, "test-business-worker");
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
