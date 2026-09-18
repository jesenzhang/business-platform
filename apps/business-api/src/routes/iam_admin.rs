//! Minimal IAM management REST surface (PLAN-0013 Stage 8, preflight §9).
//!
//! Handlers contain no business rules: each one extracts the request's
//! [`AuthorizationContext`] (installed by the Stage 7 platform-authorization
//! middleware), enforces the mapped catalog permission key through
//! [`authorize_permission`], extracts the `Idempotency-Key` header and body
//! DTO, delegates to the owning application use case, maps the application
//! error onto the existing stable error codes, and serializes the bounded
//! public DTO. Tenant and acting-user identity always come from the
//! resolved [`AuthorizationContext`], never from request fields.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, put};
use axum::{Json, Router};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use identity::application::{
    ChangeTenantMembershipStatusCommand, ChangeTenantMembershipStatusError, CreateMembershipTarget,
    CreateTenantMembershipCommand, CreateTenantMembershipError, IdentityPageCursor,
    IdentityQueryError,
};
use identity::domain::{MembershipSource, MembershipStatus};
use identity::ports::{MembershipRecord, TenantUserRecord};
use organization::application::{
    AddOrganizationMemberCommand, CreateOrganizationUnitCommand, MoveOrganizationUnitCommand,
    OrganizationApplicationError, RemoveOrganizationMemberCommand, UpdateOrganizationUnitCommand,
};
use organization::domain::{
    OrganizationMembershipType, OrganizationUnitStatus, OrganizationUnitType,
};
use policy::application::{
    AuthorizationContext, BindRoleCommand, CreateRoleCommand, PolicyApplicationError,
    RevokeRoleBindingCommand, SetRolePermissionsCommand, UpdateRoleCommand,
};
use policy::domain::{
    PermissionDefinition, PolicyDecision, ResourceScope, RoleBinding, RoleDefinition, RoleStatus,
};
use policy::ports::PolicyStoreError;
use shared_kernel::error::AppError;

use crate::api_error::ApiError;
use crate::api_response::ApiResponse;
use crate::platform_authorization::authorize_permission;
use crate::state::{AdminServices, AppState};
use public_api_contracts as contracts;

// Permission keys (crates/policy/src/catalog.rs). Kept as constants so the
// handler→permission mapping stays one auditable table.
const PERMISSION_IDENTITY_READ: &str = "identity.read";
// `identity.user.manage` (user enable/disable) is in the versioned catalog
// but gates no route yet; user-status mutations arrive with their own
// follow-up surface and must be wired here when they do.
const PERMISSION_IDENTITY_MEMBERSHIP_UPDATE: &str = "identity.membership.update";
const PERMISSION_ORGANIZATION_READ: &str = "organization.read";
const PERMISSION_ORGANIZATION_MANAGE: &str = "organization.manage";
const PERMISSION_POLICY_ROLE_READ: &str = "policy.role.read";
const PERMISSION_POLICY_ROLE_MANAGE: &str = "policy.role.manage";
const PERMISSION_POLICY_BINDING_READ: &str = "policy.binding.read";
const PERMISSION_POLICY_BINDING_MANAGE: &str = "policy.binding.manage";
const PERMISSION_POLICY_EXPLAIN: &str = "policy.explain";

