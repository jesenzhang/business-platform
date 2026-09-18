//! PLAN-0013 Stage 8: IAM management REST surface tests.
//!
//! Exercises the composed router (auth + platform authorization + admin
//! handlers) end to end against the in-memory fakes, with no socket and no
//! database. Authorization follows the `RoleBinding` path exclusively (the
//! compat bridge flag is off), which is the production semantics for every
//! IAM permission key.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use business_api::auth::AuthMiddlewareConfig;
use business_api::config::{
    AuthConfig, BusinessApiConfig, DatabaseBackend, DatabaseConfig, ObservabilityConfig,
    ServerConfig, StorageConfig,
};
use business_api::platform_authorization::{
    IdentitySubjectStatusBridge, IdentityTenantMembershipBridge, OrganizationScopeBridge,
};
use business_api::routes::create_router;
use business_api::state::{
    AccessServices, AdminServices, AppState, DocumentServices, ReadinessProbe, ReadinessReport,
    ReadinessStatus,
};
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use document::query::{
    DocumentDetailQuery, DocumentDetailView, DocumentListFilter, DocumentListPage,
    DocumentListQuery, DocumentListRequest, QueryError,
};
use identity::application::{
    ChangeTenantMembershipStatus, CreateTenantMembership, GetTenantUser, ListMemberships,
    ListTenantUsers, ResolveAuthenticatedUser, TenantAccessChecker,
};
use identity::domain::{MembershipSource, PlatformUser, TenantMembership};
use identity::testing::FakeIdentityStores;
use organization::application::{
    AddOrganizationMember, CreateOrganizationUnit, ListOrganizationTree, ListUnitMembers,
    MoveOrganizationUnit, RemoveOrganizationMember, UpdateOrganizationUnit,
};
use organization::domain::{OrganizationUnit, OrganizationUnitType};
use organization::testing::FakeOrganizationPorts;
use policy::application::{
    Authorize, BindRole, CreateRole, ExplainDecision, RevokeRoleBinding, SetRolePermissions,
    UpdateRole,
};
use policy::domain::{ResourceScope, RoleBinding, RoleDefinition, ValidityWindow};
use policy::testing::FakePolicyPorts;
use runtime_config::{RuntimeEnvironment, Secret, SecretUrl};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

const DEV_SECRET: &str = "test-dev-secret";
const DEV_TENANT_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001);
const DEV_USER_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0002);
const DEV_TENANT_HEADER: &str = "00000000-0000-0000-0000-000000000001";
const DEV_USER_HEADER: &str = "00000000-0000-0000-0000-000000000002";
/// Seeded Active platform user with an Active tenant membership: the
/// standard target of membership/unit/binding mutations.
const TARGET_USER_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0003);
/// Seeded Active platform user without any tenant membership.
const FRESH_USER_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0004);
/// A different tenant, for cross-tenant isolation probes.
const OTHER_TENANT_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_00ff);

/// Role id pre-seeded inside every granted matrix kit.
const FIXTURE_ROLE_ID: Uuid = Uuid::from_u128(0xbb);
/// Second role: target of the pre-seeded revocable binding.
const FIXTURE_ROLE_B_ID: Uuid = Uuid::from_u128(0xbe);
/// Binding id pre-seeded (revocable) inside every granted matrix kit.
const FIXTURE_BINDING_ID: Uuid = Uuid::from_u128(0xcc);
/// Root unit pre-seeded inside every granted matrix kit.
const FIXTURE_ROOT_UNIT_ID: Uuid = Uuid::from_u128(0xa9);
/// Child unit (under the fixture root) pre-seeded in every granted kit.
const FIXTURE_UNIT_ID: Uuid = Uuid::from_u128(0xaa);

// Matrix URIs with embedded fixture ids (literals; `concat!` cannot see
// consts). The Uuid consts above must stay in sync with these strings.
const GET_USER_URI: &str = "/api/v1/admin/users/00000000-0000-0000-0000-000000000003";
const SUSPEND_URI: &str =
    "/api/v1/admin/tenant-memberships/00000000-0000-0000-0000-000000000003/suspend";
const REACTIVATE_URI: &str =
    "/api/v1/admin/tenant-memberships/00000000-0000-0000-0000-000000000003/reactivate";
const ROLE_URI: &str = "/api/v1/admin/roles/00000000-0000-0000-0000-0000000000bb";
const ROLE_PERMISSIONS_URI: &str =
    "/api/v1/admin/roles/00000000-0000-0000-0000-0000000000bb/permissions";
const LIST_BINDINGS_FOR_TARGET_URI: &str =
    "/api/v1/admin/role-bindings?user_id=00000000-0000-0000-0000-000000000003";
const REVOKE_BINDING_URI: &str =
    "/api/v1/admin/role-bindings/00000000-0000-0000-0000-0000000000cc/revoke";
const UNIT_URI: &str = "/api/v1/admin/organization-units/00000000-0000-0000-0000-0000000000aa";
const UNIT_MEMBERS_URI: &str =
    "/api/v1/admin/organization-units/00000000-0000-0000-0000-0000000000aa/members";
