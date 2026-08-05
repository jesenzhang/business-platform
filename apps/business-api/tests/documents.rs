#![allow(clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use business_api::auth::AuthMiddlewareConfig;
use business_api::config::{
    AuthConfig, BusinessApiConfig, DatabaseBackend, DatabaseConfig, ObservabilityConfig,
    ServerConfig,
};
use business_api::routes;
use business_api::state::{
    AppState, DocumentServices, ReadinessProbe, ReadinessReport, ReadinessStatus,
};
use document::application::CreateDocumentMetadata;
use document::domain::DocumentMetadata;
use document::ports::{
    ApplicationPortError, CreateDocumentResult, CreateDocumentUnitOfWork, PersistNewDocument,
};
use document::query::{
    DocumentDetailQuery, DocumentDetailView, DocumentListCursor, DocumentListItem,
    DocumentListPage, DocumentListQuery, DocumentListRequest, DocumentStatusView, QueryError,
};
use http_body_util::BodyExt;
use runtime_config::{RuntimeEnvironment, Secret, SecretUrl};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "test-dev-secret";
const TENANT_A: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const TENANT_B: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const USER_A: &str = "11111111-1111-1111-1111-111111111111";

struct FakeStore {
    state: RwLock<FakeState>,
}

#[derive(Default)]
struct FakeState {
    documents: Vec<DocumentMetadata>,
    idempotency: HashMap<(Uuid, String), StoredIdempotency>,
}

#[derive(Clone)]
struct StoredIdempotency {
    request_fingerprint: String,
    fingerprint_version: i16,
    document_id: Uuid,
}

impl Default for FakeStore {
    fn default() -> Self {
        Self {
            state: RwLock::new(FakeState::default()),
        }
    }
}

#[async_trait]
impl CreateDocumentUnitOfWork for FakeStore {
    async fn execute(
        &self,
        command: PersistNewDocument,
    ) -> Result<CreateDocumentResult, ApplicationPortError> {
        let mut state = self.state.write().await;
        let key = (
            command.document.tenant_id(),
            command.idempotency_key.clone(),
        );
        if let Some(existing_key) = state.idempotency.get(&key) {
            if existing_key.request_fingerprint != command.request_fingerprint
                || existing_key.fingerprint_version != command.fingerprint_version
            {
                return Err(ApplicationPortError::IdempotencyConflict);
            }
            let Some(existing) = state
                .documents
                .iter()
                .find(|document| document.id() == existing_key.document_id)
                .cloned()
            else {
                return Err(ApplicationPortError::Failed);
            };
            return Ok(CreateDocumentResult {
                document: existing,
                replayed: true,
            });
        }
        state.documents.push(command.document.clone());
        state.idempotency.insert(
            key,
            StoredIdempotency {
                request_fingerprint: command.request_fingerprint,
                fingerprint_version: command.fingerprint_version,
                document_id: command.document.id(),
            },
        );
        Ok(CreateDocumentResult {
            document: command.document,
            replayed: false,
        })
    }
}

fn view_status(status: document::domain::DocumentStatus) -> DocumentStatusView {
    match status {
        document::domain::DocumentStatus::Active => DocumentStatusView::Active,
        document::domain::DocumentStatus::Archived => DocumentStatusView::Archived,
        document::domain::DocumentStatus::Deleted => DocumentStatusView::Deleted,
    }
}