/// Register the Stage 8 management routes on the protected router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/admin/users", get(list_users))
        .route("/api/v1/admin/users/{user_id}", get(get_user))
        .route(
            "/api/v1/admin/tenant-memberships",
            get(list_tenant_memberships).post(create_membership),
        )
        .route(
            "/api/v1/admin/tenant-memberships/{user_id}/suspend",
            axum::routing::post(suspend_membership),
        )
        .route(
            "/api/v1/admin/tenant-memberships/{user_id}/reactivate",
            axum::routing::post(reactivate_membership),
        )
        .route("/api/v1/admin/permissions", get(list_permissions))
        .route("/api/v1/admin/roles", get(list_roles).post(create_role))
        .route(
            "/api/v1/admin/roles/{role_id}",
            get(get_role).patch(update_role),
        )
        .route(
            "/api/v1/admin/roles/{role_id}/permissions",
            put(set_role_permissions),
        )
        .route(
            "/api/v1/admin/role-bindings",
            get(list_bindings).post(create_binding),
        )
        .route(
            "/api/v1/admin/role-bindings/{binding_id}/revoke",
            axum::routing::post(revoke_binding),
        )
        .route(
            "/api/v1/admin/organization-units",
            get(list_units).post(create_unit),
        )
        .route(
            "/api/v1/admin/organization-units/{unit_id}",
            patch(update_unit),
        )
        .route(
            "/api/v1/admin/organization-units/{unit_id}/move",
            axum::routing::post(move_unit),
        )
        .route(
            "/api/v1/admin/organization-units/{unit_id}/members",
            get(list_unit_members),
        )
        .route(
            "/api/v1/admin/organization-units/{unit_id}/members/{user_id}",
            axum::routing::post(add_member).delete(remove_member),
        )
        .route(
            "/api/v1/admin/authorization/explain",
            axum::routing::post(explain_decision),
        )
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageParams {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingListParams {
    #[serde(default)]
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveMemberParams {
    /// Membership kind to detach (`member` | `leader`, default `member`).
    #[serde(default)]
    pub membership_type: Option<String>,
    /// Optimistic version of the membership row.
    pub expected_version: i64,
}

fn default_limit() -> u32 {
    50
}

fn admin_services(state: &AppState) -> Result<&AdminServices, ApiError> {
    state.admin.as_ref().ok_or_else(|| {
        ApiError::from(AppError::ExternalService {
            service: "iam management".to_string(),
            message: "management plane is unavailable".to_string(),
        })
    })
}

fn conflict(message: &str) -> ApiError {
    ApiError::from(AppError::Conflict(message.to_string()))
}

/// Extract the mandatory `Idempotency-Key` header for a write route.
fn require_idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| ApiError::validation("Idempotency-Key header is required"))
}

/// Versioned opaque form of the identity keyset cursor. The wire token is
/// base64url over a versioned JSON payload; raw database fields never
/// appear in it directly.
#[derive(Serialize, Deserialize)]
struct CursorPayload {
    v: u8,
    ts: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(cursor: Option<IdentityPageCursor>) -> Result<Option<String>, ApiError> {
    cursor
        .map(|position| {
            let bytes = serde_json::to_vec(&CursorPayload {
                v: 1,
                ts: position.timestamp,
                id: position.row_id,
            })
            .map_err(|_| {
                ApiError::from(AppError::Internal("cursor encoding failed".to_string()))
            })?;
            Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
        })
        .transpose()
}

fn decode_cursor(raw: Option<String>) -> Result<Option<IdentityPageCursor>, ApiError> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| ApiError::validation("invalid cursor"))?;
    let payload: CursorPayload =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::validation("invalid cursor"))?;
    if payload.v != 1 {
        return Err(ApiError::validation("unsupported cursor version"));
    }
    Ok(Some(IdentityPageCursor {
        timestamp: payload.ts,
        row_id: payload.id,
    }))
}

// -- error mapping (stable public codes only, never internal error text) ----

fn map_identity_query_error(error: IdentityQueryError) -> ApiError {
    match error {
        IdentityQueryError::Validation(message) => ApiError::validation(message),
        IdentityQueryError::Unavailable | IdentityQueryError::Failed => {
            ApiError::service_unavailable("identity")
        }
    }
}

fn map_create_membership_error(error: CreateTenantMembershipError, target_id: String) -> ApiError {
    match error {
        CreateTenantMembershipError::Validation(message) => ApiError::validation(message),
        CreateTenantMembershipError::AlreadyExists => {
            conflict("user already belongs to this tenant")
        }
        CreateTenantMembershipError::NotFound => ApiError::not_found("platform_user", target_id),
        CreateTenantMembershipError::IdempotencyConflict => {
            conflict("idempotency key was reused with different request content")
        }
        CreateTenantMembershipError::Unavailable | CreateTenantMembershipError::Failed => {
            ApiError::service_unavailable("identity")
        }
    }
}

fn map_membership_status_error(
    error: ChangeTenantMembershipStatusError,
    user_id: Uuid,
) -> ApiError {
    match error {
        ChangeTenantMembershipStatusError::Validation(message) => ApiError::validation(message),
        ChangeTenantMembershipStatusError::ForbiddenSelfReaction => {
            conflict("a user cannot reactivate their own suspended membership")
        }
        ChangeTenantMembershipStatusError::NotFound => {
            ApiError::not_found("tenant_membership", user_id)
        }
        ChangeTenantMembershipStatusError::VersionConflict => {
            conflict("membership version conflict")
        }
        ChangeTenantMembershipStatusError::IdempotencyConflict => {
            conflict("idempotency key was reused with different request content")
        }
        ChangeTenantMembershipStatusError::Unavailable
        | ChangeTenantMembershipStatusError::Failed => ApiError::service_unavailable("identity"),
    }
}

