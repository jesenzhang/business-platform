//! Management-only runtime governance endpoints.
//!
//! Tenant and permission context are taken from authentication middleware.
//! Clients cannot select another tenant, submit SQL, or provide internal
//! storage/lease fields.

use std::sync::Arc;

use audit::{AuditActorType, AuditChainScope, AuditQueryRequest, AuditResult};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use chrono::{DateTime, Utc};
use data_integrity::{FindingStatus, IntegrityFinding, IntegrityScanScope};
use data_repair::{RepairCommand, RepairRun, RepairRunStatus, RepairStep};
use runtime_governance::dry_run_repair as execute_dry_run_repair;
use serde::{Deserialize, Serialize};
use shared_kernel::TenantContext;
use uuid::Uuid;

use crate::api_error::ApiError;
use crate::api_response::ApiResponse;
use crate::state::{AppState, GovernanceServices};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub resource_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingListQuery {
    pub status: Option<FindingStatus>,
    #[serde(default = "default_limit")]
    pub limit: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRequest {
    pub finding_id: Uuid,
    pub repair_type: String,
    pub repair_version: u32,
    pub idempotency_key: String,
    pub reason: String,
    pub expected_resource_version: Option<i64>,
    #[serde(default = "default_batch_limit")]
    pub batch_limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequest {
    pub note: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditQueryParams {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub operation_id: Option<Uuid>,
    pub trace_id: Option<String>,
    pub result: Option<String>,
    pub occurred_after: Option<DateTime<Utc>>,
    pub occurred_before: Option<DateTime<Utc>>,
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyChainRequest {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct AuditPageResponse {
    pub items: Vec<audit::AuditEvent>,
    pub next_cursor: Option<String>,
}

fn default_limit() -> u16 {
    50
}

fn default_batch_limit() -> u32 {
    1
}

fn context(context: &TenantContext) -> Result<(Uuid, Uuid), ApiError> {
    let tenant_id = Uuid::parse_str(&context.tenant_id)
        .map_err(|_| ApiError::validation("invalid tenant context"))?;
    let user_id = Uuid::parse_str(&context.user_id)
        .map_err(|_| ApiError::validation("invalid user context"))?;
    Ok((tenant_id, user_id))
}

fn require_permission(headers: &HeaderMap, permission: &str) -> Result<(), ApiError> {
    let allowed = headers
        .get("x-management-permissions")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == permission)
        });
    if allowed {
        Ok(())
    } else {
        Err(ApiError::from(shared_kernel::error::AppError::Forbidden(
            "management permission required".to_string(),
        )))
    }
}

fn governance(state: &AppState) -> Result<&GovernanceServices, ApiError> {
    state.governance.as_ref().ok_or_else(|| {
        ApiError::from(shared_kernel::error::AppError::ExternalService {
            service: "runtime governance".to_string(),
            message: "governance is unavailable".to_string(),
        })
    })
}

fn map_integrity_error(_: data_integrity::IntegrityError) -> ApiError {
    ApiError::from(shared_kernel::error::AppError::Database(
        "integrity operation failed".to_string(),
    ))
}

#[allow(clippy::needless_pass_by_value)]
fn map_repair_error(error: data_repair::RepairError) -> ApiError {
    match error {
        data_repair::RepairError::InvalidCommand | data_repair::RepairError::InvalidDescriptor => {
            ApiError::validation("invalid repair")
        }
        data_repair::RepairError::ApprovalSeparation
        | data_repair::RepairError::ApprovalRequired => ApiError::from(
            shared_kernel::error::AppError::Forbidden("repair approval required".to_string()),
        ),
        data_repair::RepairError::Conflict => ApiError::from(
            shared_kernel::error::AppError::Conflict("repair target changed".to_string()),
        ),
        data_repair::RepairError::Unavailable | data_repair::RepairError::Persistence => {
            ApiError::from(shared_kernel::error::AppError::Database(
                "repair persistence unavailable".to_string(),
            ))
        }
        data_repair::RepairError::LeaseLost => ApiError::from(
            shared_kernel::error::AppError::Conflict("repair lease lost".to_string()),
        ),
        data_repair::RepairError::InvalidTransition => ApiError::from(
            shared_kernel::error::AppError::Conflict("invalid repair transition".to_string()),
        ),
    }
}

pub async fn create_scan(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ScanRequest>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "integrity.scan")?;
    let (tenant_id, user_id) = context(&auth)?;
    let services = governance(&state)?;
    let report = services
        .scans
        .run(
            IntegrityScanScope {
                tenant_id: Some(tenant_id),
                resource_type: body.resource_type,
                resource_id: body.resource_id,
            },
            user_id,
        )
        .await
        .map_err(|_| map_integrity_error(data_integrity::IntegrityError::Persistence))?;
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(report.run))).into_response())
}