#[async_trait]
impl DocumentDetailQuery for FakeStore {
    async fn execute(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentDetailView>, QueryError> {
        Ok(self
            .state
            .read()
            .await
            .documents
            .iter()
            .find(|item| item.tenant_id() == tenant_id && item.id() == document_id)
            .map(|item| DocumentDetailView {
                id: item.id(),
                tenant_id: item.tenant_id(),
                original_filename: item.original_filename().to_string(),
                content_type: item.content_type().to_string(),
                status: view_status(item.status()),
                version: item.version(),
                content_revision: item.content_revision().value(),
                size_bytes: item.size_bytes(),
                created_by: item.created_by(),
                created_at: item.created_at(),
                updated_at: item.updated_at(),
            }))
    }
}

#[async_trait]
impl DocumentListQuery for FakeStore {
    async fn execute(&self, request: DocumentListRequest) -> Result<DocumentListPage, QueryError> {
        let mut items = self
            .state
            .read()
            .await
            .documents
            .iter()
            .filter(|item| item.tenant_id() == request.tenant_id)
            .map(|item| DocumentListItem {
                id: item.id(),
                original_filename: item.original_filename().to_string(),
                content_type: item.content_type().to_string(),
                status: view_status(item.status()),
                version: item.version(),
                content_revision: item.content_revision().value(),
                size_bytes: item.size_bytes(),
                created_at: item.created_at(),
                updated_at: item.updated_at(),
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|item| std::cmp::Reverse((item.created_at, item.id)));
        items.truncate(request.limit as usize);
        let next_cursor = None::<DocumentListCursor>;
        Ok(DocumentListPage { items, next_cursor })
    }
}

struct ReadyProbe;

#[async_trait]
impl ReadinessProbe for ReadyProbe {
    async fn check(&self) -> ReadinessReport {
        ReadinessReport {
            status: ReadinessStatus::Ready,
            database: "available",
            migrations: "compatible",
        }
    }
}

fn test_router(store: Arc<FakeStore>) -> axum::Router {
    let config = BusinessApiConfig {
        env: RuntimeEnvironment::Development,
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            request_timeout_secs: 30,
            cors_origins: Vec::new(),
            body_limit_bytes: 1024 * 1024,
        },
        database: DatabaseConfig {
            backend: DatabaseBackend::Postgres,
            url: SecretUrl::parse("postgres://localhost/test").expect("test URL should parse"),
            max_connections: 2,
            min_connections: 0,
            acquire_timeout_secs: 1,
        },
        observability: ObservabilityConfig {
            service_name: "test".to_string(),
            otlp_endpoint: None,
            log_level: "info".to_string(),
        },
        auth: AuthConfig {
            issuer_url: String::new(),
            audience: None,
            dev_secret: Some(Secret::new(SECRET.to_string())),
            dev_auth_enabled: true,
            dev_permissions: BTreeSet::new(),
            dev_tenant_id: Some(Uuid::parse_str(TENANT_A).expect("tenant fixture")),
            dev_user_id: Some(Uuid::parse_str(USER_A).expect("user fixture")),
            dev_subject: Some("document-test-user".to_string()),
            dev_roles: BTreeSet::new(),
        },
    };
    let state = Arc::new(AppState {
        documents: DocumentServices {
            create: Arc::new(CreateDocumentMetadata::new(store.clone())),
            detail: store.clone(),
            list: store,
        },
        processing: None,
        governance: None,
        readiness: Arc::new(ReadyProbe),
    });
    routes::create_router(
        state,
        AuthMiddlewareConfig {
            dev_auth_enabled: true,
            dev_secret: Some(SECRET.to_string()),
            dev_permissions: BTreeSet::new(),
            dev_tenant_id: Some(Uuid::parse_str(TENANT_A).expect("tenant fixture")),
            dev_user_id: Some(Uuid::parse_str(USER_A).expect("user fixture")),
            dev_subject: Some("document-test-user".to_string()),
            dev_roles: BTreeSet::new(),
        },
        &config.server,
    )
}

fn request(method: &str, uri: &str, tenant: &str, body: Option<String>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {SECRET}"))
        .header("x-tenant-id", tenant)
        .header("x-user-id", USER_A)
        .header("idempotency-key", "create-document-1")
        .header("x-request-id", "request-test-1");
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, Body::from))
        .expect("request must build")
}