fn map_org_error(error: OrganizationApplicationError, resource: &str) -> ApiError {
    match error {
        OrganizationApplicationError::Validation(message) => ApiError::validation(message),
        OrganizationApplicationError::NotFound => ApiError::from(AppError::NotFound {
            resource: resource.to_string(),
            id: "requested".to_string(),
        }),
        OrganizationApplicationError::AlreadyExists => conflict("organization aggregate exists"),
        OrganizationApplicationError::VersionConflict => conflict("organization version conflict"),
        OrganizationApplicationError::InvalidParent => {
            ApiError::validation("invalid parent placement")
        }
        OrganizationApplicationError::Cycle => conflict("organization tree placement rejected"),
        OrganizationApplicationError::UnitDisabled => conflict("organization unit is disabled"),
        OrganizationApplicationError::NotTenantMember => {
            conflict("user is not an active tenant member")
        }
        OrganizationApplicationError::TooManyResources => {
            ApiError::validation("resource cap exceeded")
        }
        OrganizationApplicationError::IdempotencyConflict => {
            conflict("idempotency key was reused with different request content")
        }
        OrganizationApplicationError::Unavailable | OrganizationApplicationError::Failed => {
            ApiError::service_unavailable("organization")
        }
    }
}

fn map_policy_error(error: PolicyApplicationError, resource: &str) -> ApiError {
    match error {
        PolicyApplicationError::Validation(message) => ApiError::validation(message),
        PolicyApplicationError::NotFound => ApiError::from(AppError::NotFound {
            resource: resource.to_string(),
            id: "requested".to_string(),
        }),
        PolicyApplicationError::AlreadyExists => conflict("policy aggregate exists"),
        PolicyApplicationError::VersionConflict => conflict("policy version conflict"),
        PolicyApplicationError::RoleImmutable => conflict("system roles are immutable"),
        PolicyApplicationError::UnknownPermission => {
            ApiError::validation("permission key is not in the catalog")
        }
        PolicyApplicationError::TooManyPermissions
        | PolicyApplicationError::TooManyBindings
        | PolicyApplicationError::TooManyResources => ApiError::validation("policy bound exceeded"),
        PolicyApplicationError::IdempotencyConflict => {
            conflict("idempotency key was reused with different request content")
        }
        PolicyApplicationError::NotTenantMember => conflict("user is not an active tenant member"),
        PolicyApplicationError::SelfEscalationDenied => {
            conflict("self-escalation rejected by policy")
        }
        PolicyApplicationError::ScopeUnitUnavailable => {
            ApiError::validation("organization unit scope is unavailable in this tenant")
        }
        PolicyApplicationError::Unavailable | PolicyApplicationError::Failed => {
            ApiError::service_unavailable("policy")
        }
    }
}

fn map_policy_store_error(error: PolicyStoreError, resource: &str) -> ApiError {
    match error {
        PolicyStoreError::NotFound => ApiError::from(AppError::NotFound {
            resource: resource.to_string(),
            id: "requested".to_string(),
        }),
        PolicyStoreError::Unavailable | PolicyStoreError::Failed => {
            ApiError::service_unavailable("policy")
        }
        _ => ApiError::service_unavailable("policy"),
    }
}

// -- domain → DTO conversion (bounded views only) ---------------------------

fn user_view(record: &TenantUserRecord) -> contracts::AdminUser {
    contracts::AdminUser {
        user_id: record.user.user_id(),
        status: record.user.status().as_str().to_string(),
        created_at: record.user.created_at(),
        updated_at: record.user.updated_at(),
        version: record.user.version().value(),
    }
}

fn membership_view(membership: &identity::domain::TenantMembership) -> contracts::MembershipView {
    contracts::MembershipView {
        membership_id: membership.membership_id(),
        user_id: membership.user_id(),
        status: membership.status().as_str().to_string(),
        joined_at: membership.joined_at(),
        suspended_at: membership.suspended_at(),
        source: membership.source().as_str().to_string(),
        version: membership.version().value(),
    }
}