pub async fn list_scans(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_permission(&headers, "integrity.read")?;
    let (tenant_id, _) = context(&auth)?;
    let services = governance(&state)?;
    let runs = services
        .integrity_queries
        .list_scan_runs(Some(tenant_id), default_limit())
        .await
        .map_err(map_integrity_error)?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(runs))).into_response())
}

pub async fn get_scan(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "integrity.read")?;
    let (tenant_id, _) = context(&auth)?;
    let services = governance(&state)?;
    let run = services
        .integrity_queries
        .get_scan_run(Some(tenant_id), id)
        .await
        .map_err(map_integrity_error)?
        .ok_or_else(|| ApiError::not_found("integrity_scan", id))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(run))).into_response())
}

pub async fn list_findings(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<FindingListQuery>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "integrity.read")?;
    let (tenant_id, _) = context(&auth)?;
    let services = governance(&state)?;
    let findings = services
        .integrity_queries
        .list_findings(tenant_id, query.status, query.limit)
        .await
        .map_err(map_integrity_error)?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(findings))).into_response())
}

pub async fn get_finding(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "integrity.read")?;
    let (tenant_id, _) = context(&auth)?;
    let services = governance(&state)?;
    let finding = services
        .integrity_persistence
        .load_finding(id)
        .await
        .map_err(map_integrity_error)?
        .filter(|finding| finding.tenant_id == tenant_id)
        .ok_or_else(|| ApiError::not_found("integrity_finding", id))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(finding))).into_response())
}

fn command(body: RepairRequest, tenant_id: Uuid, user_id: Uuid) -> RepairCommand {
    RepairCommand {
        idempotency_key: body.idempotency_key,
        tenant_id,
        finding_id: body.finding_id,
        repair_type: body.repair_type,
        repair_version: body.repair_version,
        requested_by: user_id,
        reason: body.reason,
        expected_resource_version: body.expected_resource_version,
        batch_limit: body.batch_limit,
    }
}

fn owner_command(
    command: &RepairCommand,
    finding: &IntegrityFinding,
) -> Result<RepairCommand, ApiError> {
    if finding.resource_type != "processing_job" {
        return Err(ApiError::validation("unsupported repair resource"));
    }
    let resource_id = Uuid::parse_str(&finding.resource_id)
        .map_err(|_| ApiError::validation("invalid repair resource"))?;
    let mut owner_command = command.clone();
    owner_command.finding_id = resource_id;
    Ok(owner_command)
}

pub async fn dry_run_repair(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<RepairRequest>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "repair.dry-run")?;
    let (tenant_id, user_id) = context(&auth)?;
    let services = governance(&state)?;
    let command = command(body, tenant_id, user_id);
    let finding = services
        .integrity_persistence
        .load_finding(command.finding_id)
        .await
        .map_err(map_integrity_error)?
        .filter(|finding| finding.tenant_id == tenant_id)
        .ok_or_else(|| ApiError::not_found("integrity_finding", command.finding_id))?;
    if finding.rule_id.is_empty() {
        return Err(ApiError::validation("invalid integrity finding"));
    }
    let owner_command = owner_command(&command, &finding)?;
    let handler = services
        .repair_handlers
        .get(&owner_command.repair_type, owner_command.repair_version)
        .await
        .ok_or_else(|| ApiError::not_found("repair_handler", command.repair_type.clone()))?;
    let mut preview = execute_dry_run_repair(handler.as_ref(), &owner_command)
        .await
        .map_err(|error| match error {
            runtime_governance::GovernanceError::Repair(error) => map_repair_error(error),
            runtime_governance::GovernanceError::Integrity(_) => {
                ApiError::validation("invalid repair")
            }
        })?;
    preview.finding_id = command.finding_id;
    preview.resource_id = finding.resource_id;
    Ok((StatusCode::OK, Json(ApiResponse::ok(preview))).into_response())
}