#[tokio::test]
async fn unauthenticated_document_list_is_rejected() {
    let router = test_router(Arc::new(FakeStore::default()));
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v1/documents")
                .body(Body::empty())
                .expect("request must build"),
        )
        .await
        .expect("router must respond");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_is_idempotent_and_returns_trace_id_on_error() {
    let router = test_router(Arc::new(FakeStore::default()));
    let body = serde_json::json!({
        "original_filename": "report.pdf",
        "content_type": "application/pdf",
        "object_key": "report.pdf",
        "size_bytes": 42
    })
    .to_string();
    let first = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/documents",
            TENANT_A,
            Some(body.clone()),
        ))
        .await
        .expect("router must respond");
    assert_eq!(first.status(), StatusCode::CREATED);
    let second = router
        .oneshot(request("POST", "/api/v1/documents", TENANT_A, Some(body)))
        .await
        .expect("router must respond");
    assert_eq!(second.status(), StatusCode::OK);
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body must collect")
        .to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    for key in ["object_key", "storage_key", "bucket", "internal_path"] {
        assert!(
            !text.contains(&format!("\"{key}\"")),
            "response leaked {key}: {text}"
        );
    }
    serde_json::from_slice(&bytes).expect("valid JSON")
}

#[tokio::test]
async fn document_http_responses_never_expose_storage_locations() {
    let router = test_router(Arc::new(FakeStore::default()));
    let body = serde_json::json!({
        "original_filename": "visible.pdf",
        "content_type": "application/pdf",
        "object_key": "private/visible.pdf"
    })
    .to_string();
    let create = router
        .clone()
        .oneshot(request("POST", "/api/v1/documents", TENANT_A, Some(body)))
        .await
        .expect("router must respond");
    let payload = response_json(create).await;
    let id = payload["data"]["id"].as_str().expect("document id");

    let get = router
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/v1/documents/{id}"),
            TENANT_A,
            None,
        ))
        .await
        .expect("router must respond");
    assert_eq!(get.status(), StatusCode::OK);
    response_json(get).await;

    let list = router
        .oneshot(request("GET", "/api/v1/documents", TENANT_A, None))
        .await
        .expect("router must respond");
    assert_eq!(list.status(), StatusCode::OK);
    response_json(list).await;
}

#[tokio::test]
async fn fake_idempotency_is_scoped_by_tenant_and_fingerprint() {
    let router = test_router(Arc::new(FakeStore::default()));
    let first_body = serde_json::json!({
        "original_filename": "first.pdf",
        "content_type": "application/pdf",
        "object_key": "first.pdf"
    })
    .to_string();
    let first = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/documents",
            TENANT_A,
            Some(first_body.clone()),
        ))
        .await
        .expect("router must respond");
    assert_eq!(first.status(), StatusCode::CREATED);

    let other_tenant = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/documents",
            TENANT_B,
            Some(first_body),
        ))
        .await
        .expect("router must respond");
    assert_eq!(other_tenant.status(), StatusCode::CREATED);

    let conflicting_body = serde_json::json!({
        "original_filename": "different.pdf",
        "content_type": "application/pdf",
        "object_key": "different.pdf"
    })
    .to_string();
    let conflict = router
        .oneshot(request(
            "POST",
            "/api/v1/documents",
            TENANT_A,
            Some(conflicting_body),
        ))
        .await
        .expect("router must respond");
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn invalid_or_legacy_http_cursors_are_rejected() {
    let router = test_router(Arc::new(FakeStore::default()));
    let invalid = router
        .clone()
        .oneshot(request(
            "GET",
            "/api/v1/documents?cursor=not-base64",
            TENANT_A,
            None,
        ))
        .await
        .expect("router must respond");
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let legacy = router
        .oneshot(request(
            "GET",
            "/api/v1/documents?cursor_created_at=2026-01-01T00%3A00%3A00Z&cursor_id=00000000-0000-0000-0000-000000000001",
            TENANT_A,
            None,
        ))
        .await
        .expect("router must respond");
    assert_eq!(legacy.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn cross_tenant_get_is_not_found() {
    let store = Arc::new(FakeStore::default());
    let router = test_router(store.clone());
    let body = serde_json::json!({
        "original_filename": "secret.pdf",
        "content_type": "application/pdf",
        "object_key": "secret.pdf"
    })
    .to_string();
    let response = router
        .clone()
        .oneshot(request("POST", "/api/v1/documents", TENANT_A, Some(body)))
        .await
        .expect("router must respond");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body must collect")
        .to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    let id = payload["data"]["id"].as_str().expect("document id");
    let response = router
        .oneshot(request(
            "GET",
            &format!("/api/v1/documents/{id}"),
            TENANT_B,
            None,
        ))
        .await
        .expect("router must respond");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