fn membership_row_view(record: &MembershipRecord) -> contracts::MembershipView {
    membership_view(&record.membership)
}

fn permission_view(permission: &PermissionDefinition) -> contracts::PermissionView {
    contracts::PermissionView {
        key: permission.key().as_str().to_string(),
        description: permission.description().to_string(),
        reserved: permission.is_reserved(),
        active: permission.is_active(),
    }
}

fn role_view(role: &RoleDefinition, permission_keys: Vec<String>) -> contracts::RoleView {
    contracts::RoleView {
        role_id: role.role_id(),
        tenant_id: role.tenant_id(),
        stable_key: role.stable_key().to_string(),
        display_name: role.display_name().to_string(),
        status: role.status().as_str().to_string(),
        system: role.is_system(),
        permission_keys,
        created_at: role.created_at(),
        updated_at: role.updated_at(),
        version: role.version().value(),
    }
}

fn scope_view(scope: &ResourceScope) -> contracts::ScopeView {
    match scope {
        ResourceScope::Tenant => contracts::ScopeView::Tenant,
        ResourceScope::OrganizationUnit {
            org_unit_id,
            include_subtree,
        } => contracts::ScopeView::OrgUnit {
            org_unit_id: *org_unit_id,
            include_subtree: *include_subtree,
        },
        ResourceScope::ResourceType { kind } => contracts::ScopeView::ResourceType {
            resource_kind: kind.clone(),
        },
        ResourceScope::Resource { kind, resource_id } => contracts::ScopeView::Resource {
            resource_kind: kind.clone(),
            resource_id: *resource_id,
        },
    }
}

/// Delivery-side conversion of the bounded public scope DTO onto the policy
/// domain constructor (grammar/nil validation runs inside the domain).
fn scope_domain(scope: contracts::ScopeView) -> Result<ResourceScope, ApiError> {
    Ok(match scope {
        contracts::ScopeView::Tenant => ResourceScope::Tenant,
        contracts::ScopeView::OrgUnit {
            org_unit_id,
            include_subtree,
        } => ResourceScope::organization_unit(org_unit_id, include_subtree)
            .map_err(|_| ApiError::validation("invalid organization-unit scope"))?,
        contracts::ScopeView::ResourceType { resource_kind } => {
            ResourceScope::resource_type(resource_kind)
                .map_err(|_| ApiError::validation("invalid resource-kind scope"))?
        }
        contracts::ScopeView::Resource {
            resource_kind,
            resource_id,
        } => ResourceScope::resource(resource_kind, resource_id)
            .map_err(|_| ApiError::validation("invalid resource scope"))?,
    })
}

fn binding_view(binding: &RoleBinding) -> contracts::RoleBindingView {
    contracts::RoleBindingView {
        binding_id: binding.binding_id(),
        user_id: binding.user_id(),
        role_id: binding.role_id(),
        scope: scope_view(binding.scope()),
        status: binding.status().as_str().to_string(),
        effective_at: binding.effective_at(),
        expires_at: binding.expires_at(),
        created_at: binding.created_at(),
        updated_at: binding.updated_at(),
        version: binding.version().value(),
    }
}

fn unit_view(unit: &organization::domain::OrganizationUnit) -> contracts::OrganizationUnitView {
    contracts::OrganizationUnitView {
        unit_id: unit.unit_id(),
        parent_id: unit.parent_id(),
        unit_type: unit.unit_type().as_str().to_string(),
        name: unit.name().to_string(),
        status: unit.status().as_str().to_string(),
        created_at: unit.created_at(),
        updated_at: unit.updated_at(),
        version: unit.version().value(),
    }
}

fn member_view(
    membership: &organization::domain::OrganizationMembership,
) -> contracts::OrganizationMemberView {
    contracts::OrganizationMemberView {
        membership_id: membership.membership_id(),
        user_id: membership.user_id(),
        unit_id: membership.unit_id(),
        membership_type: membership.membership_type().as_str().to_string(),
        status: membership.status().as_str().to_string(),
        joined_at: membership.joined_at(),
        deactivated_at: membership.deactivated_at(),
        version: membership.version().value(),
    }
}

