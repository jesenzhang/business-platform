//! Durable integrity rules and finding lifecycle contracts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegritySeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityRuleDescriptor {
    pub id: String,
    pub version: u32,
    pub bounded_context: String,
    pub severity: IntegritySeverity,
    pub automatic_repair_allowed: bool,
}

impl IntegrityRuleDescriptor {
    pub fn validate(&self) -> Result<(), IntegrityError> {
        if self.id.trim().is_empty() || self.id.len() > 128 || self.version == 0 {
            return Err(IntegrityError::InvalidDescriptor);
        }
        if self.bounded_context.trim().is_empty() {
            return Err(IntegrityError::InvalidDescriptor);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityScanScope {
    pub tenant_id: Option<Uuid>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedIntegrityIssue {
    pub tenant_id: Uuid,
    pub resource_type: String,
    pub resource_id: String,
    pub fingerprint: String,
    pub detected_state: Value,
    pub expected_state: Value,
    pub repairability: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Open,
    RepairPlanned,
    Repairing,
    Repaired,
    Ignored,
    FalsePositive,
    Stale,
    NeedsManualReview,
}

#[must_use]
pub fn finding_status_name(status: FindingStatus) -> &'static str {
    match status {
        FindingStatus::Open => "open",
        FindingStatus::RepairPlanned => "repair_planned",
        FindingStatus::Repairing => "repairing",
        FindingStatus::Repaired => "repaired",
        FindingStatus::Ignored => "ignored",
        FindingStatus::FalsePositive => "false_positive",
        FindingStatus::Stale => "stale",
        FindingStatus::NeedsManualReview => "needs_manual_review",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityFinding {
    id: Uuid,
    tenant_id: Uuid,
    rule_id: String,
    rule_version: u32,
    bounded_context: String,
    resource_type: String,
    resource_id: String,
    severity: IntegritySeverity,
    fingerprint: String,
    detected_state: Value,
    expected_state: Value,
    status: FindingStatus,
    repairability: String,
    first_detected_at: DateTime<Utc>,
    last_detected_at: DateTime<Utc>,
    occurrence_count: u64,
    resolved_at: Option<DateTime<Utc>>,
    resolution_reason: Option<String>,
    /// Timestamp of the latest explicit reopen after a resolved episode.
    reopened_at: Option<DateTime<Utc>>,
    /// Number of times a resolved finding has been observed again.
    reopen_count: u64,
    /// Resolution that was superseded by the latest reopen.
    previous_resolution: Option<String>,
    version: i64,
}

impl IntegrityFinding {
    pub fn from_issue(
        descriptor: &IntegrityRuleDescriptor,
        issue: DetectedIntegrityIssue,
        now: DateTime<Utc>,
    ) -> Result<Self, IntegrityError> {
        descriptor.validate()?;
        if issue.tenant_id.is_nil()
            || issue.resource_type.trim().is_empty()
            || issue.resource_id.trim().is_empty()
            || issue.fingerprint.trim().is_empty()
        {
            return Err(IntegrityError::InvalidFinding);
        }
        Self::rehydrate(
            Uuid::new_v4(),
            issue.tenant_id,
            descriptor.id.clone(),
            descriptor.version,
            descriptor.bounded_context.clone(),
            issue.resource_type,
            issue.resource_id,
            descriptor.severity,
            issue.fingerprint,
            issue.detected_state,
            issue.expected_state,
            FindingStatus::Open,
            issue.repairability,
            now,
            now,
            1,
            None,
            None,
            None,
            0,
            None,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: Uuid,
        tenant_id: Uuid,
        rule_id: String,
        rule_version: u32,
        bounded_context: String,
        resource_type: String,
        resource_id: String,
        severity: IntegritySeverity,
        fingerprint: String,
        detected_state: Value,
        expected_state: Value,
        status: FindingStatus,
        repairability: String,
        first_detected_at: DateTime<Utc>,
        last_detected_at: DateTime<Utc>,
        occurrence_count: u64,
        resolved_at: Option<DateTime<Utc>>,
        resolution_reason: Option<String>,
        reopened_at: Option<DateTime<Utc>>,
        reopen_count: u64,
        previous_resolution: Option<String>,
        version: i64,
    ) -> Result<Self, IntegrityError> {
        if id.is_nil()
            || tenant_id.is_nil()
            || rule_id.trim().is_empty()
            || rule_version == 0
            || bounded_context.trim().is_empty()
            || resource_type.trim().is_empty()
            || resource_id.trim().is_empty()
            || fingerprint.trim().is_empty()
            || repairability.trim().is_empty()
            || occurrence_count == 0
            || version < 0
            || last_detected_at < first_detected_at
        {
            return Err(IntegrityError::InvalidFinding);
        }
        Ok(Self {
            id,
            tenant_id,
            rule_id,
            rule_version,
            bounded_context,
            resource_type,
            resource_id,
            severity,
            fingerprint,
            detected_state,
            expected_state,
            status,
            repairability,
            first_detected_at,
            last_detected_at,
            occurrence_count,
            resolved_at,
            resolution_reason,
            reopened_at,
            reopen_count,
            previous_resolution,
            version,
        })
    }

    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    #[must_use]
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }
    #[must_use]
    pub fn rule_version(&self) -> u32 {
        self.rule_version
    }
    #[must_use]
    pub fn bounded_context(&self) -> &str {
        &self.bounded_context
    }
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }
    #[must_use]
    pub fn severity(&self) -> IntegritySeverity {
        self.severity
    }
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    #[must_use]
    pub fn detected_state(&self) -> &Value {
        &self.detected_state
    }
    #[must_use]
    pub fn expected_state(&self) -> &Value {
        &self.expected_state
    }
    #[must_use]
    pub fn status(&self) -> FindingStatus {
        self.status
    }
    #[must_use]
    pub fn repairability(&self) -> &str {
        &self.repairability
    }
    #[must_use]
    pub fn first_detected_at(&self) -> DateTime<Utc> {
        self.first_detected_at
    }
    #[must_use]
    pub fn last_detected_at(&self) -> DateTime<Utc> {
        self.last_detected_at
    }
    #[must_use]
    pub fn occurrence_count(&self) -> u64 {
        self.occurrence_count
    }
    #[must_use]
    pub fn resolved_at(&self) -> Option<DateTime<Utc>> {
        self.resolved_at
    }
    #[must_use]
    pub fn resolution_reason(&self) -> Option<&str> {
        self.resolution_reason.as_deref()
    }
    #[must_use]
    pub fn reopened_at(&self) -> Option<DateTime<Utc>> {
        self.reopened_at
    }
    #[must_use]
    pub fn reopen_count(&self) -> u64 {
        self.reopen_count
    }
    #[must_use]
    pub fn previous_resolution(&self) -> Option<&str> {
        self.previous_resolution.as_deref()
    }
    #[must_use]
    pub fn version(&self) -> i64 {
        self.version
    }

    pub fn mark_detected(&mut self, now: DateTime<Utc>) -> Result<(), IntegrityError> {
        if matches!(
            self.status,
            FindingStatus::Repaired | FindingStatus::FalsePositive
        ) {
            // Recurrence is an explicit policy: the same rule version starts a
            // new open episode and preserves the superseded resolution.  This
            // prevents a scan from silently keeping a resolved finding closed.
            self.previous_resolution = self
                .resolution_reason
                .clone()
                .or_else(|| Some(format!("{:?}", self.status).to_lowercase()));
            self.reopened_at = Some(now);
            self.reopen_count = self
                .reopen_count
                .checked_add(1)
                .ok_or(IntegrityError::Persistence)?;
            self.status = FindingStatus::Open;
            self.resolved_at = None;
            self.resolution_reason = None;
        }
        self.last_detected_at = now;
        self.occurrence_count = self
            .occurrence_count
            .checked_add(1)
            .ok_or(IntegrityError::Persistence)?;
        self.version = self
            .version
            .checked_add(1)
            .ok_or(IntegrityError::Persistence)?;
        Ok(())
    }

    pub fn transition(
        &mut self,
        status: FindingStatus,
        reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), IntegrityError> {
        if self.status == status {
            return Ok(());
        }
        #[allow(clippy::unnested_or_patterns)]
        let valid = matches!(
            (self.status, status),
            (FindingStatus::Open, FindingStatus::RepairPlanned)
                | (FindingStatus::Open, FindingStatus::Ignored)
                | (FindingStatus::Open, FindingStatus::FalsePositive)
                | (FindingStatus::Open, FindingStatus::Stale)
                | (FindingStatus::RepairPlanned, FindingStatus::Repairing)
                | (FindingStatus::Repairing, FindingStatus::Repaired)
                | (FindingStatus::Repairing, FindingStatus::NeedsManualReview)
                | (FindingStatus::Repairing, FindingStatus::Open)
                | (FindingStatus::RepairPlanned, FindingStatus::Open)
        );
        if !valid {
            return Err(IntegrityError::InvalidTransition);
        }
        self.status = status;
        self.resolution_reason = reason;
        self.resolved_at =
            matches!(status, FindingStatus::Repaired | FindingStatus::Stale).then_some(now);
        self.version = self
            .version
            .checked_add(1)
            .ok_or(IntegrityError::Persistence)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityScanRun {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub scope: IntegrityScanScope,
    pub status: ScanRunStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub rule_count: u32,
    pub finding_count: u64,
    pub failure_code: Option<String>,
    pub created_by: Uuid,
}

#[derive(Debug, Error)]
pub enum IntegrityError {
    #[error("integrity rule descriptor is invalid")]
    InvalidDescriptor,
    #[error("integrity finding is invalid")]
    InvalidFinding,
    #[error("integrity finding transition is invalid")]
    InvalidTransition,
    #[error("integrity dependency is temporarily unavailable")]
    DependencyUnavailable,
    #[error("integrity persistence failed")]
    Persistence,
}

#[async_trait]
pub trait IntegrityRule: Send + Sync {
    fn descriptor(&self) -> IntegrityRuleDescriptor;
    async fn scan(
        &self,
        scope: &IntegrityScanScope,
    ) -> Result<Vec<DetectedIntegrityIssue>, IntegrityError>;
    async fn verify(&self, finding: &IntegrityFinding) -> Result<bool, IntegrityError>;
}

#[derive(Default)]
pub struct IntegrityRuleRegistry {
    rules: Vec<Box<dyn IntegrityRule>>,
}

impl IntegrityRuleRegistry {
    pub fn register(&mut self, rule: Box<dyn IntegrityRule>) -> Result<(), IntegrityError> {
        let descriptor = rule.descriptor();
        descriptor.validate()?;
        if self.rules.iter().any(|existing| {
            let current = existing.descriptor();
            current.id == descriptor.id && current.version == descriptor.version
        }) {
            return Err(IntegrityError::InvalidDescriptor);
        }
        self.rules.push(rule);
        Ok(())
    }

    #[must_use]
    pub fn rules(&self) -> &[Box<dyn IntegrityRule>] {
        &self.rules
    }

    /// Resolve the immutable rule identity stored on a Finding and verify the
    /// current owner state. A missing rule/version is a fail-closed error; a
    /// Repair Handler's self-reported success is never sufficient to close a
    /// Finding.
    pub async fn verify_finding(&self, finding: &IntegrityFinding) -> Result<bool, IntegrityError> {
        let Some(rule) = self.rules.iter().find(|rule| {
            let descriptor = rule.descriptor();
            descriptor.id == finding.rule_id() && descriptor.version == finding.rule_version()
        }) else {
            return Err(IntegrityError::InvalidDescriptor);
        };
        rule.verify(finding).await
    }
}

#[async_trait]
pub trait IntegrityPersistencePort: Send + Sync {
    async fn record_scan_run(&self, run: &IntegrityScanRun) -> Result<(), IntegrityError>;
    async fn upsert_finding(&self, finding: &IntegrityFinding) -> Result<(), IntegrityError>;
    async fn load_finding(&self, id: Uuid) -> Result<Option<IntegrityFinding>, IntegrityError>;
}

/// Read-only management queries owned by the Governance context.  The
/// application layer exposes these as safe DTOs; adapters never return raw
/// rows or provider/storage fields.
#[async_trait]
pub trait IntegrityQueryPort: Send + Sync {
    async fn get_scan_run(
        &self,
        tenant_id: Option<Uuid>,
        id: Uuid,
    ) -> Result<Option<IntegrityScanRun>, IntegrityError>;

    async fn list_scan_runs(
        &self,
        tenant_id: Option<Uuid>,
        limit: u16,
    ) -> Result<Vec<IntegrityScanRun>, IntegrityError>;

    async fn list_findings(
        &self,
        tenant_id: Uuid,
        status: Option<FindingStatus>,
        limit: u16,
    ) -> Result<Vec<IntegrityFinding>, IntegrityError>;
}

/// Read-only, owner-defined processing facts used by PROC-INT rules.
///
/// The snapshot deliberately contains safe state and hashes only; it does not
/// expose storage keys, raw text, lease tokens, or provider responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProcessingIntegritySnapshot {
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub job_status: String,
    /// Owner attempt counter used by the state matrix to detect stale step
    /// projections and impossible retry combinations.
    #[serde(default)]
    pub job_attempt_count: i64,
    pub current_step: String,
    pub content_revision: i64,
    pub candidate_content_revision: Option<i64>,
    pub has_candidate: bool,
    pub has_review: bool,
    pub review_decision: Option<String>,
    pub has_active_ai_task: bool,
    pub has_succeeded_ai_without_candidate: bool,
    pub terminal_has_lease: bool,
    pub steps: Vec<ProcessingStepIntegritySnapshot>,
    pub text_artifact_state: TextArtifactIntegrityState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessingStepIntegritySnapshot {
    pub step_kind: String,
    pub status: String,
    /// Attempt number is an owner fact, not a governance counter.
    #[serde(default)]
    pub attempt_number: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextArtifactIntegrityState {
    Present,
    Missing,
    Unknown,
}

#[async_trait]
pub trait ProcessingIntegrityQuery: Send + Sync {
    async fn snapshots(
        &self,
        scope: &IntegrityScanScope,
    ) -> Result<Vec<ProcessingIntegritySnapshot>, IntegrityError>;

    async fn snapshot(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<ProcessingIntegritySnapshot>, IntegrityError>;
}

#[derive(Debug, Clone, Copy)]
struct ProcessingRule {
    id: &'static str,
    severity: IntegritySeverity,
    automatic_repair_allowed: bool,
}

impl ProcessingRule {
    fn descriptor(self) -> IntegrityRuleDescriptor {
        IntegrityRuleDescriptor {
            id: self.id.to_string(),
            version: 1,
            bounded_context: "document-processing".to_string(),
            severity: self.severity,
            automatic_repair_allowed: self.automatic_repair_allowed,
        }
    }
}

/// Explicit state-matrix checks for the processing owner.  Keeping the
/// allowed combinations in one table-like function makes PROC-INT-006
/// auditable and prevents it degrading into another single boolean check.
#[must_use]
pub fn processing_state_matrix_violations(
    snapshot: &ProcessingIntegritySnapshot,
) -> Vec<&'static str> {
    const PIPELINE: [&str; 6] = [
        "validate_source",
        "detect_type",
        "extract_text",
        "extract_fields",
        "validate_candidate",
        "await_review",
    ];
    let mut violations = Vec::new();
    let running_steps = snapshot
        .steps
        .iter()
        .filter(|step| step.status == "running")
        .count();
    if running_steps > 1 {
        violations.push("multiple_running_steps");
    }

    let current = snapshot
        .steps
        .iter()
        .find(|step| step.step_kind == snapshot.current_step);
    let current_projection_valid = match snapshot.job_status.as_str() {
        "queued" => current.is_some_and(|step| step.status == "pending"),
        "running" => current.is_some_and(|step| step.status == "running"),
        "waiting_for_ai" => {
            snapshot.current_step == "extract_fields"
                && current.is_some_and(|step| matches!(step.status.as_str(), "queued" | "running"))
        }
        "waiting_for_review" => {
            snapshot.current_step == "await_review"
                && current.is_some_and(|step| matches!(step.status.as_str(), "pending" | "queued"))
        }
        "succeeded" | "rejected" => current.is_some_and(|step| step.status == "succeeded"),
        _ => true,
    };
    if !current_projection_valid {
        violations.push("current_step_projection_mismatch");
    }

    if snapshot.job_status == "waiting_for_ai" {
        let extract_fields = snapshot
            .steps
            .iter()
            .find(|step| step.step_kind == "extract_fields");
        if extract_fields.is_none_or(|step| !matches!(step.status.as_str(), "queued" | "running")) {
            violations.push("waiting_for_ai_extract_fields_invalid");
        }
    }

    if snapshot.job_status == "waiting_for_review" {
        let projection_valid = snapshot
            .steps
            .iter()
            .any(|step| step.step_kind == "validate_candidate" && step.status == "succeeded")
            && snapshot.steps.iter().any(|step| {
                step.step_kind == "await_review"
                    && matches!(step.status.as_str(), "pending" | "queued")
            });
        if !projection_valid {
            violations.push("waiting_for_review_projection_invalid");
        }
    }

    if matches!(
        snapshot.job_status.as_str(),
        "succeeded" | "failed" | "cancelled" | "rejected"
    ) && snapshot.steps.iter().any(|step| step.status == "running")
    {
        violations.push("terminal_job_has_running_step");
    }

    // The fixed MVP pipeline is deliberately represented as an ordered list;
    // a later succeeded step cannot precede an unfinished earlier step.
    for (index, kind) in PIPELINE.iter().enumerate() {
        let later_succeeded = snapshot.steps.iter().any(|step| {
            PIPELINE
                .iter()
                .position(|candidate| candidate == &step.step_kind)
                .is_some_and(|position| position > index)
                && step.status == "succeeded"
        });
        if later_succeeded
            && snapshot
                .steps
                .iter()
                .find(|step| step.step_kind == *kind)
                .is_some_and(|step| step.status != "succeeded")
        {
            violations.push("later_step_succeeded_before_predecessor");
            break;
        }
    }

    if snapshot.steps.iter().any(|step| {
        step.attempt_number < 0
            || step.attempt_number > snapshot.job_attempt_count.saturating_add(1)
    }) {
        violations.push("step_attempt_job_attempt_mismatch");
    }
    violations
}

macro_rules! processing_rule {
    ($name:ident, $id:literal, $severity:expr, $auto:expr, $predicate:expr, $repair:literal) => {
        pub struct $name<Q> {
            query: std::sync::Arc<Q>,
        }

        impl<Q> $name<Q> {
            #[must_use]
            pub fn new(query: std::sync::Arc<Q>) -> Self {
                Self { query }
            }
        }

        #[async_trait]
        impl<Q: ProcessingIntegrityQuery + 'static> IntegrityRule for $name<Q> {
            fn descriptor(&self) -> IntegrityRuleDescriptor {
                ProcessingRule {
                    id: $id,
                    severity: $severity,
                    automatic_repair_allowed: $auto,
                }
                .descriptor()
            }

            async fn scan(
                &self,
                scope: &IntegrityScanScope,
            ) -> Result<Vec<DetectedIntegrityIssue>, IntegrityError> {
                let mut issues = Vec::new();
                for snapshot in self.query.snapshots(scope).await? {
                    if ($predicate)(&snapshot) {
                        issues.push(DetectedIntegrityIssue {
                            tenant_id: snapshot.tenant_id,
                            resource_type: "processing_job".to_string(),
                            resource_id: snapshot.job_id.to_string(),
                            fingerprint: format!("{}:{}", $id, snapshot.job_id),
                            detected_state: serde_json::json!({
                                "status": &snapshot.job_status,
                                "current_step": &snapshot.current_step,
                            }),
                            expected_state: serde_json::json!({"rule": $id}),
                            repairability: $repair.to_string(),
                        });
                    }
                }
                Ok(issues)
            }

            async fn verify(
                &self,
                finding: &IntegrityFinding,
            ) -> Result<bool, IntegrityError> {
                let job_id = Uuid::parse_str(finding.resource_id())
                    .map_err(|_| IntegrityError::InvalidFinding)?;
            let Some(snapshot) = self.query.snapshot(finding.tenant_id(), job_id).await? else {
                    return Ok(false);
                };
                Ok(!($predicate)(&snapshot))
            }
        }
    };
}

processing_rule!(
    ProcessingMissingAiTaskRule,
    "PROC-INT-001",
    IntegritySeverity::Error,
    true,
    |snapshot: &ProcessingIntegritySnapshot| {
        snapshot.job_status == "waiting_for_ai" && !snapshot.has_active_ai_task
    },
    "requeue_missing_ai_task.v1"
);
processing_rule!(
    ProcessingMissingCandidateRule,
    "PROC-INT-002",
    IntegritySeverity::Error,
    false,
    |snapshot: &ProcessingIntegritySnapshot| {
        snapshot.job_status == "waiting_for_review" && !snapshot.has_candidate
    },
    "needs_manual_review"
);
processing_rule!(
    ProcessingReviewStateRule,
    "PROC-INT-003",
    IntegritySeverity::Error,
    false,
    |snapshot: &ProcessingIntegritySnapshot| {
        snapshot.has_review
            && !matches!(
                snapshot.job_status.as_str(),
                "succeeded" | "rejected" | "cancelled" | "failed"
            )
    },
    "reconcile_processing_job.v1"
);
processing_rule!(
    ProcessingSucceededAiWithoutCandidateRule,
    "PROC-INT-004",
    IntegritySeverity::Error,
    false,
    |snapshot: &ProcessingIntegritySnapshot| snapshot.has_succeeded_ai_without_candidate,
    "needs_manual_review"
);
processing_rule!(
    ProcessingTerminalLeaseRule,
    "PROC-INT-005",
    IntegritySeverity::Warning,
    true,
    |snapshot: &ProcessingIntegritySnapshot| {
        snapshot.terminal_has_lease
            && matches!(
                snapshot.job_status.as_str(),
                "succeeded" | "failed" | "cancelled" | "rejected"
            )
    },
    "clear_terminal_job_lease.v1"
);
processing_rule!(
    ProcessingStepProjectionRule,
    "PROC-INT-006",
    IntegritySeverity::Error,
    false,
    |snapshot: &ProcessingIntegritySnapshot| {
        !processing_state_matrix_violations(snapshot).is_empty()
    },
    "rebuild_processing_step_projection.v1"
);
processing_rule!(
    ProcessingCandidateRevisionRule,
    "PROC-INT-007",
    IntegritySeverity::Error,
    false,
    |snapshot: &ProcessingIntegritySnapshot| {
        snapshot
            .candidate_content_revision
            .is_some_and(|revision| revision != snapshot.content_revision)
    },
    "needs_manual_review"
);
processing_rule!(
    ProcessingTextArtifactRule,
    "PROC-INT-008",
    IntegritySeverity::Critical,
    false,
    |snapshot: &ProcessingIntegritySnapshot| {
        snapshot.text_artifact_state == TextArtifactIntegrityState::Missing
    },
    "needs_manual_review"
);

#[must_use]
pub fn processing_rules<Q: ProcessingIntegrityQuery + 'static>(
    query: std::sync::Arc<Q>,
) -> Vec<Box<dyn IntegrityRule>> {
    vec![
        Box::new(ProcessingMissingAiTaskRule::new(std::sync::Arc::clone(
            &query,
        ))),
        Box::new(ProcessingMissingCandidateRule::new(std::sync::Arc::clone(
            &query,
        ))),
        Box::new(ProcessingReviewStateRule::new(std::sync::Arc::clone(
            &query,
        ))),
        Box::new(ProcessingSucceededAiWithoutCandidateRule::new(
            std::sync::Arc::clone(&query),
        )),
        Box::new(ProcessingTerminalLeaseRule::new(std::sync::Arc::clone(
            &query,
        ))),
        Box::new(ProcessingStepProjectionRule::new(std::sync::Arc::clone(
            &query,
        ))),
        Box::new(ProcessingCandidateRevisionRule::new(std::sync::Arc::clone(
            &query,
        ))),
        Box::new(ProcessingTextArtifactRule::new(query)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> IntegrityRuleDescriptor {
        IntegrityRuleDescriptor {
            id: "PROC-INT-001".to_string(),
            version: 1,
            bounded_context: "document-processing".to_string(),
            severity: IntegritySeverity::Warning,
            automatic_repair_allowed: true,
        }
    }

    #[test]
    fn finding_deduplication_increments_occurrence() {
        let issue = DetectedIntegrityIssue {
            tenant_id: Uuid::new_v4(),
            resource_type: "processing_job".to_string(),
            resource_id: "job-1".to_string(),
            fingerprint: "missing-task".to_string(),
            detected_state: serde_json::json!({"state": "waiting_for_ai"}),
            expected_state: serde_json::json!({"task": "active"}),
            repairability: "requeue_missing_ai_task.v1".to_string(),
        };
        let mut finding = IntegrityFinding::from_issue(&descriptor(), issue, Utc::now())
            .unwrap_or_else(|_| unreachable!());
        finding
            .mark_detected(Utc::now())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(finding.occurrence_count, 2);
    }

    #[test]
    fn invalid_lifecycle_is_rejected() {
        let issue = DetectedIntegrityIssue {
            tenant_id: Uuid::new_v4(),
            resource_type: "job".to_string(),
            resource_id: "job-1".to_string(),
            fingerprint: "x".to_string(),
            detected_state: Value::Null,
            expected_state: Value::Null,
            repairability: "none".to_string(),
        };
        let mut finding = IntegrityFinding::from_issue(&descriptor(), issue, Utc::now())
            .unwrap_or_else(|_| unreachable!());
        assert!(finding
            .transition(FindingStatus::Repaired, None, Utc::now())
            .is_err());
    }

    #[test]
    fn resolved_finding_reopens_as_a_new_episode_on_recurrence() {
        let issue = DetectedIntegrityIssue {
            tenant_id: Uuid::new_v4(),
            resource_type: "job".to_string(),
            resource_id: "job-1".to_string(),
            fingerprint: "x".to_string(),
            detected_state: Value::Null,
            expected_state: Value::Null,
            repairability: "none".to_string(),
        };
        let mut finding = IntegrityFinding::from_issue(&descriptor(), issue, Utc::now())
            .unwrap_or_else(|_| unreachable!());
        finding.status = FindingStatus::Repaired;
        finding.resolution_reason = Some("repair_succeeded".to_string());
        finding.resolved_at = Some(Utc::now());
        finding
            .mark_detected(Utc::now())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(finding.status, FindingStatus::Open);
        assert_eq!(finding.reopen_count, 1);
        assert_eq!(
            finding.previous_resolution.as_deref(),
            Some("repair_succeeded")
        );
        assert!(finding.reopened_at.is_some());
    }

    #[test]
    fn processing_state_matrix_reports_multiple_inconsistencies() {
        let snapshot = ProcessingIntegritySnapshot {
            tenant_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            job_status: "succeeded".to_string(),
            job_attempt_count: 0,
            current_step: "await_review".to_string(),
            content_revision: 1,
            candidate_content_revision: None,
            has_candidate: false,
            has_review: false,
            review_decision: None,
            has_active_ai_task: false,
            has_succeeded_ai_without_candidate: false,
            terminal_has_lease: true,
            steps: vec![
                ProcessingStepIntegritySnapshot {
                    step_kind: "extract_text".to_string(),
                    status: "running".to_string(),
                    attempt_number: 2,
                },
                ProcessingStepIntegritySnapshot {
                    step_kind: "await_review".to_string(),
                    status: "succeeded".to_string(),
                    attempt_number: 0,
                },
            ],
            text_artifact_state: TextArtifactIntegrityState::Unknown,
        };
        let violations = processing_state_matrix_violations(&snapshot);
        assert!(violations.contains(&"terminal_job_has_running_step"));
        assert!(violations.contains(&"step_attempt_job_attempt_mismatch"));
    }

    #[test]
    fn waiting_for_review_accepts_owner_pending_projection() {
        let snapshot = ProcessingIntegritySnapshot {
            tenant_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            job_status: "waiting_for_review".to_string(),
            job_attempt_count: 0,
            current_step: "await_review".to_string(),
            content_revision: 1,
            candidate_content_revision: Some(1),
            has_candidate: true,
            has_review: false,
            review_decision: None,
            has_active_ai_task: false,
            has_succeeded_ai_without_candidate: false,
            terminal_has_lease: false,
            steps: vec![
                ProcessingStepIntegritySnapshot {
                    step_kind: "validate_candidate".to_string(),
                    status: "succeeded".to_string(),
                    attempt_number: 0,
                },
                ProcessingStepIntegritySnapshot {
                    step_kind: "await_review".to_string(),
                    status: "pending".to_string(),
                    attempt_number: 0,
                },
            ],
            text_artifact_state: TextArtifactIntegrityState::Present,
        };
        assert!(processing_state_matrix_violations(&snapshot).is_empty());
    }
}