const MOVE_UNIT_URI: &str =
    "/api/v1/admin/organization-units/00000000-0000-0000-0000-0000000000aa/move";
const UNIT_MEMBER_URI: &str =
    "/api/v1/admin/organization-units/00000000-0000-0000-0000-0000000000aa/members/00000000-0000-0000-0000-000000000003";
const REMOVE_MEMBER_URI: &str =
    "/api/v1/admin/organization-units/00000000-0000-0000-0000-0000000000aa/members/00000000-0000-0000-0000-000000000003?expected_version=1";

struct EmptyPorts;

#[async_trait]
impl CreateDocumentUnitOfWork for EmptyPorts {
    async fn execute(
        &self,
        _command: PersistNewDocument,
    ) -> Result<CreateDocumentResult, ApplicationPortError> {
        Err(ApplicationPortError::Unavailable)
    }
}

#[async_trait]
impl DocumentDetailQuery for EmptyPorts {
    async fn execute(
        &self,
        _tenant_id: Uuid,
        _document_id: Uuid,
    ) -> Result<Option<DocumentDetailView>, QueryError> {
        Ok(None)
    }
}

#[async_trait]
impl DocumentListQuery for EmptyPorts {
    async fn execute(&self, _request: DocumentListRequest) -> Result<DocumentListPage, QueryError> {
        Ok(DocumentListPage {
            items: Vec::new(),
            next_cursor: None,
        })
    }

    async fn count(
        &self,
        _tenant_id: Uuid,
        _filter: DocumentListFilter,
    ) -> Result<u64, QueryError> {
        Ok(0)
    }
}

#[async_trait]
impl ReadinessProbe for EmptyPorts {
    async fn check(&self) -> ReadinessReport {
        ReadinessReport {
            status: ReadinessStatus::NotReady,
            database: "unavailable",
            migrations: "unknown",
        }
    }
}

fn test_config() -> BusinessApiConfig {
    BusinessApiConfig {
        env: RuntimeEnvironment::Development,
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            request_timeout_secs: 30,
            cors_origins: vec!["*".to_string()],
            body_limit_bytes: 64 * 1024,
        },
        database: DatabaseConfig {
            backend: DatabaseBackend::Postgres,
            url: SecretUrl::parse("postgres://user:pass@localhost:5432/db")
                .expect("test URL should parse"),
            max_connections: 1,
            min_connections: 0,
            acquire_timeout_secs: 1,
        },
        observability: ObservabilityConfig {
            service_name: "business-api-test".to_string(),
            otlp_endpoint: None,
            log_level: "off".to_string(),
            log_format: "text".to_string(),
        },
        storage: StorageConfig::default(),
        auth: AuthConfig {
            issuer_url: String::new(),
            audience: None,
            jwks_url: None,
            dev_secret: Some(Secret::new(DEV_SECRET.to_string())),
            dev_auth_enabled: true,
            dev_permissions: BTreeSet::new(),
            dev_tenant_id: Some(DEV_TENANT_ID),
            dev_user_id: Some(DEV_USER_ID),
            dev_subject: Some("iam-admin-test-user".to_string()),
            dev_roles: BTreeSet::new(),
            management_permission_compat_enabled: true,
            bootstrap: business_api::config::BootstrapAdminConfig::default(),
        },
    }
}

/// Base fixture state shared by every kit: dev principal (Active user +
/// Active membership), an Active target member, and an Active platform user
/// who is not yet a member.
fn seed_base(stores: &FakeIdentityStores) {
    let now = chrono::Utc::now();
    stores.seed_user(PlatformUser::create(DEV_USER_ID, now).expect("dev user fixture"));
    stores.seed_membership(
        TenantMembership::join(
            Uuid::now_v7(),
            DEV_TENANT_ID,
            DEV_USER_ID,
            MembershipSource::Admin,
            now,
        )
        .expect("dev membership fixture"),
    );
    stores.seed_user(PlatformUser::create(TARGET_USER_ID, now).expect("target user fixture"));
    stores.seed_membership(
        TenantMembership::join(
            Uuid::now_v7(),
            DEV_TENANT_ID,
            TARGET_USER_ID,
            MembershipSource::Admin,
            now,
        )
        .expect("target membership fixture"),
    );
    stores.seed_user(PlatformUser::create(FRESH_USER_ID, now).expect("fresh user fixture"));
}

fn seed_role(policy: &FakePolicyPorts, role_id: Uuid, stable_key: &str, keys: &[&str]) {
    let now = chrono::Utc::now();
    let role =
        RoleDefinition::create_tenant_role(role_id, DEV_TENANT_ID, stable_key, "Fixture", now)
            .expect("role fixture");
    policy.seed_role(role, keys);
}

fn seed_unit(organization: &FakeOrganizationPorts, unit_id: Uuid, parent: Option<Uuid>) {
    let now = chrono::Utc::now();
    organization.seed_unit(
        OrganizationUnit::create(
            unit_id,
            DEV_TENANT_ID,
            parent,
            OrganizationUnitType::Department,
            "Fixture unit",
            now,
        )
        .expect("unit fixture"),
    );
}