fn explain_view(decision: &PolicyDecision) -> contracts::ExplainView {
    contracts::ExplainView {
        allowed: decision.allowed,
        reason: decision.reason.as_str().to_string(),
        matched_binding: decision.matched_binding,
        matched_permission: decision.matched_permission.clone(),
        matched_scope: decision.matched_scope.as_ref().map(scope_view),
        policy_reference: decision.policy_reference.clone(),
        evaluations: decision
            .evaluations
            .iter()
            .map(|evaluation| contracts::ExplainEvaluation {
                binding_id: evaluation.binding_id,
                role_id: evaluation.role_id,
                binding_active: evaluation.binding_active,
                within_validity: evaluation.within_validity,
                role_grants_permission: evaluation.role_grants_permission,
                outcome: evaluation.outcome.as_str().to_string(),
            })
            .collect(),
    }
}

// -- bounded enum parsing (request strings → domain, 400 on unknown) --------

fn parse_unit_type(raw: &str) -> Result<OrganizationUnitType, ApiError> {
    match raw.trim() {
        "company" => Ok(OrganizationUnitType::Company),
        "department" => Ok(OrganizationUnitType::Department),
        "team" => Ok(OrganizationUnitType::Team),
        _ => Err(ApiError::validation("invalid unit_type")),
    }
}

fn parse_unit_status(raw: &str) -> Result<OrganizationUnitStatus, ApiError> {
    match raw.trim() {
        "active" => Ok(OrganizationUnitStatus::Active),
        "disabled" => Ok(OrganizationUnitStatus::Disabled),
        _ => Err(ApiError::validation("invalid status")),
    }
}

fn parse_role_status(raw: &str) -> Result<RoleStatus, ApiError> {
    match raw.trim() {
        "active" => Ok(RoleStatus::Active),
        "disabled" => Ok(RoleStatus::Disabled),
        _ => Err(ApiError::validation("invalid status")),
    }
}

fn parse_membership_type(raw: &str) -> Result<OrganizationMembershipType, ApiError> {
    match raw.trim() {
        "member" => Ok(OrganizationMembershipType::Member),
        "leader" => Ok(OrganizationMembershipType::Leader),
        _ => Err(ApiError::validation("invalid membership_type")),
    }
}

// ---------------------------------------------------------------------------
// Identity reads (`identity.read`)
// ---------------------------------------------------------------------------

pub async fn list_users(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    Query(query): Query<PageParams>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_IDENTITY_READ).await?;
    let admin = admin_services(&state)?;
    let after = decode_cursor(query.cursor)?;
    let page = admin
        .list_users
        .execute(authz.tenant_id, query.limit, after)
        .await
        .map_err(map_identity_query_error)?;
    let response = contracts::Page {
        items: page.items.iter().map(user_view).collect(),
        next_cursor: encode_cursor(page.next_cursor)?,
    };
    Ok((StatusCode::OK, Json(ApiResponse::ok(response))).into_response())
}

pub async fn get_user(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_IDENTITY_READ).await?;
    let admin = admin_services(&state)?;
    let record = admin
        .get_user
        .execute(authz.tenant_id, user_id)
        .await
        .map_err(map_identity_query_error)?
        .ok_or_else(|| ApiError::not_found("platform_user", user_id))?;
    Ok((StatusCode::OK, Json(ApiResponse::ok(user_view(&record)))).into_response())
}

pub async fn list_tenant_memberships(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    Query(query): Query<PageParams>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_IDENTITY_READ).await?;
    let admin = admin_services(&state)?;
    let after = decode_cursor(query.cursor)?;
    let page = admin
        .list_memberships
        .execute(authz.tenant_id, query.limit, after)
        .await
        .map_err(map_identity_query_error)?;
    let response = contracts::Page {
        items: page.items.iter().map(membership_row_view).collect(),
        next_cursor: encode_cursor(page.next_cursor)?,
    };
    Ok((StatusCode::OK, Json(ApiResponse::ok(response))).into_response())
}

// ---------------------------------------------------------------------------
// Identity writes
// ---------------------------------------------------------------------------

