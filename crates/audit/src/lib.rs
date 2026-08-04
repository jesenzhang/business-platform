//! Unified, validated audit evidence for business and runtime governance.
//!
//! The crate intentionally contains no database, HTTP, storage, or provider
//! dependency. Adapters persist the validated event inside the owner context's
//! transaction.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

const MAX_ACTION_LEN: usize = 128;
const MAX_RESOURCE_TYPE_LEN: usize = 128;
const MAX_RESOURCE_ID_LEN: usize = 256;
const MAX_SCHEMA_VERSION_LEN: usize = 32;
const MAX_REASON_LEN: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorType {
    User,
    Service,
    Worker,
    RepairJob,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditActor {
    pub actor_type: AuditActorType,
    pub actor_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditAction(String);

impl AuditAction {
    pub fn new(value: impl Into<String>) -> Result<Self, AuditValidationError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > MAX_ACTION_LEN {
            return Err(AuditValidationError::InvalidAction);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditResource {
    pub resource_type: String,
    pub resource_id: String,
}

impl AuditResource {
    pub fn new(
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Result<Self, AuditValidationError> {
        let resource_type = resource_type.into();
        let resource_id = resource_id.into();
        if resource_type.trim().is_empty() || resource_type.len() > MAX_RESOURCE_TYPE_LEN {
            return Err(AuditValidationError::InvalidResourceType);
        }
        if resource_id.trim().is_empty() || resource_id.len() > MAX_RESOURCE_ID_LEN {
            return Err(AuditValidationError::InvalidResourceId);
        }
        Ok(Self {
            resource_type,
            resource_id,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Succeeded,
    Failed,
    Denied,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuditValidationError {
    #[error("tenant id must not be nil")]
    NilTenant,
    #[error("actor id must not be nil")]
    NilActor,
    #[error("action is empty or too long")]
    InvalidAction,
    #[error("resource type is empty or too long")]
    InvalidResourceType,
    #[error("resource id is empty or too long")]
    InvalidResourceId,
    #[error("operation id must not be nil")]
    NilOperation,
    #[error("schema version is empty or too long")]
    InvalidSchemaVersion,
    #[error("reason is too long")]
    InvalidReason,
    #[error("failure code is only valid for failed or denied results")]
    InvalidFailureCode,
    #[error("details contain a forbidden sensitive field")]
    SensitiveDetails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub actor: AuditActor,
    pub action: AuditAction,
    pub resource: AuditResource,
    pub operation_id: Uuid,
    pub correlation_id: Option<Uuid>,
    pub causation_id: Option<Uuid>,
    pub trace_id: Option<String>,
    pub reason: Option<String>,
    pub result: AuditResult,
    pub failure_code: Option<String>,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub changed_fields: Vec<String>,
    pub details: Value,
    pub schema_version: String,
    pub occurred_at: DateTime<Utc>,
    pub previous_hash: Option<String>,
    pub record_hash: Option<String>,
}

impl AuditEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        tenant_id: Uuid,
        actor: AuditActor,
        action: AuditAction,
        resource: AuditResource,
        operation_id: Uuid,
        correlation_id: Option<Uuid>,
        causation_id: Option<Uuid>,
        trace_id: Option<String>,
        reason: Option<String>,
        result: AuditResult,
        failure_code: Option<String>,
        before_hash: Option<String>,
        after_hash: Option<String>,
        changed_fields: Vec<String>,
        details: Value,
        schema_version: impl Into<String>,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, AuditValidationError> {
        if tenant_id.is_nil() {
            return Err(AuditValidationError::NilTenant);
        }
        if actor.actor_id.is_nil() {
            return Err(AuditValidationError::NilActor);
        }
        if operation_id.is_nil() {
            return Err(AuditValidationError::NilOperation);
        }
        let schema_version = schema_version.into();
        if schema_version.trim().is_empty() || schema_version.len() > MAX_SCHEMA_VERSION_LEN {
            return Err(AuditValidationError::InvalidSchemaVersion);
        }
        if reason
            .as_ref()
            .is_some_and(|value| value.len() > MAX_REASON_LEN)
        {
            return Err(AuditValidationError::InvalidReason);
        }
        if failure_code.is_some() && !matches!(result, AuditResult::Failed | AuditResult::Denied) {
            return Err(AuditValidationError::InvalidFailureCode);
        }
        let details = redact_details(details)?;
        let mut changed_fields = changed_fields;
        changed_fields.sort_unstable();
        changed_fields.dedup();
        Ok(Self {
            id,
            tenant_id,
            actor,
            action,
            resource,
            operation_id,
            correlation_id,
            causation_id,
            trace_id,
            reason,
            result,
            failure_code,
            before_hash,
            after_hash,
            changed_fields,
            details,
            schema_version,
            occurred_at,
            previous_hash: None,
            record_hash: None,
        })
    }

    #[must_use]
    pub fn with_chain(mut self, previous_hash: Option<String>) -> Self {
        self.previous_hash = previous_hash;
        self.record_hash = Some(hash_record(&self));
        self
    }

    #[must_use]
    pub fn canonical_payload(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "tenant_id": self.tenant_id,
            "actor_type": self.actor.actor_type,
            "actor_id": self.actor.actor_id,
            "action": self.action.as_str(),
            "resource_type": self.resource.resource_type,
            "resource_id": self.resource.resource_id,
            "operation_id": self.operation_id,
            "correlation_id": self.correlation_id,
            "causation_id": self.causation_id,
            "trace_id": self.trace_id,
            "reason": self.reason,
            "result": self.result,
            "failure_code": self.failure_code,
            "before_hash": self.before_hash,
            "after_hash": self.after_hash,
            "changed_fields": self.changed_fields,
            "details": self.details,
            "schema_version": self.schema_version,
            "occurred_at": self.occurred_at,
            "previous_hash": self.previous_hash,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuditError {
    #[error("audit validation failed: {0}")]
    Validation(#[from] AuditValidationError),
    #[error("audit persistence failed")]
    Persistence,
    #[error("audit chain verification failed")]
    ChainBroken,
    #[error("audit query cursor is invalid")]
    InvalidCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditCursor {
    pub occurred_at: DateTime<Utc>,
    pub id: Uuid,
}

#[derive(Debug, Clone, Default)]
pub struct AuditQueryRequest {
    pub tenant_id: Uuid,
    pub actor: Option<AuditActorType>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub operation_id: Option<Uuid>,
    pub trace_id: Option<String>,
    pub result: Option<AuditResult>,
    pub occurred_after: Option<DateTime<Utc>>,
    pub occurred_before: Option<DateTime<Utc>>,
    pub cursor: Option<AuditCursor>,
    pub limit: u16,
}

#[derive(Debug, Clone)]
pub struct AuditPage {
    pub items: Vec<AuditEvent>,
    pub next_cursor: Option<AuditCursor>,
}

#[derive(Debug, Clone)]
pub struct AuditChainScope {
    pub tenant_id: Uuid,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditChainVerification {
    pub checked: u64,
    pub valid: bool,
    pub first_broken_id: Option<Uuid>,
}

#[async_trait]
pub trait AuditAppendPort: Send + Sync {
    async fn append(&self, event: &AuditEvent) -> Result<(), AuditError>;

    /// Adapter-owned transaction seam.  A business unit-of-work should call
    /// its database-specific mapper while the aggregate transaction is open;
    /// standalone callers may use this convenience method.
    async fn append_in_transaction(&self, event: &AuditEvent) -> Result<(), AuditError> {
        self.append(event).await
    }
}

#[async_trait]
pub trait AuditQuery: Send + Sync {
    async fn list(&self, query: AuditQueryRequest) -> Result<AuditPage, AuditError>;
    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<AuditEvent>, AuditError> {
        let _ = (tenant_id, id);
        Ok(None)
    }
    async fn verify_chain(
        &self,
        scope: AuditChainScope,
    ) -> Result<AuditChainVerification, AuditError>;
}

#[must_use]
pub fn hash_record(event: &AuditEvent) -> String {
    let payload = serde_json::to_vec(&event.canonical_payload()).unwrap_or_default();
    let digest = Sha256::digest(payload);
    hex_encode(&digest)
}

fn redact_details(value: Value) -> Result<Value, AuditValidationError> {
    const FORBIDDEN: [&str; 10] = [
        "object_key",
        "storage_key",
        "signed_url",
        "lease_token",
        "database_url",
        "secret",
        "credential",
        "raw_text",
        "prompt",
        "provider_response",
    ];
    match value {
        Value::Object(mut object) => {
            for key in object.keys() {
                if FORBIDDEN
                    .iter()
                    .any(|forbidden: &&str| key.eq_ignore_ascii_case(forbidden))
                {
                    return Err(AuditValidationError::SensitiveDetails);
                }
            }
            for child in object.values_mut() {
                *child = redact_details(child.take())?;
            }
            Ok(Value::Object(object))
        }
        Value::Array(values) => values
            .into_iter()
            .map(redact_details)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        other => Ok(other),
    }
}

/// Sanitize legacy details before exposing them through a read API.
///
/// Older audit rows may contain fields that the unified event contract rejects
/// on write.  A reader must remain usable without returning those values.
#[must_use]
pub fn sanitize_details_for_read(value: Value) -> Value {
    const FORBIDDEN: [&str; 10] = [
        "object_key",
        "storage_key",
        "signed_url",
        "lease_token",
        "database_url",
        "secret",
        "credential",
        "raw_text",
        "prompt",
        "provider_response",
    ];
    match value {
        Value::Object(object) => {
            if object.keys().any(|key| {
                FORBIDDEN
                    .iter()
                    .any(|forbidden: &&str| key.eq_ignore_ascii_case(forbidden))
            }) {
                return serde_json::json!({"redacted": true});
            }
            Value::Object(
                object
                    .into_iter()
                    .map(|(key, child)| (key, sanitize_details_for_read(child)))
                    .collect(),
            )
        }
        Value::Array(values) => {
            Value::Array(values.into_iter().map(sanitize_details_for_read).collect())
        }
        other => other,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(result: AuditResult, details: Value) -> Result<AuditEvent, AuditValidationError> {
        AuditEvent::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            AuditActor {
                actor_type: AuditActorType::System,
                actor_id: Uuid::new_v4(),
            },
            AuditAction::new("document.created")?,
            AuditResource::new("document", "doc-1")?,
            Uuid::new_v4(),
            None,
            None,
            None,
            None,
            result,
            None,
            None,
            None,
            vec!["title".to_string(), "title".to_string()],
            details,
            "audit.v1",
            Utc::now(),
        )
    }

    #[test]
    fn validates_and_redacts_sensitive_details() {
        let valid_event = event(AuditResult::Succeeded, serde_json::json!({"ok": true}))
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(valid_event.changed_fields, vec!["title"]);
        assert!(event(
            AuditResult::Succeeded,
            serde_json::json!({"lease_token": "hidden"})
        )
        .is_err());
    }

    #[test]
    fn hash_chain_detects_payload_change() {
        let first = event(AuditResult::Succeeded, serde_json::json!({"n": 1}))
            .unwrap_or_else(|_| unreachable!())
            .with_chain(None);
        let second = event(AuditResult::Succeeded, serde_json::json!({"n": 2}))
            .unwrap_or_else(|_| unreachable!())
            .with_chain(first.record_hash.clone());
        assert_eq!(second.record_hash, Some(hash_record(&second)));
        let mut changed = second.clone();
        changed.details = serde_json::json!({"n": 3});
        assert_ne!(changed.record_hash, Some(hash_record(&changed)));
    }

    #[test]
    fn legacy_details_are_redacted_for_readers() {
        let sanitized = sanitize_details_for_read(serde_json::json!({
            "object_key": "private/path",
            "safe": "retained only when no sensitive sibling exists"
        }));
        assert_eq!(sanitized, serde_json::json!({"redacted": true}));
        assert!(!sanitized.to_string().contains("private/path"));
    }
}