fn seed_active_binding(policy: &FakePolicyPorts, binding_id: Uuid, user_id: Uuid, role_id: Uuid) {
    let now = chrono::Utc::now();
    policy.seed_binding(
        RoleBinding::create(
            binding_id,
            DEV_TENANT_ID,
            user_id,
            role_id,
            ResourceScope::Tenant,
            ValidityWindow {
                effective_at: now,
                expires_at: None,
            },
            now,
        )
        .expect("binding fixture"),
    );
}

/// Seed an active tenant role granting `keys` and bind it to `user_id`.
fn grant(policy: &FakePolicyPorts, user_id: Uuid, stable_key: &str, keys: &[&str]) {
    let now = chrono::Utc::now();
    let role = RoleDefinition::create_tenant_role(
        Uuid::now_v7(),
        DEV_TENANT_ID,
        stable_key,
        "Grant fixture",
        now,
    )
    .expect("grant role fixture");
    let role_id = role.role_id();
    policy.seed_role(role, keys);
    seed_active_binding(policy, Uuid::now_v7(), user_id, role_id);
}

/// Build the full router with `AccessServices` + `AdminServices` over the fakes.
/// `configure` runs after the base fixtures and before the router is built.
fn iam_router_with<F>(configure: F) -> axum::Router
where
    F: FnOnce(&FakeIdentityStores, &FakePolicyPorts, &FakeOrganizationPorts),
{
    let config = test_config();
    let ports = Arc::new(EmptyPorts);

    let stores = Arc::new(FakeIdentityStores::new());
    seed_base(&stores);
    let policy = FakePolicyPorts::new();
    let organization = FakeOrganizationPorts::new();
    configure(&stores, &policy, &organization);

    let subject: Arc<dyn policy::ports::SubjectStatusPort> =
        Arc::new(IdentitySubjectStatusBridge::new(Arc::new(
            TenantAccessChecker::new(Arc::clone(&stores.query)),
        )));
    let tenant_reader: Arc<dyn organization::ports::TenantMembershipReader> =
        Arc::new(IdentityTenantMembershipBridge::new(Arc::new(
            TenantAccessChecker::new(Arc::clone(&stores.query)),
        )));
    let org_scope: Arc<dyn policy::ports::OrganizationScopePort> = Arc::new(
        OrganizationScopeBridge::new(Arc::clone(&organization.query)),
    );

    let access = AccessServices {
        resolve: Arc::new(ResolveAuthenticatedUser::new(Arc::clone(&stores.resolve))),
        authorize: Arc::new(Authorize::new(
            Arc::clone(&policy.query),
            Arc::clone(&subject),
            Arc::clone(&policy.org),
        )),
        compat_enabled: false,
        oidc_issuer: String::new(),
    };
    let admin = AdminServices {
        list_users: Arc::new(ListTenantUsers::new(Arc::clone(&stores.query))),
        get_user: Arc::new(GetTenantUser::new(Arc::clone(&stores.query))),
        list_memberships: Arc::new(ListMemberships::new(Arc::clone(&stores.query))),
        create_membership: Arc::new(CreateTenantMembership::new(Arc::clone(&stores.command))),
        change_membership_status: Arc::new(ChangeTenantMembershipStatus::new(Arc::clone(
            &stores.command,
        ))),
        policy_query: Arc::clone(&policy.query),
        list_org_units: Arc::new(ListOrganizationTree::new(Arc::clone(&organization.query))),
        list_org_members: Arc::new(ListUnitMembers::new(Arc::clone(&organization.query))),
        create_unit: Arc::new(CreateOrganizationUnit::new(
            Arc::clone(&organization.command),
            Arc::clone(&organization.query),
        )),
        update_unit: Arc::new(UpdateOrganizationUnit::new(Arc::clone(
            &organization.command,
        ))),
        move_unit: Arc::new(MoveOrganizationUnit::new(
            Arc::clone(&organization.command),
            Arc::clone(&organization.query),
        )),
        add_org_member: Arc::new(AddOrganizationMember::new(
            Arc::clone(&organization.command),
            Arc::clone(&organization.query),
            tenant_reader,
        )),
        remove_org_member: Arc::new(RemoveOrganizationMember::new(Arc::clone(
            &organization.command,
        ))),
        create_role: Arc::new(CreateRole::new(
            Arc::clone(&policy.command),
            Arc::clone(&policy.query),
        )),
        update_role: Arc::new(UpdateRole::new(
            Arc::clone(&policy.command),
            Arc::clone(&policy.query),
        )),
        set_role_permissions: Arc::new(SetRolePermissions::new(
            Arc::clone(&policy.command),
            Arc::clone(&policy.query),
        )),
        bind_role: Arc::new(BindRole::new(
            Arc::clone(&policy.command),
            Arc::clone(&policy.query),
            Arc::clone(&subject),
            Arc::clone(&org_scope),
        )),
        revoke_binding: Arc::new(RevokeRoleBinding::new(Arc::clone(&policy.command))),
        explain: Arc::new(ExplainDecision::new(
            Arc::clone(&policy.query),
            Arc::clone(&subject),
            Arc::clone(&org_scope),
        )),
    };

    let state = Arc::new(AppState {
        documents: DocumentServices {
            create: Arc::new(document::application::CreateDocumentMetadata::new(
                ports.clone(),
            )),
            detail: ports.clone(),
            list: ports.clone(),
        },
        processing: None,
        governance: None,
        readiness: ports,
        storage: None,
        access: Some(access),
        admin: Some(admin),
    });
    let auth_config = AuthMiddlewareConfig {
        dev_auth_enabled: true,
        dev_secret: Some(DEV_SECRET.to_string()),
        dev_permissions: BTreeSet::new(),
        dev_tenant_id: Some(DEV_TENANT_ID),
        dev_user_id: Some(DEV_USER_ID),
        dev_subject: Some("iam-admin-test-user".to_string()),
        dev_roles: BTreeSet::new(),
        oidc: None,
    };
    create_router(state, auth_config, &config.server)
}

