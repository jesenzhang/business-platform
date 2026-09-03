//! Independent AI-task worker for the fixed document-processing pipeline.
//!
//! The MVP provider is deterministic and local. The worker still exercises a
//! separate `PostgreSQL` task claim/lease/fence boundary so a future provider can
//! be substituted without changing job ownership.

mod config;
mod extractor;
mod metrics;
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
    ProcessingJobQuery, ProcessingRepositoryError,
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
    // PLAN-0012 T4.2: workers expose Prometheus metrics on an internal
    // address; production validation already requires it to be configured.
    let _metrics_server = match config.observability.metrics_addr.as_deref() {
        Some(addr) => {
            observability::metrics::install_metrics();
            Some(observability::metrics::spawn_metrics_server(addr).await?)
        }
        None => None,
    };
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
                    Ok(reclaimed) if reclaimed > 0 => {
                        metrics::record_leases_reclaimed(reclaimed);
                        tracing::info!(worker_id = %config.worker_id, reclaimed, "expired AI leases reclaimed");
                    }
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
                        metrics::record_queue_wait(
                            now.signed_duration_since(task.created_at).num_milliseconds(),
                        );
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
    let started = std::time::Instant::now();
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
    metrics::record_task_duration(started.elapsed().as_secs_f64());
    match result {
        Ok(candidate) if heartbeat_stopped && !heartbeat_lost => {
            let completion = CompleteAiTaskCommand {
                tenant_id: task.tenant_id,
                job_id: task.job_id,
                task_id: task.id,
                fence: fence.clone(),
                candidate,
            };
            // PLAN-0012 release closure: `succeeded` is recorded only after the
            // fenced completion has durably persisted. A lease-lost completion
            // is lease-unproven and increments `ai_lease_lost_total`; any other
            // persistence error is `failed` and must not inflate the lease-loss
            // signal. Every attempt contributes exactly one `ai_tasks_total`
            // increment.
            match services
                .execution
                .complete_ai_and_resume(completion, Utc::now())
                .await
            {
                Ok(_) => metrics::record_task_outcome(metrics::TaskOutcome::Succeeded),
                Err(ProcessingRepositoryError::LeaseLost) => {
                    metrics::record_task_outcome(metrics::TaskOutcome::LeaseUnproven);
                    metrics::record_lease_lost();
                    tracing::warn!(task_id = %task.id, "AI task completion was fenced; attempt did not complete durably");
                }
                Err(error) => {
                    metrics::record_task_outcome(metrics::TaskOutcome::Failed);
                    tracing::warn!(task_id = %task.id, error = %error, "AI task completion failed to persist; attempt did not complete durably");
                }
            }
        }
        Ok(_) => {
            metrics::record_task_outcome(metrics::TaskOutcome::LeaseUnproven);
            metrics::record_lease_lost();
            tracing::error!(tenant_id = %task.tenant_id, task_id = %task.id, "AI task result discarded because lease state was not proven");
        }
        Err(error) if heartbeat_stopped && !heartbeat_lost => {
            metrics::record_task_outcome(metrics::TaskOutcome::Failed);
            metrics::record_disposition(&classify_failure(&error, task.attempt_count).disposition);
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
            metrics::record_task_outcome(metrics::TaskOutcome::LeaseUnproven);
            metrics::record_lease_lost();
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

    /// Regression tests for the AI-task completion boundary (PLAN-0012 release
    /// closure): `ai_tasks_total{outcome="succeeded"}` must be recorded only
    /// after `complete_ai_and_resume` durably persisted the fenced completion;
    /// a `LeaseLost` completion is `lease_unproven` and increments
    /// `ai_lease_lost_total`, while any other persistence error is `failed`
    /// without touching the lease-loss counter. Every attempt must contribute
    /// exactly one final outcome.
    mod completion_outcome {
        use std::collections::BTreeMap;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

        use async_trait::async_trait;
        use bytes::Bytes;
        use chrono::{DateTime, Duration, Utc};
        use document::domain::{
            AggregateVersion, DocumentMetadata, DocumentRepository, DocumentRevision,
            RepositoryError,
        };
        use document_processing::domain::{ExtractionCandidate, ProcessingJob, ProcessingStepKind};
        use document_processing::ports::{
            AiTask, ClaimedProcessingJob, ClassifiedProcessingFailure, CompleteAiTaskCommand,
            ExecutionFence, FinalizeReviewCommand, FinalizeReviewResult,
            ProcessingExecutionUnitOfWork, ProcessingJobDetail, ProcessingJobListRequest,
            ProcessingJobPage, ProcessingJobQuery, ProcessingJobStatusCounts,
            ProcessingRepositoryError, StepCheckpoint, TextArtifactReference,
        };
        use document_processing::DeterministicLocalExtractor;
        use metrics::{
            Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata,
            Recorder, SharedString, Unit,
        };
        use object_storage::{
            ObjectKey, ObjectMetadata, ObjectStorageClient, ObjectStream, StorageError,
            StoredObject,
        };
        use uuid::Uuid;

        use super::super::{process_task, AiWorkerServices, StorageSource};
        use crate::config::AiWorkerConfig;

        // ---------------------------------------------------- metric capture --

        /// Counter-only `metrics` recorder. Only counter increments are stored;
        /// gauges/histograms are no-ops because the regression tests assert the
        /// bounded `ai_tasks_total{outcome}` labels and `ai_lease_lost_total`.
        #[derive(Default)]
        struct CountingRecorder {
            totals: Arc<Mutex<BTreeMap<Key, u64>>>,
        }

        impl CountingRecorder {
            fn counter(&self, name: &str, label: Option<(&str, &str)>) -> u64 {
                let totals = self.totals.lock().unwrap_or_else(PoisonError::into_inner);
                totals
                    .iter()
                    .filter(|(key, _)| {
                        key.name() == name
                            && label.is_none_or(|(k, v)| {
                                key.labels().any(|l| l.key() == k && l.value() == v)
                            })
                    })
                    .map(|(_, total)| *total)
                    .sum()
            }
        }

        struct SharedCounter {
            key: Key,
            totals: Arc<Mutex<BTreeMap<Key, u64>>>,
        }

        impl CounterFn for SharedCounter {
            fn increment(&self, value: u64) {
                let mut totals = self.totals.lock().unwrap_or_else(PoisonError::into_inner);
                *totals.entry(self.key.clone()).or_default() += value;
            }
            fn absolute(&self, value: u64) {
                let mut totals = self.totals.lock().unwrap_or_else(PoisonError::into_inner);
                *totals.entry(self.key.clone()).or_default() = value;
            }
        }

        struct NoopGauge;
        impl GaugeFn for NoopGauge {
            fn increment(&self, _value: f64) {}
            fn decrement(&self, _value: f64) {}
            fn set(&self, _value: f64) {}
        }

        struct NoopHistogram;
        impl HistogramFn for NoopHistogram {
            fn record(&self, _value: f64) {}
        }

        impl Recorder for CountingRecorder {
            fn describe_counter(
                &self,
                _key: KeyName,
                _unit: Option<Unit>,
                _description: SharedString,
            ) {
            }
            fn describe_gauge(
                &self,
                _key: KeyName,
                _unit: Option<Unit>,
                _description: SharedString,
            ) {
            }
            fn describe_histogram(
                &self,
                _key: KeyName,
                _unit: Option<Unit>,
                _description: SharedString,
            ) {
            }
            fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
                Counter::from_arc(Arc::new(SharedCounter {
                    key: key.clone(),
                    totals: Arc::clone(&self.totals),
                }))
            }
            fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
                Gauge::from_arc(Arc::new(NoopGauge))
            }
            fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
                Histogram::from_arc(Arc::new(NoopHistogram))
            }
        }

        fn install_test_metrics() -> Arc<CountingRecorder> {
            static RECORDER: OnceLock<Arc<CountingRecorder>> = OnceLock::new();
            Arc::clone(RECORDER.get_or_init(|| {
                let recorder = Arc::new(CountingRecorder::default());
                if metrics::set_global_recorder(Arc::clone(&recorder)).is_err() {
                    unreachable!("no other recorder is installed in the ai-worker test binary");
                }
                Arc::clone(&recorder)
            }))
        }

        /// Serializes the counter-delta assertions so the three tests below
        /// cannot observe each other's increments on the global recorder.
        fn metrics_lock() -> MutexGuard<'static, ()> {
            static LOCK: Mutex<()> = Mutex::new(());
            LOCK.lock().unwrap_or_else(PoisonError::into_inner)
        }

        // --------------------------------------------------------------- fakes --

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum CompletionOutcome {
            Persisted,
            Fenced,
            PersistenceFailed,
        }

        struct FakeExecution {
            outcome: CompletionOutcome,
            complete_calls: AtomicUsize,
            fail_calls: AtomicUsize,
            last_completion: Mutex<Option<CompleteAiTaskCommand>>,
        }

        impl FakeExecution {
            fn new(outcome: CompletionOutcome) -> Self {
                Self {
                    outcome,
                    complete_calls: AtomicUsize::new(0),
                    fail_calls: AtomicUsize::new(0),
                    last_completion: Mutex::new(None),
                }
            }
            fn recorded_completion(&self) -> Option<CompleteAiTaskCommand> {
                self.last_completion
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone()
            }
        }

        #[async_trait]
        impl ProcessingExecutionUnitOfWork for FakeExecution {
            async fn create_job(
                &self,
                _job: &ProcessingJob,
            ) -> Result<ProcessingJob, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn claim_next_job(
                &self,
                _worker_id: &str,
                _now: DateTime<Utc>,
                _lease_duration_secs: i64,
            ) -> Result<Option<ClaimedProcessingJob>, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn claim_next_ai_task(
                &self,
                _worker_id: &str,
                _now: DateTime<Utc>,
                _lease_duration_secs: i64,
            ) -> Result<Option<AiTask>, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn start_step(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _expected_step: ProcessingStepKind,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
            ) -> Result<ProcessingJob, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn complete_step(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _completed_step: ProcessingStepKind,
                _checkpoint: Option<StepCheckpoint>,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
            ) -> Result<ProcessingJob, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn retry_or_fail_step(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _step: ProcessingStepKind,
                _failure: ClassifiedProcessingFailure,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
            ) -> Result<ProcessingJob, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn enqueue_ai_and_wait(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _text_artifact: TextArtifactReference,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
            ) -> Result<AiTask, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn complete_ai_and_resume(
                &self,
                completion: CompleteAiTaskCommand,
                now: DateTime<Utc>,
            ) -> Result<ProcessingJob, ProcessingRepositoryError> {
                self.complete_calls.fetch_add(1, Ordering::SeqCst);
                *self
                    .last_completion
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(completion.clone());
                match self.outcome {
                    CompletionOutcome::Persisted => ProcessingJob::queue(
                        completion.tenant_id,
                        Uuid::now_v7(),
                        1,
                        format!("completed-{}", completion.task_id),
                        Uuid::now_v7(),
                        3,
                        now,
                    )
                    .map_err(|_| ProcessingRepositoryError::Failed),
                    CompletionOutcome::Fenced => Err(ProcessingRepositoryError::LeaseLost),
                    CompletionOutcome::PersistenceFailed => Err(ProcessingRepositoryError::Failed),
                }
            }
            async fn fail_ai_task(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _task_id: Uuid,
                _failure: ClassifiedProcessingFailure,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
            ) -> Result<AiTask, ProcessingRepositoryError> {
                self.fail_calls.fetch_add(1, Ordering::SeqCst);
                Err(ProcessingRepositoryError::Failed)
            }
            async fn save_candidate_and_wait_for_review(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _candidate: &ExtractionCandidate,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
            ) -> Result<ProcessingJob, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn finalize_review(
                &self,
                _command: FinalizeReviewCommand,
                _now: DateTime<Utc>,
            ) -> Result<FinalizeReviewResult, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn cancel_processing(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _requested_by: Uuid,
                _now: DateTime<Utc>,
            ) -> Result<ProcessingJob, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn heartbeat_job(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
                _lease_duration_secs: i64,
            ) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn release_job(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
                _fence: &ExecutionFence,
                _now: DateTime<Utc>,
            ) -> Result<(), ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn reclaim_expired_jobs(
                &self,
                _now: DateTime<Utc>,
            ) -> Result<u64, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn heartbeat_ai_task(
                &self,
                _tenant_id: Uuid,
                _task_id: Uuid,
                _fence: &ExecutionFence,
                now: DateTime<Utc>,
                lease_duration_secs: i64,
            ) -> Result<DateTime<Utc>, ProcessingRepositoryError> {
                Ok(now + Duration::seconds(lease_duration_secs))
            }
            async fn reclaim_expired_ai_tasks(
                &self,
                _now: DateTime<Utc>,
            ) -> Result<u64, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
        }

        struct FakeQuery {
            detail: ProcessingJobDetail,
        }

        #[async_trait]
        impl ProcessingJobQuery for FakeQuery {
            async fn status_counts(
                &self,
                _tenant_id: Uuid,
            ) -> Result<ProcessingJobStatusCounts, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn detail(
                &self,
                _tenant_id: Uuid,
                _job_id: Uuid,
            ) -> Result<Option<ProcessingJobDetail>, ProcessingRepositoryError> {
                Ok(Some(self.detail.clone()))
            }
            async fn list(
                &self,
                _request: ProcessingJobListRequest,
            ) -> Result<ProcessingJobPage, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
            async fn list_for_document(
                &self,
                _tenant_id: Uuid,
                _document_id: Uuid,
            ) -> Result<Vec<ProcessingJobDetail>, ProcessingRepositoryError> {
                Err(ProcessingRepositoryError::NotFound)
            }
        }

        struct FakeStorage {
            body: Bytes,
        }

        #[async_trait]
        impl ObjectStorageClient for FakeStorage {
            async fn put_stream(
                &self,
                _key: &ObjectKey,
                _body: ObjectStream,
                _content_length: u64,
                _content_type: &str,
                _metadata: &BTreeMap<String, String>,
            ) -> Result<(), StorageError> {
                Err(StorageError::Config("unused in tests".to_string()))
            }
            async fn open_stream(&self, _key: &ObjectKey) -> Result<StoredObject, StorageError> {
                let content_length = u64::try_from(self.body.len()).unwrap_or(u64::MAX);
                let body = self.body.clone();
                Ok(StoredObject {
                    body: Box::pin(futures_util::stream::iter([Ok::<_, StorageError>(body)])),
                    metadata: ObjectMetadata {
                        content_length,
                        content_type: Some("text/plain".to_string()),
                        ..ObjectMetadata::default()
                    },
                })
            }
            async fn head(&self, _key: &ObjectKey) -> Result<ObjectMetadata, StorageError> {
                Ok(ObjectMetadata {
                    content_length: u64::try_from(self.body.len()).unwrap_or(u64::MAX),
                    content_type: Some("text/plain".to_string()),
                    ..ObjectMetadata::default()
                })
            }
            async fn delete(&self, _key: &ObjectKey) -> Result<(), StorageError> {
                Err(StorageError::Config("unused in tests".to_string()))
            }
            async fn exists(&self, _key: &ObjectKey) -> Result<bool, StorageError> {
                Ok(true)
            }
            async fn presign(
                &self,
                _key: &ObjectKey,
                _expires_secs: u64,
            ) -> Result<String, StorageError> {
                Err(StorageError::Presign("unused in tests".to_string()))
            }
        }

        struct FakeDocumentRepository;

        #[async_trait]
        impl DocumentRepository for FakeDocumentRepository {
            async fn load(
                &self,
                _tenant_id: Uuid,
                _document_id: Uuid,
            ) -> Result<Option<DocumentMetadata>, RepositoryError> {
                Ok(None)
            }
            async fn save(
                &self,
                _document: &DocumentMetadata,
                _new_revision: Option<&DocumentRevision>,
                _expected_version: AggregateVersion,
            ) -> Result<(), RepositoryError> {
                Err(RepositoryError::Failed)
            }
        }

        // --------------------------------------------------------------- drive --

        struct AttemptTally {
            succeeded: u64,
            lease_unproven: u64,
            failed: u64,
            lease_lost: u64,
            complete_calls: usize,
            fail_calls: usize,
            last_completion: Option<CompleteAiTaskCommand>,
        }

        // The counter-delta window must not interleave with the sibling tests,
        // so the guard is deliberately held across the awaited task drive.
        // Each #[tokio::test] owns its runtime thread; blocking the sibling's
        // thread for the duration of one fast in-process attempt is the point.
        #[allow(clippy::await_holding_lock)]
        async fn drive_attempt(outcome: CompletionOutcome) -> AttemptTally {
            let _guard = metrics_lock();
            let recorder = install_test_metrics();
            let tenant_id = Uuid::now_v7();
            let document_id = Uuid::now_v7();
            let job_id = Uuid::now_v7();
            let task_id = Uuid::now_v7();
            let now = Utc::now();
            let job = ProcessingJob::queue(
                tenant_id,
                document_id,
                1,
                format!("drill-request-{task_id}"),
                Uuid::now_v7(),
                3,
                now,
            )
            .unwrap_or_else(|error| unreachable!("queue fixture must succeed: {error}"));
            let execution = Arc::new(FakeExecution::new(outcome));
            let services = Arc::new(AiWorkerServices {
                execution: Arc::clone(&execution) as Arc<dyn ProcessingExecutionUnitOfWork>,
                queries: Arc::new(FakeQuery {
                    detail: ProcessingJobDetail {
                        job,
                        candidate: None,
                        review: None,
                    },
                }) as Arc<dyn ProcessingJobQuery>,
            });
            let source = StorageSource {
                documents: Arc::new(FakeDocumentRepository) as Arc<dyn DocumentRepository>,
                storage: Arc::new(FakeStorage {
                    body: Bytes::from_static(b"Contract One\nparty alpha\n"),
                }) as Arc<dyn ObjectStorageClient>,
            };
            let config: AiWorkerConfig = serde_json::from_str("{}")
                .unwrap_or_else(|error| unreachable!("default config must deserialize: {error}"));
            let task = AiTask {
                id: task_id,
                tenant_id,
                job_id,
                step_kind: ProcessingStepKind::ExtractFields,
                status: "claimed".to_string(),
                input_artifact_id: Some("artifacts/drill/text.txt".to_string()),
                attempt_count: 1,
                max_attempts: 3,
                next_attempt_at: now,
                cancel_requested_at: None,
                lease_owner: Some("test-ai-worker".to_string()),
                lease_token: Some("lease-token-1".to_string()),
                fence_version: 7,
                lease_expires_at: Some(now + Duration::seconds(30)),
                output_candidate_id: None,
                correlation_id: Some("corr-drill".to_string()),
                created_at: now,
            };
            let baseline = AttemptTally {
                succeeded: recorder.counter("ai_tasks_total", Some(("outcome", "succeeded"))),
                lease_unproven: recorder
                    .counter("ai_tasks_total", Some(("outcome", "lease_unproven"))),
                failed: recorder.counter("ai_tasks_total", Some(("outcome", "failed"))),
                lease_lost: recorder.counter("ai_lease_lost_total", None),
                complete_calls: 0,
                fail_calls: 0,
                last_completion: None,
            };
            process_task(
                services.as_ref(),
                &source,
                task,
                &config,
                &DeterministicLocalExtractor,
            )
            .await;
            AttemptTally {
                succeeded: recorder.counter("ai_tasks_total", Some(("outcome", "succeeded")))
                    - baseline.succeeded,
                lease_unproven: recorder
                    .counter("ai_tasks_total", Some(("outcome", "lease_unproven")))
                    - baseline.lease_unproven,
                failed: recorder.counter("ai_tasks_total", Some(("outcome", "failed")))
                    - baseline.failed,
                lease_lost: recorder.counter("ai_lease_lost_total", None) - baseline.lease_lost,
                complete_calls: execution.complete_calls.load(Ordering::SeqCst),
                fail_calls: execution.fail_calls.load(Ordering::SeqCst),
                last_completion: execution.recorded_completion(),
            }
        }

        #[tokio::test]
        async fn fenced_completion_records_exactly_one_lease_unproven_outcome() {
            let tally = drive_attempt(CompletionOutcome::Fenced).await;
            assert_eq!(
                tally.succeeded, 0,
                "a fenced completion must not count as succeeded"
            );
            assert_eq!(
                tally.succeeded + tally.lease_unproven + tally.failed,
                1,
                "the attempt must record exactly one final outcome"
            );
            assert_eq!(
                tally.lease_unproven, 1,
                "a fenced completion is lease-unproven"
            );
            assert_eq!(tally.failed, 0);
            assert_eq!(
                tally.lease_lost, 1,
                "a fenced completion must increment the lease-loss counter"
            );
            assert_eq!(tally.complete_calls, 1);
            assert_eq!(tally.fail_calls, 0);
            let completion = tally
                .last_completion
                .unwrap_or_else(|| unreachable!("completion must have been attempted"));
            assert_eq!(completion.fence.fence_version, 7);
            assert_eq!(completion.fence.lease_token, "lease-token-1");
        }

        #[tokio::test]
        async fn completion_persistence_failure_records_failed_without_lease_loss() {
            let tally = drive_attempt(CompletionOutcome::PersistenceFailed).await;
            assert_eq!(
                tally.succeeded, 0,
                "a completion that failed to persist must not count as succeeded"
            );
            assert_eq!(
                tally.succeeded + tally.lease_unproven + tally.failed,
                1,
                "the attempt must record exactly one final outcome"
            );
            assert_eq!(
                tally.failed, 1,
                "a persistence error other than LeaseLost is a failed outcome"
            );
            assert_eq!(
                tally.lease_unproven, 0,
                "a persistence error other than LeaseLost is not lease-unproven"
            );
            assert_eq!(
                tally.lease_lost, 0,
                "a persistence error other than LeaseLost must not inflate the lease-loss counter"
            );
            assert_eq!(tally.complete_calls, 1);
            assert_eq!(tally.fail_calls, 0);
        }

        #[tokio::test]
        async fn durable_completion_records_exactly_one_succeeded_outcome() {
            let tally = drive_attempt(CompletionOutcome::Persisted).await;
            assert_eq!(
                tally.succeeded, 1,
                "a durably persisted completion must count exactly once as succeeded"
            );
            assert_eq!(
                tally.succeeded + tally.lease_unproven + tally.failed,
                1,
                "the attempt must record exactly one final outcome"
            );
            assert_eq!(tally.lease_unproven, 0);
            assert_eq!(tally.failed, 0);
            assert_eq!(tally.lease_lost, 0);
            assert_eq!(tally.complete_calls, 1);
            assert_eq!(tally.fail_calls, 0);
            let completion = tally
                .last_completion
                .unwrap_or_else(|| unreachable!("completion must have been attempted"));
            assert_eq!(completion.fence.fence_version, 7);
            assert!(completion.candidate.payload.title.is_some());
        }
    }
}