pub async fn create_membership(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<contracts::CreateTenantMembershipRequest>,
) -> Result<Response, ApiError> {
    // Catalog contract: `identity.membership.update` governs "create,
    // suspend, and reactivate tenant memberships"; membership creation must
    // not ride the user-manage key (reviewer finding, Stage 8).
    authorize_permission(&state, &authz, PERMISSION_IDENTITY_MEMBERSHIP_UPDATE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let (target, target_id) = match (body.user_id, body.issuer, body.subject) {
        (Some(user_id), None, None) => {
            (CreateMembershipTarget::UserId(user_id), user_id.to_string())
        }
        (None, Some(issuer), Some(subject)) => (
            CreateMembershipTarget::ExternalSubject { issuer, subject },
            "external-subject".to_string(),
        ),
        _ => {
            return Err(ApiError::validation(
                "provide exactly one of user_id or issuer+subject",
            ))
        }
    };
    let outcome = admin
        .create_membership
        .execute(CreateTenantMembershipCommand {
            tenant_id: authz.tenant_id,
            target,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: body.reason,
            source: MembershipSource::Admin,
        })
        .await
        .map_err(|error| map_create_membership_error(error, target_id))?;
    let status = if outcome.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ApiResponse::ok(membership_view(&outcome.membership))),
    )
        .into_response())
}

async fn change_membership_status(
    state: &Arc<AppState>,
    authz: &AuthorizationContext,
    headers: &HeaderMap,
    user_id: Uuid,
    target_status: MembershipStatus,
    expected_version: i64,
) -> Result<Response, ApiError> {
    authorize_permission(state, authz, PERMISSION_IDENTITY_MEMBERSHIP_UPDATE).await?;
    let idempotency_key = require_idempotency_key(headers)?;
    let admin = admin_services(state)?;
    let outcome = admin
        .change_membership_status
        .execute(ChangeTenantMembershipStatusCommand {
            tenant_id: authz.tenant_id,
            user_id,
            target_status,
            expected_version,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_membership_status_error(error, user_id))?;
    // Status change is a mutation of an existing membership: success is
    // always 200 (replay is indistinguishable, same final state).
    let _ = outcome.replayed;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(membership_view(&outcome.membership))),
    )
        .into_response())
}

pub async fn suspend_membership(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<contracts::MembershipStatusChangeRequest>,
) -> Result<Response, ApiError> {
    change_membership_status(
        &state,
        &authz,
        &headers,
        user_id,
        MembershipStatus::Suspended,
        body.expected_version,
    )
    .await
}

pub async fn reactivate_membership(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<contracts::MembershipStatusChangeRequest>,
) -> Result<Response, ApiError> {
    change_membership_status(
        &state,
        &authz,
        &headers,
        user_id,
        MembershipStatus::Active,
        body.expected_version,
    )
    .await
}

// ---------------------------------------------------------------------------
// Policy reads (`policy.role.read` / `policy.binding.read`)
// ---------------------------------------------------------------------------

pub async fn list_permissions(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_ROLE_READ).await?;
    let admin = admin_services(&state)?;
    let permissions = admin
        .policy_query
        .list_permissions()
        .await
        .map_err(|error| map_policy_store_error(error, "permission"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(
            permissions.iter().map(permission_view).collect::<Vec<_>>(),
        )),
    )
        .into_response())
}

pub async fn list_roles(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_ROLE_READ).await?;
    let admin = admin_services(&state)?;
    let roles = admin
        .policy_query
        .list_roles(authz.tenant_id)
        .await
        .map_err(|error| map_policy_store_error(error, "role"))?;
    let mut views = Vec::with_capacity(roles.len());
    for role in &roles {
        let permission_keys = admin
            .policy_query
            .get_role_permissions(authz.tenant_id, role.role_id())
            .await
            .map_err(|error| map_policy_store_error(error, "role"))?;
        views.push(role_view(role, permission_keys));
    }
    Ok((StatusCode::OK, Json(ApiResponse::ok(views))).into_response())
}

pub async fn get_role(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    Path(role_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_ROLE_READ).await?;
    let admin = admin_services(&state)?;
    let role = admin
        .policy_query
        .get_role(authz.tenant_id, role_id)
        .await
        .map_err(|error| map_policy_store_error(error, "role"))?
        .ok_or_else(|| ApiError::not_found("role", role_id))?;
    let permission_keys = admin
        .policy_query
        .get_role_permissions(authz.tenant_id, role_id)
        .await
        .map_err(|error| map_policy_store_error(error, "role"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(role_view(&role, permission_keys))),
    )
        .into_response())
}