fn iam_router() -> axum::Router {
    iam_router_with(|_, _, _| {})
}

/// Granted-kit fixtures: the stable ids the matrix rows reference. The
/// revocable fixture binding points at a role the matrix never re-binds.
fn seed_matrix_fixtures(
    _stores: &FakeIdentityStores,
    policy: &FakePolicyPorts,
    organization: &FakeOrganizationPorts,
) {
    seed_role(policy, FIXTURE_ROLE_ID, "fixture-role", &["identity.read"]);
    seed_role(
        policy,
        FIXTURE_ROLE_B_ID,
        "fixture-role-b",
        &["identity.read"],
    );
    seed_active_binding(
        policy,
        FIXTURE_BINDING_ID,
        TARGET_USER_ID,
        FIXTURE_ROLE_B_ID,
    );
    seed_unit(organization, FIXTURE_ROOT_UNIT_ID, None);
    seed_unit(organization, FIXTURE_UNIT_ID, Some(FIXTURE_ROOT_UNIT_ID));
}

fn authorized(
    method: Method,
    uri: &str,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {DEV_SECRET}"))
        .header("x-tenant-id", DEV_TENANT_HEADER)
        .header("x-user-id", DEV_USER_HEADER);
    if let Some(key) = idempotency_key {
        builder = builder.header("idempotency-key", key);
    }
    match body {
        Some(value) => builder
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .expect("request must build"),
        None => builder.body(Body::empty()).expect("request must build"),
    }
}

fn get(uri: &str) -> Request<Body> {
    authorized(Method::GET, uri, None, None)
}

async fn call(router: axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.expect("router must respond");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body must be readable");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body must be JSON")
    };
    (status, value)
}

fn data(value: &Value) -> &Value {
    &value["data"]
}

fn array(value: &Value) -> Vec<&Value> {
    value
        .as_array()
        .expect("data must be an array")
        .iter()
        .collect()
}

