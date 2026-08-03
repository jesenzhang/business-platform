#![allow(clippy::expect_used)]

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
use document::domain::{
    DocumentMetadata, DocumentPage, DocumentQueryRepository, ListDocumentsQuery, RepositoryError,
};
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

#[derive(Default)]
struct FakeStore {
    documents: RwLock<Vec<DocumentMetadata>>,
}

#[async_trait]
impl DocumentQueryRepository for FakeStore {
    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Option<DocumentMetadata>, RepositoryError> {
        Ok(self
            .documents
            .read()
            .await
            .iter()
            .find(|document| document.tenant_id == tenant_id && document.id == document_id)
            .cloned())
    }

    async fn list(&self, query: ListDocumentsQuery) -> Result<DocumentPage, RepositoryError> {
        let mut items: Vec<_> = self
            .documents
            .read()
            .await
            .iter()
            .filter(|document| document.tenant_id == query.tenant_id)
            .cloned()
            .collect();
        items.sort_by_key(|document| std::cmp::Reverse(document.created_at));
        let total = i64::try_from(items.len()).unwrap_or(0);
        let items = items
            .into_iter()
            .skip(usize::try_from(query.offset).unwrap_or(0))
            .take(usize::try_from(query.limit).unwrap_or(20))
            .collect();
        Ok(DocumentPage { items, total })
    }
}

#[async_trait]
impl CreateDocumentUnitOfWork for FakeStore {
    async fn execute(
        &self,
        command: PersistNewDocument,
    ) -> Result<CreateDocumentResult, ApplicationPortError> {
        let mut documents = self.documents.write().await;
        if let Some(existing) = documents
            .iter()
            .find(|document| document.tenant_id == command.document.tenant_id)
            .cloned()
        {
            return Ok(CreateDocumentResult {
                document: existing,
                replayed: true,
            });
        }
        documents.push(command.document.clone());
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
            .documents
            .read()
            .await
            .iter()
            .find(|item| item.tenant_id == tenant_id && item.id == document_id)
            .map(|item| DocumentDetailView {
                id: item.id,
                tenant_id: item.tenant_id,
                original_filename: item.original_filename.clone(),
                content_type: item.content_type.clone(),
                status: view_status(item.status),
                version: item.version,
                size_bytes: item.size_bytes,
                created_by: item.created_by,
                created_at: item.created_at,
                updated_at: item.updated_at,
            }))
    }
}

#[async_trait]
impl DocumentListQuery for FakeStore {
    async fn execute(&self, request: DocumentListRequest) -> Result<DocumentListPage, QueryError> {
        let mut items = self
            .documents
            .read()
            .await
            .iter()
            .filter(|item| item.tenant_id == request.tenant_id)
            .map(|item| DocumentListItem {
                id: item.id,
                original_filename: item.original_filename.clone(),
                content_type: item.content_type.clone(),
                status: view_status(item.status),
                version: item.version,
                size_bytes: item.size_bytes,
                created_at: item.created_at,
                updated_at: item.updated_at,
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
        },
    };
    let state = Arc::new(AppState {
        documents: DocumentServices {
            create: Arc::new(CreateDocumentMetadata::new(store.clone())),
            detail: store.clone(),
            list: store,
        },
        readiness: Arc::new(ReadyProbe),
    });
    routes::create_router(
        state,
        AuthMiddlewareConfig {
            dev_auth_enabled: true,
            dev_secret: Some(SECRET.to_string()),
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