pub async fn create_repair(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<RepairRequest>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "repair.execute")?;
    let (tenant_id, user_id) = context(&auth)?;
    let services = governance(&state)?;
    let command = command(body, tenant_id, user_id);
    let finding = services
        .integrity_persistence
        .load_finding(command.finding_id)
        .await
        .map_err(map_integrity_error)?
        .filter(|finding| finding.tenant_id == tenant_id)
        .ok_or_else(|| ApiError::not_found("integrity_finding", command.finding_id))?;
    if matches!(
        finding.status,
        FindingStatus::Repaired | FindingStatus::FalsePositive | FindingStatus::Stale
    ) {
        return Err(ApiError::from(shared_kernel::error::AppError::Conflict(
            "integrity finding is already resolved".to_string(),
        )));
    }
    if let Some(existing) = services
        .repair_persistence
        .load_run_by_idempotency(tenant_id, &command.idempotency_key)
        .await
        .map_err(map_repair_error)?
    {
        if existing.command == command {
            return Ok((StatusCode::OK, Json(ApiResponse::ok(existing))).into_response());
        }
        return Err(ApiError::from(shared_kernel::error::AppError::Conflict(
            "idempotency key is already used for another repair".to_string(),
        )));
    }
    let owner_command = owner_command(&command, &finding)?;
    let handler = services
        .repair_handlers
        .get(&owner_command.repair_type, owner_command.repair_version)
        .await
        .ok_or_else(|| ApiError::not_found("repair_handler", command.repair_type.clone()))?;
    let mut preview = execute_dry_run_repair(handler.as_ref(), &owner_command)
        .await
        .map_err(|error| match error {
            runtime_governance::GovernanceError::Repair(error) => map_repair_error(error),
            runtime_governance::GovernanceError::Integrity(_) => {
                ApiError::validation("invalid repair")
            }
        })?;
    preview.finding_id = command.finding_id;
    preview.resource_id = finding.resource_id.clone();
    let now = Utc::now();
    let descriptor = preview.descriptor;
    let status = if descriptor.requires_approval {
        RepairRunStatus::AwaitingApproval
    } else {
        RepairRunStatus::Queued
    };
    let run = RepairRun {
        id: Uuid::now_v7(),
        tenant_id,
        finding_id: finding.id,
        command,
        status,
        created_by: user_id,
        approved_by: None,
        approval_note: None,
        created_at: now,
        updated_at: now,
        version: 0,
    };
    services
        .repair_persistence
        .save_run(&run)
        .await
        .map_err(map_repair_error)?;
    let step = RepairStep {
        id: Uuid::now_v7(),
        run_id: run.id,
        finding_id: run.finding_id,
        status,
        attempt_count: 0,
        checkpoint: None,
        lease_owner: None,
        lease_token: None,
        fence_version: 0,
        lease_expires_at: None,
        next_attempt_at: now,
    };
    services
        .repair_persistence
        .save_step(&step)
        .await
        .map_err(map_repair_error)?;
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(run))).into_response())
}

pub async fn get_repair(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "repair.execute")?;
    let (tenant_id, _) = context(&auth)?;
    let services = governance(&state)?;
    let run = services
        .repair_persistence
        .load_run(id)
        .await
        .map_err(map_repair_error)?
        .filter(|run| run.tenant_id == tenant_id)
        .ok_or_else(|| ApiError::not_found("repair_run", id))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(run))).into_response())
}

pub async fn approve_repair(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ApprovalRequest>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "repair.approve")?;
    let (tenant_id, approver) = context(&auth)?;
    let services = governance(&state)?;
    let mut run = services
        .repair_persistence
        .load_run(id)
        .await
        .map_err(map_repair_error)?
        .filter(|run| run.tenant_id == tenant_id)
        .ok_or_else(|| ApiError::not_found("repair_run", id))?;
    run.approve(approver, body.note).map_err(map_repair_error)?;
    services
        .repair_persistence
        .save_run(&run)
        .await
        .map_err(map_repair_error)?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(run))).into_response())
}

pub async fn cancel_repair(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "repair.cancel")?;
    let (tenant_id, _) = context(&auth)?;
    let services = governance(&state)?;
    let mut run = services
        .repair_persistence
        .load_run(id)
        .await
        .map_err(map_repair_error)?
        .filter(|run| run.tenant_id == tenant_id)
        .ok_or_else(|| ApiError::not_found("repair_run", id))?;
    if matches!(
        run.status,
        RepairRunStatus::Succeeded | RepairRunStatus::Cancelled
    ) {
        return Err(ApiError::from(shared_kernel::error::AppError::Conflict(
            "repair already completed".to_string(),
        )));
    }
    run.status = RepairRunStatus::Cancelled;
    run.version = run.version.saturating_add(1);
    run.updated_at = Utc::now();
    services
        .repair_persistence
        .save_run(&run)
        .await
        .map_err(map_repair_error)?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(run))).into_response())
}