pub async fn list_bindings(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    Query(query): Query<BindingListParams>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_BINDING_READ).await?;
    let admin = admin_services(&state)?;
    let bindings = admin
        .policy_query
        .list_bindings(authz.tenant_id, query.user_id)
        .await
        .map_err(|error| map_policy_store_error(error, "binding"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(
            bindings.iter().map(binding_view).collect::<Vec<_>>(),
        )),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Organization reads (`organization.read`)
// ---------------------------------------------------------------------------

pub async fn list_units(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_ORGANIZATION_READ).await?;
    let admin = admin_services(&state)?;
    let units = admin
        .list_org_units
        .execute(authz.tenant_id)
        .await
        .map_err(|error| map_org_error(error, "organization_unit"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(
            units.iter().map(unit_view).collect::<Vec<_>>(),
        )),
    )
        .into_response())
}

pub async fn list_unit_members(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    Path(unit_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_ORGANIZATION_READ).await?;
    let admin = admin_services(&state)?;
    let members = admin
        .list_org_members
        .execute(authz.tenant_id, unit_id)
        .await
        .map_err(|error| map_org_error(error, "organization_unit"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(
            members
                .iter()
                .map(|m| member_view(&m.membership))
                .collect::<Vec<_>>(),
        )),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Organization writes (`organization.manage`)
// ---------------------------------------------------------------------------

pub async fn create_unit(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<contracts::CreateUnitRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_ORGANIZATION_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .create_unit
        .execute(CreateOrganizationUnitCommand {
            tenant_id: authz.tenant_id,
            unit_id: None,
            parent_id: body.parent_id,
            unit_type: parse_unit_type(&body.unit_type)?,
            name: body.name,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_org_error(error, "organization_unit"))?;
    let status = if outcome.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(ApiResponse::ok(unit_view(&outcome.unit)))).into_response())
}

pub async fn update_unit(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(unit_id): Path<Uuid>,
    Json(body): Json<contracts::UpdateUnitRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_ORGANIZATION_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .update_unit
        .execute(UpdateOrganizationUnitCommand {
            tenant_id: authz.tenant_id,
            unit_id,
            name: body.name,
            unit_type: None,
            status: body.status.as_deref().map(parse_unit_status).transpose()?,
            expected_version: body.expected_version,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_org_error(error, "organization_unit"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(unit_view(&outcome.unit))),
    )
        .into_response())
}

pub async fn move_unit(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(unit_id): Path<Uuid>,
    Json(body): Json<contracts::MoveUnitRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_ORGANIZATION_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .move_unit
        .execute(MoveOrganizationUnitCommand {
            tenant_id: authz.tenant_id,
            unit_id,
            new_parent_id: body.new_parent_id,
            expected_version: body.expected_version,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_org_error(error, "organization_unit"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(unit_view(&outcome.unit))),
    )
        .into_response())
}

pub async fn add_member(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((unit_id, user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<contracts::AddUnitMemberRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_ORGANIZATION_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .add_org_member
        .execute(AddOrganizationMemberCommand {
            tenant_id: authz.tenant_id,
            unit_id,
            user_id,
            membership_type: parse_membership_type(&body.membership_type)?,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_org_error(error, "organization_membership"))?;
    let status = if outcome.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ApiResponse::ok(member_view(&outcome.membership))),
    )
        .into_response())
}

pub async fn remove_member(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((unit_id, user_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<RemoveMemberParams>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_ORGANIZATION_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .remove_org_member
        .execute(RemoveOrganizationMemberCommand {
            tenant_id: authz.tenant_id,
            unit_id,
            user_id,
            membership_type: parse_membership_type(
                query.membership_type.as_deref().unwrap_or("member"),
            )?,
            expected_version: query.expected_version,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_org_error(error, "organization_membership"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(member_view(&outcome.membership))),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Policy writes (`policy.role.manage` / `policy.binding.manage`)
// ---------------------------------------------------------------------------

pub async fn create_role(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<contracts::CreateRoleRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_ROLE_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let created = admin
        .create_role
        .execute(CreateRoleCommand {
            tenant_id: authz.tenant_id,
            role_id: None,
            stable_key: body.stable_key,
            display_name: body.display_name,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key.clone()),
            reason: None,
        })
        .await
        .map_err(|error| map_policy_error(error, "role"))?;
    // Optional initial grant: the bounded set-replace use case, keyed
    // deterministically off the create key so replay of the same request
    // converges instead of double-applying.
    if !body.permission_keys.is_empty() {
        admin
            .set_role_permissions
            .execute(SetRolePermissionsCommand {
                tenant_id: authz.tenant_id,
                role_id: created.role.role_id(),
                permission_keys: body.permission_keys,
                expected_version: created.role.version().value(),
                actor_user_id: authz.user_id,
                idempotency_key: Some(format!("{idempotency_key}|initial-permissions")),
                reason: None,
            })
            .await
            .map_err(|error| map_policy_error(error, "role"))?;
    }
    let permission_keys = admin
        .policy_query
        .get_role_permissions(authz.tenant_id, created.role.role_id())
        .await
        .map_err(|error| map_policy_store_error(error, "role"))?;
    let status = if created.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ApiResponse::ok(role_view(&created.role, permission_keys))),
    )
        .into_response())
}

pub async fn update_role(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(role_id): Path<Uuid>,
    Json(body): Json<contracts::UpdateRoleRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_ROLE_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .update_role
        .execute(UpdateRoleCommand {
            tenant_id: authz.tenant_id,
            role_id,
            display_name: body.display_name,
            status: body.status.as_deref().map(parse_role_status).transpose()?,
            expected_version: body.expected_version,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_policy_error(error, "role"))?;
    let permission_keys = admin
        .policy_query
        .get_role_permissions(authz.tenant_id, role_id)
        .await
        .map_err(|error| map_policy_store_error(error, "role"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(role_view(&outcome.role, permission_keys))),
    )
        .into_response())
}

pub async fn set_role_permissions(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(role_id): Path<Uuid>,
    Json(body): Json<contracts::SetRolePermissionsRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_ROLE_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .set_role_permissions
        .execute(SetRolePermissionsCommand {
            tenant_id: authz.tenant_id,
            role_id,
            permission_keys: body.permission_keys,
            expected_version: body.expected_version,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_policy_error(error, "role"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(role_view(
            &outcome.role,
            outcome.permission_keys,
        ))),
    )
        .into_response())
}

pub async fn create_binding(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<contracts::BindingCreateRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_BINDING_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .bind_role
        .execute(BindRoleCommand {
            tenant_id: authz.tenant_id,
            binding_id: None,
            user_id: body.user_id,
            role_id: body.role_id,
            scope: scope_domain(body.scope)?,
            effective_at: body.effective_at.unwrap_or_else(Utc::now),
            expires_at: body.expires_at,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_policy_error(error, "binding"))?;
    let status = if outcome.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ApiResponse::ok(binding_view(&outcome.binding))),
    )
        .into_response())
}

