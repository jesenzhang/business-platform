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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityFinding {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub rule_id: String,
    pub rule_version: u32,
    pub bounded_context: String,
    pub resource_type: String,
    pub resource_id: String,
    pub severity: IntegritySeverity,
    pub fingerprint: String,
    pub detected_state: Value,
    pub expected_state: Value,
    pub status: FindingStatus,
    pub repairability: String,
    pub first_detected_at: DateTime<Utc>,
    pub last_detected_at: DateTime<Utc>,
    pub occurrence_count: u64,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution_reason: Option<String>,
    pub version: i64,
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
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id: issue.tenant_id,
            rule_id: descriptor.id.clone(),
            rule_version: descriptor.version,
            bounded_context: descriptor.bounded_context.clone(),
            resource_type: issue.resource_type,
            resource_id: issue.resource_id,
            severity: descriptor.severity,
            fingerprint: issue.fingerprint,
            detected_state: issue.detected_state,
            expected_state: issue.expected_state,
            status: FindingStatus::Open,
            repairability: issue.repairability,
            first_detected_at: now,
            last_detected_at: now,
            occurrence_count: 1,
            resolved_at: None,
            resolution_reason: None,
            version: 0,
        })
    }

    pub fn mark_detected(&mut self, now: DateTime<Utc>) -> Result<(), IntegrityError> {
        if matches!(
            self.status,
            FindingStatus::Repaired | FindingStatus::FalsePositive
        ) {
            return Err(IntegrityError::InvalidTransition);
        }
        self.last_detected_at = now;
        self.occurrence_count = self.occurrence_count.saturating_add(1);
        self.version = self.version.saturating_add(1);
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
        self.version = self.version.saturating_add(1);
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
                let job_id = Uuid::parse_str(&finding.resource_id)
                    .map_err(|_| IntegrityError::InvalidFinding)?;
                let Some(snapshot) = self.query.snapshot(finding.tenant_id, job_id).await? else {
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
        snapshot.steps.iter().any(|step| {
            step.step_kind == snapshot.current_step
                && snapshot.job_status == "waiting_for_review"
                && step.status != "succeeded"
        })
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
}