pub async fn resume_repair(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "repair.execute")?;
    let (tenant_id, _) = context(&auth)?;
    let services = governance(&state)?;
    let mut run = services
        .repair_persistence
        .load_run(id)
        .await
        .map_err(map_repair_error)?
        .filter(|run| run.tenant_id == tenant_id)
        .ok_or_else(|| ApiError::not_found("repair_run", id))?;
    if !matches!(
        run.status,
        RepairRunStatus::Cancelled | RepairRunStatus::Failed | RepairRunStatus::NeedsManualReview
    ) {
        return Err(ApiError::from(shared_kernel::error::AppError::Conflict(
            "repair is not resumable in its current state".to_string(),
        )));
    }
    run.status = if run.approved_by.is_some() {
        RepairRunStatus::Queued
    } else {
        RepairRunStatus::AwaitingApproval
    };
    run.version = run.version.saturating_add(1);
    run.updated_at = Utc::now();
    services
        .repair_persistence
        .save_run(&run)
        .await
        .map_err(map_repair_error)?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(run))).into_response())
}

fn parse_cursor(value: Option<String>) -> Result<Option<audit::AuditCursor>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ApiError::validation("invalid audit cursor"))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| ApiError::validation("invalid audit cursor"))
}

fn encode_cursor(cursor: Option<audit::AuditCursor>) -> Result<Option<String>, ApiError> {
    cursor
        .map(|cursor| {
            let bytes = serde_json::to_vec(&cursor).map_err(|_| {
                ApiError::from(shared_kernel::error::AppError::Internal(
                    "cursor encoding failed".to_string(),
                ))
            })?;
            Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
        })
        .transpose()
}

pub async fn list_audit_events(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<AuditQueryParams>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "audit.read")?;
    let (tenant_id, _) = context(&auth)?;
    let actor = query.actor.as_deref().and_then(parse_actor_type);
    let result = query.result.as_deref().and_then(parse_result);
    let request = AuditQueryRequest {
        tenant_id,
        actor,
        action: query.action,
        resource_type: query.resource_type,
        resource_id: query.resource_id,
        operation_id: query.operation_id,
        trace_id: query.trace_id,
        result,
        occurred_after: query.occurred_after,
        occurred_before: query.occurred_before,
        cursor: parse_cursor(query.cursor)?,
        limit: query.limit,
    };
    let page = governance(&state)?
        .audit_queries
        .list(request)
        .await
        .map_err(|_| {
            ApiError::from(shared_kernel::error::AppError::Database(
                "audit query failed".to_string(),
            ))
        })?;
    let response = AuditPageResponse {
        items: page.items,
        next_cursor: encode_cursor(page.next_cursor)?,
    };
    Ok((StatusCode::OK, Json(ApiResponse::ok(response))).into_response())
}

pub async fn get_audit_event(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "audit.read")?;
    let (tenant_id, _) = context(&auth)?;
    let event = governance(&state)?
        .audit_queries
        .get(tenant_id, id)
        .await
        .map_err(|_| {
            ApiError::from(shared_kernel::error::AppError::Database(
                "audit query failed".to_string(),
            ))
        })?
        .ok_or_else(|| ApiError::not_found("audit_event", id))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(event))).into_response())
}

pub async fn verify_audit_chain(
    axum::Extension(auth): axum::Extension<TenantContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<VerifyChainRequest>,
) -> Result<Response, ApiError> {
    require_permission(&headers, "audit.read")?;
    let (tenant_id, _) = context(&auth)?;
    let verification = governance(&state)?
        .audit_queries
        .verify_chain(AuditChainScope {
            tenant_id,
            from: body.from,
            to: body.to,
        })
        .await
        .map_err(|_| {
            ApiError::from(shared_kernel::error::AppError::Database(
                "audit verification failed".to_string(),
            ))
        })?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(verification))).into_response())
}

fn parse_actor_type(value: &str) -> Option<AuditActorType> {
    match value {
        "user" => Some(AuditActorType::User),
        "service" => Some(AuditActorType::Service),
        "worker" => Some(AuditActorType::Worker),
        "repair_job" | "repairjob" => Some(AuditActorType::RepairJob),
        "system" => Some(AuditActorType::System),
        _ => None,
    }
}

fn parse_result(value: &str) -> Option<AuditResult> {
    match value {
        "succeeded" => Some(AuditResult::Succeeded),
        "failed" => Some(AuditResult::Failed),
        "denied" => Some(AuditResult::Denied),
        "cancelled" => Some(AuditResult::Cancelled),
        _ => None,
    }
}