// ---------------------------------------------------------------------------
// Authentication (401) and the full permission matrix (403 / allow).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unauthenticated_read_is_401() {
    let router = iam_router();
    let request = Request::builder()
        .uri("/api/v1/admin/users")
        .body(Body::empty())
        .expect("request must build");
    let (status, _) = call(router, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthenticated_write_is_401() {
    let router = iam_router();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/admin/roles")
        .header("idempotency-key", "k-401")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"stable_key": "x", "display_name": "X"}).to_string(),
        ))
        .expect("request must build");
    let (status, _) = call(router, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// One row per (permission, operation). `granted` false marks rows whose
/// success case needs a mutated fixture state (covered by dedicated tests
/// below); the deny probe still runs for them.
struct MatrixRow {
    permission: &'static str,
    method: Method,
    uri: &'static str,
    body: Value,
    granted: bool,
}

fn matrix() -> Vec<MatrixRow> {
    let row = |permission, method, uri, body, granted| MatrixRow {
        permission,
        method,
        uri,
        body,
        granted,
    };
    vec![
        row(
            "identity.read",
            Method::GET,
            "/api/v1/admin/users?limit=20",
            Value::Null,
            true,
        ),
        row(
            "identity.read",
            Method::GET,
            GET_USER_URI,
            Value::Null,
            true,
        ),
        row(
            "identity.read",
            Method::GET,
            "/api/v1/admin/tenant-memberships?limit=20",
            Value::Null,
            true,
        ),
        // Catalog contract: membership creation is governed by
        // `identity.membership.update`, NOT `identity.user.manage`.
        row(
            "identity.membership.update",
            Method::POST,
            "/api/v1/admin/tenant-memberships",
            json!({"user_id": FRESH_USER_ID}),
            true,
        ),
        row(
            "identity.membership.update",
            Method::POST,
            SUSPEND_URI,
            json!({"expected_version": 1}),
            true,
        ),
        // The granted reactivate probe needs a suspended fixture; the full
        // suspend -> reactivate cycle is covered by
        // membership_create_list_suspend_reactivate.
        row(
            "identity.membership.update",
            Method::POST,
            REACTIVATE_URI,
            json!({"expected_version": 1}),
            false,
        ),
        row(
            "policy.role.read",
            Method::GET,
            "/api/v1/admin/permissions",
            Value::Null,
            true,
        ),
        row(
            "policy.role.read",
            Method::GET,
            "/api/v1/admin/roles",
            Value::Null,
            true,
        ),
        row("policy.role.read", Method::GET, ROLE_URI, Value::Null, true),
        row(
            "policy.role.manage",
            Method::POST,
            "/api/v1/admin/roles",
            json!({"stable_key": "matrix-role", "display_name": "Matrix role"}),
            true,
        ),
        row(
            "policy.role.manage",
            Method::PATCH,
            ROLE_URI,
            json!({"display_name": "renamed", "expected_version": 1}),
            true,
        ),
        row(
            "policy.role.manage",
            Method::PUT,
            ROLE_PERMISSIONS_URI,
            json!({"permission_keys": ["identity.read"], "expected_version": 1}),
            true,
        ),
        row(
            "policy.binding.read",
            Method::GET,
            "/api/v1/admin/role-bindings",
            Value::Null,
            true,
        ),
        row(
            "policy.binding.read",
            Method::GET,
            LIST_BINDINGS_FOR_TARGET_URI,
            Value::Null,
            true,
        ),
        row(
            "policy.binding.manage",
            Method::POST,
            "/api/v1/admin/role-bindings",
            json!({"user_id": TARGET_USER_ID, "role_id": FIXTURE_ROLE_ID, "scope": {"scope": "tenant"}}),
            true,
        ),
        row(
            "policy.binding.manage",
            Method::POST,
            REVOKE_BINDING_URI,
            json!({"expected_version": 1}),
            true,
        ),
        row(
            "organization.read",
            Method::GET,
            "/api/v1/admin/organization-units",
            Value::Null,
            true,
        ),
        row(
            "organization.read",
            Method::GET,
            UNIT_MEMBERS_URI,
            Value::Null,
            true,
        ),
        row(
            "organization.manage",
            Method::POST,
            "/api/v1/admin/organization-units",
            json!({"unit_type": "department", "name": "Matrix"}),
            true,
        ),
        row(
            "organization.manage",
            Method::PATCH,
            UNIT_URI,
            json!({"name": "renamed", "expected_version": 1}),
            true,
        ),
        row(
            "organization.manage",
            Method::POST,
            MOVE_UNIT_URI,
            json!({"new_parent_id": Value::Null, "expected_version": 1}),
            true,
        ),
        row(
            "organization.manage",
            Method::POST,
            UNIT_MEMBER_URI,
            json!({"membership_type": "member"}),
            true,
        ),
        // The granted remove probe needs an existing unit membership;
        // covered by unit_member_add_list_remove.
        row(
            "organization.manage",
            Method::DELETE,
            REMOVE_MEMBER_URI,
            Value::Null,
            false,
        ),
        row(
            "policy.explain",
            Method::POST,
            "/api/v1/admin/authorization/explain",
            json!({"user_id": TARGET_USER_ID, "permission": "identity.read"}),
            true,
        ),
    ]
}

#[tokio::test]
async fn every_iam_route_denies_without_grant() {
    for row in matrix() {
        let router = iam_router();
        let key = format!("deny-{}", row.permission);
        let request = authorized(
            row.method.clone(),
            row.uri,
            Some(&key),
            (!row.body.is_null()).then(|| row.body.clone()),
        );
        let (status, _) = call(router, request).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "expected 403 for {} {} without {}",
            row.method,
            row.uri,
            row.permission
        );
    }
}

#[tokio::test]
async fn granted_permission_matrix_allows_every_surface() {
    for row in matrix() {
        if !row.granted {
            continue;
        }
        let permission = row.permission;
        let router = iam_router_with(move |stores, policy, organization| {
            seed_matrix_fixtures(stores, policy, organization);
            grant(policy, DEV_USER_ID, "granted-key", &[permission]);
        });
        let key = format!("grant-{}", row.permission);
        let request = authorized(
            row.method.clone(),
            row.uri,
            Some(&key),
            (!row.body.is_null()).then(|| row.body.clone()),
        );
        let (status, value) = call(router, request).await;
        assert!(
            status.is_success(),
            "expected success for {} {} with {permission}, got {status}: {value}",
            row.method,
            row.uri
        );
    }
}