pub async fn revoke_binding(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(binding_id): Path<Uuid>,
    Json(body): Json<contracts::BindingRevokeRequest>,
) -> Result<Response, ApiError> {
    authorize_permission(&state, &authz, PERMISSION_POLICY_BINDING_MANAGE).await?;
    let idempotency_key = require_idempotency_key(&headers)?;
    let admin = admin_services(&state)?;
    let outcome = admin
        .revoke_binding
        .execute(RevokeRoleBindingCommand {
            tenant_id: authz.tenant_id,
            binding_id,
            expected_version: body.expected_version,
            actor_user_id: authz.user_id,
            idempotency_key: Some(idempotency_key),
            reason: None,
        })
        .await
        .map_err(|error| map_policy_error(error, "binding"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(binding_view(&outcome.binding))),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Policy explain (`policy.explain`)
// ---------------------------------------------------------------------------

pub async fn explain_decision(
    axum::Extension(authz): axum::Extension<AuthorizationContext>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<contracts::ExplainRequest>,
) -> Result<Response, ApiError> {
    // The caller needs `policy.explain`; the decision evaluated is the
    // target user's (built from the request tenant boundary + body user id
    // with no compat grants — explain reports the RoleBinding path).
    authorize_permission(&state, &authz, PERMISSION_POLICY_EXPLAIN).await?;
    let admin = admin_services(&state)?;
    let target = AuthorizationContext::new(body.user_id, authz.tenant_id);
    let resource = body
        .resource
        .as_ref()
        .map(|resource| policy::application::ResourceTarget {
            kind: resource.kind.clone(),
            resource_id: resource.resource_id,
            org_unit_id: resource.org_unit_id,
        });
    let decision = admin
        .explain
        .explain(&target, &body.permission, resource.as_ref())
        .await
        .map_err(|error| map_policy_error(error, "permission"))?;
    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(explain_view(&decision))),
    )
        .into_response())
}
