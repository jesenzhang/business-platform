//! Unified, validated audit evidence for business and runtime governance.
//!
//! The crate intentionally contains no database, HTTP, storage, or provider
//! dependency. Adapters persist the validated event inside the owner context's
//! transaction.

use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
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
    #[error("audit chain metadata is invalid")]
    InvalidChainMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEvent {
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
    schema_version: String,
    occurred_at: DateTime<Utc>,
    /// Immutable tenant-local append order assigned by the persistence
    /// adapter. Zero means the event has not yet been persisted.
    #[serde(default)]
    stream_sequence: i64,
    /// Database recording time. This is deliberately distinct from the
    /// business occurrence timestamp above.
    #[serde(default = "Utc::now")]
    recorded_at: DateTime<Utc>,
    /// `0` is the explicit legacy/unverified history boundary; `1` is the
    /// sequence-based chain written by Revision 1 adapters.
    chain_version: i16,
    previous_hash: Option<String>,
    record_hash: Option<String>,
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
        let occurred_at = canonical_timestamp(occurred_at);
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
            stream_sequence: 0,
            recorded_at: occurred_at,
            chain_version: 1,
            previous_hash: None,
            record_hash: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
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
        schema_version: String,
        occurred_at: DateTime<Utc>,
        recorded_at: DateTime<Utc>,
        stream_sequence: i64,
        chain_version: i16,
        previous_hash: Option<String>,
        record_hash: Option<String>,
    ) -> Result<Self, AuditError> {
        if stream_sequence < 0
            || !matches!(chain_version, 0 | 1)
            || (chain_version == 1 && stream_sequence == 0)
            || (chain_version == 0 && (previous_hash.is_some() || record_hash.is_some()))
            || (chain_version == 1
                && (previous_hash
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
                    || record_hash
                        .as_deref()
                        .is_none_or(|value| value.trim().is_empty())))
        {
            return Err(AuditValidationError::InvalidChainMetadata.into());
        }
        let mut event = Self::new(
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
        )?;
        // `new` canonicalizes `occurred_at` to microsecond precision because a
        // newly built event is about to be signed and written. A stored row must
        // come back byte for byte instead: history can carry nanosecond
        // `occurred_at` values, and restoring the stored instant verbatim is what
        // lets `hash_record` reproduce the digest the row was signed with.
        // Assigning only `recorded_at` here would silently truncate the other
        // hashed timestamp and report intact history as broken.
        event.occurred_at = occurred_at;
        event.recorded_at = recorded_at;
        event.stream_sequence = stream_sequence;
        event.chain_version = chain_version;
        event.previous_hash = previous_hash;
        event.record_hash = record_hash;
        Ok(event)
    }

    pub fn with_chain(mut self, previous_hash: Option<String>) -> Result<Self, AuditError> {
        self.previous_hash = previous_hash;
        self.record_hash = Some(hash_record(&self)?);
        Ok(self)
    }

    pub fn with_chain_metadata(
        mut self,
        stream_sequence: i64,
        recorded_at: DateTime<Utc>,
        chain_version: i16,
        previous_hash: Option<String>,
    ) -> Result<Self, AuditError> {
        if stream_sequence <= 0
            || chain_version != 1
            || previous_hash
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(AuditValidationError::InvalidChainMetadata.into());
        }
        self.stream_sequence = stream_sequence;
        // Both hashed timestamps are normalized here, not just `recorded_at`.
        // This is the transition that signs a protected row about to be written,
        // so the event must carry the precision every supported backend can
        // store even when it reached the adapter through `rehydrate` carrying a
        // historical nanosecond `occurred_at`. Normalizing only at construction
        // would let a re-signed historical row recreate the unverifiable-row
        // defect on `PostgreSQL`.
        self.occurred_at = canonical_timestamp(self.occurred_at);
        self.recorded_at = canonical_timestamp(recorded_at);
        self.chain_version = chain_version;
        self.previous_hash = previous_hash;
        self.record_hash = Some(hash_record(&self)?);
        Ok(self)
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
    pub fn actor(&self) -> &AuditActor {
        &self.actor
    }
    #[must_use]
    pub fn action(&self) -> &AuditAction {
        &self.action
    }
    #[must_use]
    pub fn resource(&self) -> &AuditResource {
        &self.resource
    }
    #[must_use]
    pub fn operation_id(&self) -> Uuid {
        self.operation_id
    }
    #[must_use]
    pub fn correlation_id(&self) -> Option<Uuid> {
        self.correlation_id
    }
    #[must_use]
    pub fn causation_id(&self) -> Option<Uuid> {
        self.causation_id
    }
    #[must_use]
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    #[must_use]
    pub fn result(&self) -> AuditResult {
        self.result
    }
    #[must_use]
    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }
    #[must_use]
    pub fn before_hash(&self) -> Option<&str> {
        self.before_hash.as_deref()
    }
    #[must_use]
    pub fn after_hash(&self) -> Option<&str> {
        self.after_hash.as_deref()
    }
    #[must_use]
    pub fn changed_fields(&self) -> &[String] {
        &self.changed_fields
    }
    #[must_use]
    pub fn details(&self) -> &Value {
        &self.details
    }
    #[must_use]
    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }
    #[must_use]
    pub fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
    #[must_use]
    pub fn stream_sequence(&self) -> i64 {
        self.stream_sequence
    }
    #[must_use]
    pub fn recorded_at(&self) -> DateTime<Utc> {
        self.recorded_at
    }
    #[must_use]
    pub fn chain_version(&self) -> i16 {
        self.chain_version
    }
    #[must_use]
    pub fn previous_hash(&self) -> Option<&str> {
        self.previous_hash.as_deref()
    }
    #[must_use]
    pub fn record_hash(&self) -> Option<&str> {
        self.record_hash.as_deref()
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
            "stream_sequence": self.stream_sequence,
            "recorded_at": self.recorded_at,
            "chain_version": self.chain_version,
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
    #[error("audit persistence contains an unsupported enum value")]
    InvalidStoredEnum,
    #[error("audit chain verification failed")]
    ChainBroken,
    #[error("audit query cursor is invalid")]
    InvalidCursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditCursor {
    #[serde(default = "default_cursor_version")]
    pub version: u8,
    #[serde(default)]
    pub stream_sequence: i64,
    pub occurred_at: DateTime<Utc>,
    pub id: Uuid,
}

fn default_cursor_version() -> u8 {
    1
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
    pub legacy_count: u64,
    pub verified_count: u64,
    pub valid: bool,
    pub first_broken_id: Option<Uuid>,
    pub chain_version: i16,
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
    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<AuditEvent>, AuditError> {
        Err(AuditError::Persistence)
    }
    async fn verify_chain(
        &self,
        scope: AuditChainScope,
    ) -> Result<AuditChainVerification, AuditError>;
}

pub fn hash_record(event: &AuditEvent) -> Result<String, AuditError> {
    let payload =
        serde_json::to_vec(&event.canonical_payload()).map_err(|_| AuditError::Persistence)?;
    let digest = Sha256::digest(payload);
    Ok(hex_encode(&digest))
}

/// Canonical precision for every timestamp that enters `record_hash`.
///
/// `record_hash` is recomputed from stored rows during verification, so a hashed
/// timestamp must survive a persistence round trip unchanged on every supported
/// backend. `PostgreSQL` `TIMESTAMPTZ` stores microseconds while
/// `chrono::Utc::now()` carries nanoseconds: without this normalization the
/// digest written at append time no longer matches the digest recomputed from
/// the stored row, and a freshly written chain reports itself as broken.
///
/// Only the sub-second remainder is truncated. The implementation must not
/// rebuild the instant from `timestamp_micros() * 1_000`, because the epoch
/// microsecond count exceeds `i64` nanoseconds past the year 2262: that
/// arithmetic overflows instead of normalizing. Truncating the sub-second
/// component in place keeps the calendar date and the whole second identical for
/// every representable `DateTime`, including pre-epoch instants.
///
/// `rehydrate` deliberately does not normalize. Verification compares against
/// the bytes a row was actually signed with, so stored timestamps are preserved
/// exactly and existing history is never reinterpreted or re-signed.
#[must_use]
pub fn canonical_timestamp(value: DateTime<Utc>) -> DateTime<Utc> {
    let microsecond_nanos = value.timestamp_subsec_nanos() / 1_000 * 1_000;
    value.with_nanosecond(microsecond_nanos).unwrap_or(value)
}

#[must_use]
pub fn audit_chain_genesis(tenant_id: Uuid) -> String {
    format!("GENESIS:audit:v1:{tenant_id}")
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
    // Only the tests still build instants from raw nanosecond counts; production
    // code normalizes sub-seconds in place and needs no `TimeZone`.
    use chrono::TimeZone as _;

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

    /// Fixed instants and identifiers for the historical-compatibility fixture.
    /// `occurred_at` and `recorded_at` both carry a non-zero nanosecond tail,
    /// which is what `SQLite` history can contain because it stores RFC 3339 text.
    const HISTORICAL_NANOS: u32 = 123_456_789;
    const HISTORICAL_OCCURRED_AT: &str = "2026-09-26T12:00:00.123456789Z";
    const HISTORICAL_ID: &str = "49ed89fe-7a4a-4c82-9b6a-3f1c2d5e6a01";
    const HISTORICAL_TENANT: &str = "49ed89fe-7a4a-4c82-9b6a-3f1c2d5e6a02";
    const HISTORICAL_ACTOR: &str = "49ed89fe-7a4a-4c82-9b6a-3f1c2d5e6a03";
    const HISTORICAL_OPERATION: &str = "49ed89fe-7a4a-4c82-9b6a-3f1c2d5e6a04";
    /// `record_hash` of the fixture above, produced by the historical
    /// (pre-normalization) algorithm: SHA-256 over the canonical payload with
    /// both timestamps at full nanosecond precision. It is a literal on purpose.
    /// Deriving it from the constructor or from `canonical_timestamp` would make
    /// the assertion circular and hide exactly the regression it pins, so it was
    /// frozen once and independently re-hashed outside this crate.
    const HISTORICAL_RECORD_HASH: &str =
        "12b8823dbe1ea4ebaaf7896c676a52b790dbc004668166b203988e42f7cdf815";

    fn historical_instant() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(HISTORICAL_OCCURRED_AT)
            .unwrap_or_else(|_| unreachable!("fixture instant is valid RFC 3339"))
            .with_timezone(&Utc)
    }

    fn fixed_uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).unwrap_or_else(|_| unreachable!("fixture uuid is canonical"))
    }

    /// Reads the fixture row back through the real `rehydrate` entry point every
    /// adapter uses, with the digest the row was historically signed with.
    fn rehydrate_historical_row(
        occurred_at: DateTime<Utc>,
        recorded_at: DateTime<Utc>,
        record_hash: &str,
    ) -> Result<AuditEvent, AuditError> {
        let tenant_id = fixed_uuid(HISTORICAL_TENANT);
        AuditEvent::rehydrate(
            fixed_uuid(HISTORICAL_ID),
            tenant_id,
            AuditActor {
                actor_type: AuditActorType::System,
                actor_id: fixed_uuid(HISTORICAL_ACTOR),
            },
            AuditAction::new("document.created").unwrap_or_else(|_| unreachable!()),
            AuditResource::new("document", "doc-historical").unwrap_or_else(|_| unreachable!()),
            fixed_uuid(HISTORICAL_OPERATION),
            None,
            None,
            Some("trace-historical".to_string()),
            None,
            AuditResult::Succeeded,
            None,
            None,
            None,
            vec!["title".to_string()],
            serde_json::json!({"source": "round-2c"}),
            "audit.v1".to_string(),
            occurred_at,
            recorded_at,
            1,
            1,
            Some(audit_chain_genesis(tenant_id)),
            Some(record_hash.to_string()),
        )
    }

    #[test]
    fn rehydrate_preserves_historical_nanosecond_signed_content() {
        // Reading history back must not change what history was signed over.
        // A nanosecond tail is the only case that distinguishes the two, and
        // truncating it turns an intact chain into a reported-broken chain.
        let stored_instant = historical_instant();
        assert_eq!(
            stored_instant.timestamp_subsec_nanos(),
            HISTORICAL_NANOS,
            "fixture must actually carry a sub-microsecond tail"
        );

        let event =
            rehydrate_historical_row(stored_instant, stored_instant, HISTORICAL_RECORD_HASH)
                .unwrap_or_else(|_| unreachable!("fixture row is a valid v1 row"));

        assert_eq!(
            event.occurred_at(),
            stored_instant,
            "rehydrate must restore the stored occurred_at verbatim"
        );
        assert_eq!(
            event.recorded_at(),
            stored_instant,
            "rehydrate must restore the stored recorded_at verbatim"
        );
        assert_eq!(
            event.occurred_at().timestamp_subsec_nanos(),
            HISTORICAL_NANOS,
            "a hashed timestamp must not be silently truncated on read"
        );

        // The stored digest reproduces exactly, so a chain verifier accepts it.
        assert_eq!(
            hash_record(&event).unwrap_or_else(|_| unreachable!()),
            HISTORICAL_RECORD_HASH,
            "a historically signed nanosecond row must re-verify after rehydrate"
        );
        assert_eq!(event.record_hash(), Some(HISTORICAL_RECORD_HASH));
        assert_eq!(event.chain_version(), 1);
        assert_eq!(event.stream_sequence(), 1);

        // Negative control: verification is live, not skipped. One nanosecond of
        // drift on either hashed timestamp must break the match.
        let drifted_occurred = stored_instant
            .with_nanosecond(HISTORICAL_NANOS - 1)
            .unwrap_or_else(|| unreachable!("sub-second tail stays in range"));
        let tampered =
            rehydrate_historical_row(drifted_occurred, stored_instant, HISTORICAL_RECORD_HASH)
                .unwrap_or_else(|_| unreachable!());
        assert_ne!(
            hash_record(&tampered).unwrap_or_else(|_| unreachable!()),
            HISTORICAL_RECORD_HASH,
            "the fixture digest must stay nanosecond sensitive"
        );
        let drifted_recorded = stored_instant
            .with_nanosecond(HISTORICAL_NANOS + 1)
            .unwrap_or_else(|| unreachable!("sub-second tail stays in range"));
        let tampered =
            rehydrate_historical_row(stored_instant, drifted_recorded, HISTORICAL_RECORD_HASH)
                .unwrap_or_else(|_| unreachable!());
        assert_ne!(
            hash_record(&tampered).unwrap_or_else(|_| unreachable!()),
            HISTORICAL_RECORD_HASH
        );
    }

    #[test]
    fn re_signing_a_historical_row_leaves_no_nanosecond_hashed_input() {
        // Reading history verbatim must not leak into a row that is about to be
        // written: an adapter that rehydrates a nanosecond row and then signs it
        // onto a chain would otherwise store a digest PostgreSQL cannot reproduce.
        let stored_instant = historical_instant();
        let historical =
            rehydrate_historical_row(stored_instant, stored_instant, HISTORICAL_RECORD_HASH)
                .unwrap_or_else(|_| unreachable!("fixture row is a valid v1 row"));
        assert_eq!(
            historical.occurred_at().timestamp_subsec_nanos(),
            HISTORICAL_NANOS
        );

        let resigned = historical
            .with_chain_metadata(
                2,
                stored_instant,
                1,
                Some(HISTORICAL_RECORD_HASH.to_string()),
            )
            .unwrap_or_else(|_| unreachable!("re-signing a valid row must succeed"));

        assert_eq!(
            resigned.occurred_at().timestamp_subsec_nanos(),
            123_456_000,
            "a row being signed for storage must not keep a sub-microsecond tail"
        );
        assert_eq!(resigned.recorded_at().timestamp_subsec_nanos(), 123_456_000);
        let resigned_digest = resigned
            .record_hash()
            .unwrap_or_else(|| unreachable!("row was just signed"))
            .to_string();
        assert_eq!(
            hash_record(&resigned).unwrap_or_else(|_| unreachable!()),
            resigned_digest,
            "the re-signed digest must be reproducible from stored precision"
        );
        assert_ne!(
            resigned.record_hash.as_deref(),
            Some(HISTORICAL_RECORD_HASH),
            "re-signing is a new row, never a silent rewrite of the historical digest"
        );
    }

    #[test]
    fn canonical_timestamp_truncates_without_leaving_the_representable_range() {
        let nanosecond_instant = historical_instant();
        let truncated = canonical_timestamp(nanosecond_instant);
        assert_eq!(truncated.timestamp_subsec_nanos(), 123_456_000);
        assert_eq!(canonical_timestamp(truncated), truncated);

        // Instants outside the i64 nanosecond window, in both directions: the
        // whole second and the calendar date must survive, and normalization
        // must not panic. Rebuilding an instant as epoch micros * 1_000 overflows
        // past the year 2262 instead of truncating.
        for text in [
            "1969-07-20T20:17:40.123456789Z",
            "1900-01-01T00:00:00.000000001Z",
            "2262-01-01T00:00:00.999999999Z",
            "2500-12-31T23:59:59.987654321Z",
        ] {
            let value = DateTime::parse_from_rfc3339(text)
                .unwrap_or_else(|_| unreachable!("fixture instant {text} is valid"))
                .with_timezone(&Utc);
            let truncated = canonical_timestamp(value);
            assert_eq!(
                truncated.timestamp(),
                value.timestamp(),
                "{text}: normalization must not move the whole second"
            );
            assert_eq!(
                truncated.naive_utc().date(),
                value.naive_utc().date(),
                "{text}: normalization must not move the calendar date"
            );
            assert_eq!(
                truncated.timestamp_subsec_nanos() % 1_000,
                0,
                "{text}: the result must be microsecond aligned"
            );
            assert!(
                truncated.timestamp_subsec_nanos() <= value.timestamp_subsec_nanos(),
                "{text}: sub-second remainder must be truncated, never rounded up"
            );
        }
    }

    #[test]
    fn hash_inputs_survive_a_microsecond_persistence_round_trip() {
        // PostgreSQL TIMESTAMPTZ stores microseconds. A timestamp that enters
        // record_hash must therefore already be microsecond aligned, otherwise
        // the digest recomputed from the stored row no longer matches the one
        // written at append time and a healthy chain verifies as broken.
        let nanosecond_instant = Utc.timestamp_nanos(1_761_000_000_123_456_789);
        assert_eq!(nanosecond_instant.timestamp_subsec_nanos(), 123_456_789);
        let persisted = canonical_timestamp(nanosecond_instant);
        assert_eq!(persisted.timestamp_subsec_nanos(), 123_456_000);
        assert_eq!(canonical_timestamp(persisted), persisted);

        let base = event(AuditResult::Succeeded, serde_json::json!({"n": 1}))
            .unwrap_or_else(|_| unreachable!());
        let tenant = base.tenant_id();
        let mut signed = base
            .with_chain_metadata(1, nanosecond_instant, 1, Some(audit_chain_genesis(tenant)))
            .unwrap_or_else(|_| unreachable!());
        // Both hashed instants are backend representable before signing.
        assert_eq!(signed.recorded_at.timestamp_subsec_nanos() % 1_000, 0);
        assert_eq!(signed.occurred_at.timestamp_subsec_nanos() % 1_000, 0);
        let signed_digest = signed
            .record_hash
            .clone()
            .unwrap_or_else(|| unreachable!("chain metadata must sign the row"));
        // Rehydrate the row the way an adapter reads it back from PostgreSQL.
        signed.occurred_at = canonical_timestamp(signed.occurred_at);
        signed.recorded_at = canonical_timestamp(signed.recorded_at);
        assert_eq!(
            hash_record(&signed).unwrap_or_else(|_| unreachable!()),
            signed_digest,
            "a row read back from a microsecond-precision backend must re-verify"
        );
        // Negative control: the hash really is nanosecond sensitive, so the
        // normalization above is what keeps the chain verifiable.
        let mut drifted = signed.clone();
        let recorded_nanos = drifted
            .recorded_at
            .timestamp_nanos_opt()
            .unwrap_or_else(|| unreachable!("a microsecond-aligned instant fits i64 nanos"));
        drifted.recorded_at = Utc.timestamp_nanos(recorded_nanos + 1);
        assert_ne!(
            hash_record(&drifted).unwrap_or_else(|_| unreachable!()),
            signed_digest
        );
    }

    #[test]
    fn hash_chain_detects_payload_change() {
        let first = event(AuditResult::Succeeded, serde_json::json!({"n": 1}))
            .unwrap_or_else(|_| unreachable!())
            .with_chain(None)
            .unwrap_or_else(|_| unreachable!());
        let second = event(AuditResult::Succeeded, serde_json::json!({"n": 2}))
            .unwrap_or_else(|_| unreachable!())
            .with_chain(first.record_hash.clone())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            second.record_hash,
            Some(hash_record(&second).unwrap_or_else(|_| unreachable!()))
        );
        let mut changed = second.clone();
        changed.details = serde_json::json!({"n": 3});
        assert_ne!(
            changed.record_hash,
            Some(hash_record(&changed).unwrap_or_else(|_| unreachable!()))
        );
    }

    #[test]
    fn append_sequence_is_independent_of_business_time() {
        let tenant = Uuid::new_v4();
        let first_time = Utc::now();
        let first = AuditEvent::new(
            Uuid::new_v4(),
            tenant,
            AuditActor {
                actor_type: AuditActorType::System,
                actor_id: Uuid::new_v4(),
            },
            AuditAction::new("audit.first").unwrap_or_else(|_| unreachable!()),
            AuditResource::new("job", "1").unwrap_or_else(|_| unreachable!()),
            Uuid::new_v4(),
            None,
            None,
            None,
            None,
            AuditResult::Succeeded,
            None,
            None,
            None,
            Vec::new(),
            serde_json::json!({}),
            "audit.v1",
            first_time,
        )
        .unwrap_or_else(|_| unreachable!())
        .with_chain_metadata(1, first_time, 1, Some(audit_chain_genesis(tenant)))
        .unwrap_or_else(|_| unreachable!());
        let second = AuditEvent::new(
            Uuid::new_v4(),
            tenant,
            AuditActor {
                actor_type: AuditActorType::System,
                actor_id: Uuid::new_v4(),
            },
            AuditAction::new("audit.second").unwrap_or_else(|_| unreachable!()),
            AuditResource::new("job", "1").unwrap_or_else(|_| unreachable!()),
            Uuid::new_v4(),
            None,
            None,
            None,
            None,
            AuditResult::Succeeded,
            None,
            None,
            None,
            Vec::new(),
            serde_json::json!({}),
            "audit.v1",
            first_time - chrono::Duration::hours(1),
        )
        .unwrap_or_else(|_| unreachable!())
        .with_chain_metadata(
            2,
            first_time + chrono::Duration::seconds(1),
            1,
            first.record_hash.clone(),
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(second.stream_sequence, first.stream_sequence + 1);
        assert_eq!(second.previous_hash, first.record_hash);
        assert_eq!(
            second.record_hash,
            Some(hash_record(&second).unwrap_or_else(|_| unreachable!()))
        );
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