// ---------------------------------------------------------------------------
// Happy paths.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_role_then_list_and_get() {
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "admins",
            &["policy.role.read", "policy.role.manage"],
        );
    });
    let (status, created) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/roles",
            Some("mk-create"),
            Some(json!({"stable_key": "reviewers", "display_name": "Reviewers", "permission_keys": ["identity.read"]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let role_id = data(&created)["role_id"]
        .as_str()
        .expect("role id")
        .to_string();
    assert_eq!(data(&created)["stable_key"], "reviewers");
    assert_eq!(data(&created)["permission_keys"], json!(["identity.read"]));

    let (status, listed) = call(router.clone(), get("/api/v1/admin/roles")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(array(data(&listed))
        .iter()
        .any(|role| role["role_id"] == json!(role_id.clone())));

    let (status, fetched) = call(router, get(&format!("/api/v1/admin/roles/{role_id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&fetched)["display_name"], "Reviewers");
    assert_eq!(data(&fetched)["system"], false);
}

#[tokio::test]
async fn bind_role_to_other_user_and_list_bindings() {
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "binders",
            &["policy.binding.read", "policy.binding.manage"],
        );
        seed_role(policy, FIXTURE_ROLE_ID, "approvers", &["identity.read"]);
    });
    let (status, binding) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/role-bindings",
            Some("mk-bind"),
            Some(json!({"user_id": TARGET_USER_ID, "role_id": FIXTURE_ROLE_ID, "scope": {"scope": "tenant"}})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(data(&binding)["user_id"], json!(TARGET_USER_ID));
    assert_eq!(data(&binding)["status"], "active");
    // Audit timestamps are part of the public binding view.
    assert!(data(&binding)["created_at"].is_string());
    assert!(data(&binding)["updated_at"].is_string());
    let binding_id = data(&binding)["binding_id"]
        .as_str()
        .expect("id")
        .to_string();

    let (status, listed) = call(
        router.clone(),
        get(&format!(
            "/api/v1/admin/role-bindings?user_id={TARGET_USER_ID}"
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(array(data(&listed)).len(), 1);

    let (status, revoked) = call(
        router,
        authorized(
            Method::POST,
            &format!("/api/v1/admin/role-bindings/{binding_id}/revoke"),
            Some("mk-revoke"),
            Some(json!({"expected_version": data(&binding)["version"]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&revoked)["status"], "revoked");
}

#[tokio::test]
async fn organization_unit_crud_and_move() {
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "org-admins",
            &["organization.read", "organization.manage"],
        );
    });
    let (status, root) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/organization-units",
            Some("mk-root"),
            Some(json!({"unit_type": "company", "name": "Acme"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let root_id = data(&root)["unit_id"]
        .as_str()
        .expect("root id")
        .to_string();

    let (status, child) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/organization-units",
            Some("mk-child"),
            Some(json!({"parent_id": root_id, "unit_type": "department", "name": "R&D"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let child_id = data(&child)["unit_id"]
        .as_str()
        .expect("child id")
        .to_string();

    let (status, units) = call(router.clone(), get("/api/v1/admin/organization-units")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(array(data(&units)).len(), 2);

    let (status, updated) = call(
        router.clone(),
        authorized(
            Method::PATCH,
            &format!("/api/v1/admin/organization-units/{child_id}"),
            Some("mk-update"),
            Some(json!({"name": "Research", "expected_version": data(&child)["version"]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&updated)["name"], "Research");

    let (status, moved) = call(
        router,
        authorized(
            Method::POST,
            &format!("/api/v1/admin/organization-units/{child_id}/move"),
            Some("mk-move"),
            Some(json!({"new_parent_id": Value::Null, "expected_version": data(&updated)["version"]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&moved)["parent_id"], Value::Null);
}

#[tokio::test]
async fn unit_member_add_list_remove() {
    let router = iam_router_with(|_, policy, organization| {
        grant(
            policy,
            DEV_USER_ID,
            "org-admins",
            &["organization.read", "organization.manage"],
        );
        seed_unit(organization, FIXTURE_UNIT_ID, None);
    });
    let unit_uri = format!("/api/v1/admin/organization-units/{FIXTURE_UNIT_ID}");
    let (status, added) = call(
        router.clone(),
        authorized(
            Method::POST,
            &format!("{unit_uri}/members/{TARGET_USER_ID}"),
            Some("mk-add"),
            Some(json!({"membership_type": "member"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(data(&added)["status"], "active");

    let (status, members) = call(router.clone(), get(&format!("{unit_uri}/members"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(array(data(&members)).len(), 1);

    let (status, removed) = call(
        router,
        authorized(
            Method::DELETE,
            &format!(
                "{unit_uri}/members/{TARGET_USER_ID}?membership_type=member&expected_version={}",
                data(&added)["version"]
            ),
            Some("mk-remove"),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&removed)["status"], "inactive");
}

#[tokio::test]
async fn user_manage_grant_cannot_create_memberships() {
    // Catalog contract pin: membership creation is governed solely by
    // `identity.membership.update`; the (currently unrouted) user-manage
    // key must not reach it (Stage 8 review, MINOR #2).
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "user-manager",
            &["identity.user.manage"],
        );
    });
    let (status, _) = call(
        router,
        authorized(
            Method::POST,
            "/api/v1/admin/tenant-memberships",
            Some("user-manage-probe"),
            Some(json!({"user_id": FRESH_USER_ID})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn membership_create_list_suspend_reactivate() {
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "identity-admins",
            &[
                "identity.read",
                "identity.user.manage",
                "identity.membership.update",
            ],
        );
    });
    let (status, created) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/tenant-memberships",
            Some("mk-membership"),
            Some(json!({"user_id": FRESH_USER_ID})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(data(&created)["user_id"], json!(FRESH_USER_ID));
    assert_eq!(data(&created)["status"], "active");

    let (status, listed) = call(
        router.clone(),
        get("/api/v1/admin/tenant-memberships?limit=50"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(array(&data(&listed)["items"]).len(), 3); // dev + target + fresh

    let (status, users) = call(router.clone(), get("/api/v1/admin/users?limit=50")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(array(&data(&users)["items"]).len(), 3);

    let (status, suspended) = call(
        router.clone(),
        authorized(
            Method::POST,
            &format!("/api/v1/admin/tenant-memberships/{FRESH_USER_ID}/suspend"),
            Some("mk-suspend"),
            Some(json!({"expected_version": data(&created)["version"]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&suspended)["status"], "suspended");

    let (status, reactivated) = call(
        router,
        authorized(
            Method::POST,
            &format!("/api/v1/admin/tenant-memberships/{FRESH_USER_ID}/reactivate"),
            Some("mk-reactivate"),
            Some(json!({"expected_version": data(&suspended)["version"]})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&reactivated)["status"], "active");
}

#[tokio::test]
async fn permissions_catalog_lists_known_keys() {
    let router = iam_router_with(|_, policy, _| {
        grant(policy, DEV_USER_ID, "readers", &["policy.role.read"]);
    });
    let (status, value) = call(router, get("/api/v1/admin/permissions")).await;
    assert_eq!(status, StatusCode::OK);
    let keys: Vec<&str> = array(data(&value))
        .iter()
        .filter_map(|entry| entry["key"].as_str())
        .collect();
    for expected in [
        "identity.read",
        "identity.user.manage",
        "identity.membership.update",
        "organization.read",
        "organization.manage",
        "policy.role.read",
        "policy.role.manage",
        "policy.binding.read",
        "policy.binding.manage",
        "policy.explain",
        "integrity.read",
        "audit.read",
    ] {
        assert!(keys.contains(&expected), "catalog missing {expected}");
    }
}

#[tokio::test]
async fn explain_reports_bounded_decision_for_target_user() {
    let router = iam_router_with(|_, policy, _| {
        grant(policy, TARGET_USER_ID, "target-readers", &["identity.read"]);
        grant(policy, DEV_USER_ID, "explain-admins", &["policy.explain"]);
    });
    let (status, value) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/authorization/explain",
            None,
            Some(json!({"user_id": TARGET_USER_ID, "permission": "identity.read"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&value)["allowed"], true);
    assert!(data(&value)["matched_binding"].is_string());
    assert_eq!(data(&value)["matched_permission"], "identity.read");
    assert!(!data(&value)["policy_reference"]
        .as_str()
        .unwrap_or_default()
        .is_empty());

    let (status, denied) = call(
        router,
        authorized(
            Method::POST,
            "/api/v1/admin/authorization/explain",
            None,
            Some(json!({"user_id": FRESH_USER_ID, "permission": "identity.read"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&denied)["allowed"], false);
    assert!(data(&denied)["evaluations"].is_array());
}

// ---------------------------------------------------------------------------
// Idempotency, versioning, isolation, domain rules.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn write_without_idempotency_key_is_400() {
    let router = iam_router_with(|_, policy, _| {
        grant(policy, DEV_USER_ID, "admins", &["policy.role.manage"]);
    });
    let request = authorized(
        Method::POST,
        "/api/v1/admin/roles",
        None,
        Some(json!({"stable_key": "no-key", "display_name": "No key"})),
    );
    let (status, value) = call(router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        value["message"]
            .as_str()
            .is_some_and(|m| m.contains("Idempotency-Key")),
        "message should name the header: {value}"
    );
}

#[tokio::test]
async fn replay_same_key_returns_original_without_double_mutation() {
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "admins",
            &["policy.role.manage", "policy.role.read"],
        );
    });
    let body = json!({"stable_key": "replayed", "display_name": "Replayed"});
    let (first_status, first) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/roles",
            Some("same-key"),
            Some(body.clone()),
        ),
    )
    .await;
    assert_eq!(first_status, StatusCode::CREATED);
    let (second_status, second) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/roles",
            Some("same-key"),
            Some(body),
        ),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(data(&first), data(&second));

    let (status, listed) = call(router, get("/api/v1/admin/roles")).await;
    assert_eq!(status, StatusCode::OK);
    let matches = array(data(&listed))
        .iter()
        .filter(|role| role["stable_key"] == "replayed")
        .count();
    assert_eq!(matches, 1);
}

#[tokio::test]
async fn stale_expected_version_is_conflict() {
    let router = iam_router_with(|_, policy, _| {
        grant(policy, DEV_USER_ID, "admins", &["policy.role.manage"]);
    });
    let (status, created) = call(
        router.clone(),
        authorized(
            Method::POST,
            "/api/v1/admin/roles",
            Some("mk-v"),
            Some(json!({"stable_key": "versioned", "display_name": "Versioned"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let role_id = data(&created)["role_id"].as_str().expect("id").to_string();
    let stale = data(&created)["version"].as_i64().expect("version") + 100;
    let (status, _) = call(
        router,
        authorized(
            Method::PATCH,
            &format!("/api/v1/admin/roles/{role_id}"),
            Some("mk-stale"),
            Some(json!({"display_name": "late", "expected_version": stale})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn foreign_tenant_role_is_not_found() {
    let router = iam_router_with(|_, policy, _| {
        grant(policy, DEV_USER_ID, "readers", &["policy.role.read"]);
        let now = chrono::Utc::now();
        let role = RoleDefinition::create_tenant_role(
            Uuid::from_u128(0xbb),
            OTHER_TENANT_ID,
            "foreign",
            "Foreign",
            now,
        )
        .expect("foreign role fixture");
        policy.seed_role(role, &[]);
    });
    let (status, _) = call(
        router.clone(),
        get("/api/v1/admin/roles/00000000-0000-0000-0000-0000000000bb"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Also must not appear in the tenant listing.
    let (status, listed) = call(router, get("/api/v1/admin/roles")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!array(data(&listed))
        .iter()
        .any(|role| role["stable_key"] == "foreign"));
}

#[tokio::test]
async fn system_role_mutation_is_rejected() {
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "admins",
            &["policy.role.read", "policy.role.manage"],
        );
    });
    let (status, listed) = call(router.clone(), get("/api/v1/admin/roles")).await;
    assert_eq!(status, StatusCode::OK);
    let system = array(data(&listed))
        .into_iter()
        .find(|role| role["system"] == json!(true))
        .expect("system role fixture must exist");
    let role_id = system["role_id"].as_str().expect("id").to_string();
    let version = system["version"].as_i64().expect("version");
    let (status, _) = call(
        router.clone(),
        authorized(
            Method::PATCH,
            &format!("/api/v1/admin/roles/{role_id}"),
            Some("mk-system"),
            Some(json!({"display_name": "tampered", "expected_version": version})),
        ),
    )
    .await;
    assert!(
        status.is_client_error(),
        "system role rename must be rejected, got {status}"
    );
    let (status, _) = call(
        router,
        authorized(
            Method::PUT,
            &format!("/api/v1/admin/roles/{role_id}/permissions"),
            Some("mk-system-perms"),
            Some(json!({"permission_keys": [], "expected_version": version})),
        ),
    )
    .await;
    assert!(
        status.is_client_error(),
        "system role permission replace must be rejected, got {status}"
    );
}

#[tokio::test]
async fn self_membership_reactivation_is_rejected() {
    // The domain-safety invariant: a user may never lift their own
    // suspension. The guard fires before any state transition, so an
    // Active self-membership reactivate is already rejected.
    let router = iam_router_with(|_, policy, _| {
        grant(
            policy,
            DEV_USER_ID,
            "admins",
            &["identity.membership.update"],
        );
    });
    let (status, _) = call(
        router,
        authorized(
            Method::POST,
            &format!("/api/v1/admin/tenant-memberships/{DEV_USER_ID}/reactivate"),
            Some("mk-self"),
            Some(json!({"expected_version": 1})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn self_binding_escalation_is_rejected() {
    let router = iam_router_with(|_, policy, _| {
        grant(policy, DEV_USER_ID, "binders", &["policy.binding.manage"]);
        // The self-bind guard engages only for roles carrying an IAM
        // management permission; bind a management-keyed role to self.
        seed_role(policy, FIXTURE_ROLE_ID, "power", &["policy.role.manage"]);
    });
    let (status, _) = call(
        router,
        authorized(
            Method::POST,
            "/api/v1/admin/role-bindings",
            Some("mk-self-bind"),
            Some(json!({"user_id": DEV_USER_ID, "role_id": FIXTURE_ROLE_ID, "scope": {"scope": "tenant"}})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn invalid_cursor_and_limit_are_400() {
    let router =
        iam_router_with(|_, policy, _| grant(policy, DEV_USER_ID, "readers", &["identity.read"]));
    let (status, _) = call(
        router.clone(),
        get("/api/v1/admin/users?limit=20&cursor=not-a-cursor"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(router, get("/api/v1/admin/users?limit=0")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_permission_in_explain_denies_boundedly() {
    let router = iam_router_with(|_, policy, _| {
        grant(policy, DEV_USER_ID, "explainers", &["policy.explain"]);
    });
    let (status, value) = call(
        router,
        authorized(
            Method::POST,
            "/api/v1/admin/authorization/explain",
            None,
            Some(json!({"user_id": DEV_USER_ID, "permission": "does.not.exist"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data(&value)["allowed"], false);
    assert!(data(&value)["reason"].is_string());
}

#[tokio::test]
async fn role_binding_with_retired_catalog_permission_denies_route() {
    // Sanity: a binding that grants a retired key must not let the caller
    // through (catalog check runs before bindings in the evaluator).
    let router = iam_router_with(|_, policy, _| {
        grant(policy, DEV_USER_ID, "readers", &["identity.read"]);
        policy.retire_permission("identity.read");
    });
    let (status, _) = call(router, get("/api/v1/admin/users?limit=20")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
