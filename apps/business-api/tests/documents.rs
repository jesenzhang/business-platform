//! Document API integration tests.
//!
//! Tests the document HTTP endpoints via `tower::ServiceExt::oneshot`.
//! Uses an in-memory mock repository so most tests run without a real database.
//! Tests that require transactional writes (create) are marked `#[ignore]`.

#![allow(clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::Router;
use business_api::auth::{auth_middleware, AuthMiddlewareConfig};
use document::api::{self, DocumentServices};
use document::domain::{DocumentMetadata, DocumentRepository};
use http_body_util::BodyExt;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

const DEV_SECRET: &str = "test-dev-secret";
const TENANT_A: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const TENANT_B: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const USER_A: &str = "11111111-1111-1111-1111-111111111111";

/// In-memory mock repository for testing without a real database.
#[derive(Default)]
struct MockDocumentRepository {
    docs: RwLock<HashMap<Uuid, DocumentMetadata>>,
}

impl MockDocumentRepository {
    fn new() -> Self {
        Self::default()
    }

    /// Seed a document directly (bypasses the create use case).
    async fn seed(&self, doc: DocumentMetadata) {
        self.docs.write().await.insert(doc.id, doc);
    }
}

#[async_trait]
impl DocumentRepository for MockDocumentRepository {
    async fn save(
        &self,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        doc: &DocumentMetadata,
    ) -> Result<(), sqlx::Error> {
        self.docs.write().await.insert(doc.id, doc.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DocumentMetadata>, sqlx::Error> {
        let docs = self.docs.read().await;
        Ok(docs.get(&id).filter(|d| d.tenant_id == tenant_id).cloned())
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<DocumentMetadata>, i64), sqlx::Error> {
        let docs = self.docs.read().await;
        let mut tenant_docs: Vec<DocumentMetadata> = docs
            .values()
            .filter(|d| d.tenant_id == tenant_id)
            .cloned()
            .collect();
        tenant_docs.sort_by_key(|d| std::cmp::Reverse(d.created_at));

        let total = i64::try_from(tenant_docs.len()).unwrap_or(0);
        let offset_usize = usize::try_from(offset).unwrap_or(0);
        let limit_usize = usize::try_from(limit).unwrap_or(20);
        let items: Vec<DocumentMetadata> = tenant_docs
            .into_iter()
            .skip(offset_usize)
            .take(limit_usize)
            .collect();

        Ok((items, total))
    }
}

fn auth_config() -> AuthMiddlewareConfig {
    AuthMiddlewareConfig {
        dev_auth_enabled: true,
        dev_secret: Some(DEV_SECRET.to_string()),
    }
}

/// Build a test router with the mock repository (no real DB needed for GET/LIST).
fn test_router_with_mock(mock_repo: Arc<MockDocumentRepository>) -> Router {
    let pool = PgPool::connect_lazy("postgres://user:pass@localhost:5432/testdb")
        .expect("lazy pool must not connect eagerly");

    let services = DocumentServices {
        repo: mock_repo,
        pool,
    };

    Router::new()
        .nest("/api/v1/documents", api::router(services))
        .layer(middleware::from_fn_with_state(
            auth_config(),
            auth_middleware,
        ))
}

fn authorized_request(
    method: &str,
    uri: &str,
    tenant_id: &str,
    user_id: &str,
    body: Option<&str>,
) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {DEV_SECRET}"))
        .header("x-tenant-id", tenant_id)
        .header("x-user-id", user_id)
        .header("content-type", "application/json");

    let body = match body {
        Some(b) => Body::from(b.to_string()),
        None => Body::empty(),
    };

    builder.body(body).expect("request must build")
}

fn unauthenticated_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request must build")
}

async fn response_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body must collect")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("body must be valid JSON")
}

// --- Auth tests (no DB needed) ---

#[tokio::test]
async fn unauthenticated_create_is_rejected() {
    let mock = Arc::new(MockDocumentRepository::new());
    let router = test_router_with_mock(mock);

    let request = unauthenticated_request("POST", "/api/v1/documents");
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthenticated_list_is_rejected() {
    let mock = Arc::new(MockDocumentRepository::new());
    let router = test_router_with_mock(mock);

    let request = unauthenticated_request("GET", "/api/v1/documents");
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthenticated_get_is_rejected() {
    let mock = Arc::new(MockDocumentRepository::new());
    let router = test_router_with_mock(mock);

    let id = Uuid::now_v7();
    let uri = format!("/api/v1/documents/{id}");
    let request = unauthenticated_request("GET", &uri);
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// --- GET tests (mock repo, no DB needed) ---

#[tokio::test]
async fn get_document_returns_200() {
    let mock = Arc::new(MockDocumentRepository::new());

    let tenant_id = Uuid::parse_str(TENANT_A).expect("valid uuid");
    let doc = DocumentMetadata::create(
        tenant_id,
        "report.pdf".to_string(),
        "application/pdf".to_string(),
        "documents/tenant-a/report.pdf".to_string(),
        Uuid::parse_str(USER_A).expect("valid uuid"),
        Some(2048),
    )
    .expect("valid document");

    mock.seed(doc.clone()).await;

    let router = test_router_with_mock(Arc::clone(&mock));
    let uri = format!("/api/v1/documents/{}", doc.id);
    let request = authorized_request("GET", &uri, TENANT_A, USER_A, None);
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_body(response).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["original_filename"], "report.pdf");
    assert_eq!(body["data"]["content_type"], "application/pdf");
}

#[tokio::test]
async fn get_nonexistent_document_returns_404() {
    let mock = Arc::new(MockDocumentRepository::new());
    let router = test_router_with_mock(mock);

    let id = Uuid::now_v7();
    let uri = format!("/api/v1/documents/{id}");
    let request = authorized_request("GET", &uri, TENANT_A, USER_A, None);
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cross_tenant_access_returns_404() {
    let mock = Arc::new(MockDocumentRepository::new());

    // Create document owned by tenant A.
    let tenant_a = Uuid::parse_str(TENANT_A).expect("valid uuid");
    let doc = DocumentMetadata::create(
        tenant_a,
        "secret.pdf".to_string(),
        "application/pdf".to_string(),
        "documents/tenant-a/secret.pdf".to_string(),
        Uuid::parse_str(USER_A).expect("valid uuid"),
        None,
    )
    .expect("valid document");

    mock.seed(doc.clone()).await;

    // Try to access with tenant B.
    let router = test_router_with_mock(Arc::clone(&mock));
    let uri = format!("/api/v1/documents/{}", doc.id);
    let request = authorized_request("GET", &uri, TENANT_B, USER_A, None);
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// --- LIST tests (mock repo, no DB needed) ---

#[tokio::test]
async fn list_documents_returns_200_with_items() {
    let mock = Arc::new(MockDocumentRepository::new());

    let tenant_id = Uuid::parse_str(TENANT_A).expect("valid uuid");
    let user_id = Uuid::parse_str(USER_A).expect("valid uuid");

    for i in 0..3 {
        let doc = DocumentMetadata::create(
            tenant_id,
            format!("file-{i}.pdf"),
            "application/pdf".to_string(),
            format!("documents/tenant-a/file-{i}.pdf"),
            user_id,
            None,
        )
        .expect("valid document");
        mock.seed(doc).await;
    }

    let router = test_router_with_mock(Arc::clone(&mock));
    let request = authorized_request(
        "GET",
        "/api/v1/documents?page=1&page_size=10",
        TENANT_A,
        USER_A,
        None,
    );
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_body(response).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["total"], 3);
    assert_eq!(
        body["data"]["items"].as_array().expect("items array").len(),
        3
    );
}

#[tokio::test]
async fn list_documents_is_tenant_scoped() {
    let mock = Arc::new(MockDocumentRepository::new());

    let tenant_a = Uuid::parse_str(TENANT_A).expect("valid uuid");
    let tenant_b = Uuid::parse_str(TENANT_B).expect("valid uuid");
    let user_id = Uuid::parse_str(USER_A).expect("valid uuid");

    // Seed docs for tenant A.
    let doc_a = DocumentMetadata::create(
        tenant_a,
        "a.pdf".to_string(),
        "application/pdf".to_string(),
        "documents/tenant-a/a.pdf".to_string(),
        user_id,
        None,
    )
    .expect("valid document");
    mock.seed(doc_a).await;

    // Seed docs for tenant B.
    let doc_b = DocumentMetadata::create(
        tenant_b,
        "b.pdf".to_string(),
        "application/pdf".to_string(),
        "documents/tenant-b/b.pdf".to_string(),
        user_id,
        None,
    )
    .expect("valid document");
    mock.seed(doc_b).await;

    // List as tenant A - should only see tenant A's docs.
    let router = test_router_with_mock(Arc::clone(&mock));
    let request = authorized_request("GET", "/api/v1/documents", TENANT_A, USER_A, None);
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_body(response).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["original_filename"], "a.pdf");
}

// --- CREATE tests (require real DB for transactions) ---

#[tokio::test]
#[ignore = "requires a real PostgreSQL database for transaction support"]
async fn create_document_returns_201() {
    // This test requires a real database because CreateDocumentMetadata
    // uses a transaction (pool.begin()) which cannot be mocked.
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/business".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("must connect to database");

    let repo: Arc<dyn DocumentRepository> = Arc::new(
        document::infrastructure::PostgresDocumentRepository::new(pool.clone()),
    );

    let services = DocumentServices { repo, pool };

    let router = Router::new()
        .nest("/api/v1/documents", api::router(services))
        .layer(middleware::from_fn_with_state(
            auth_config(),
            auth_middleware,
        ));

    let body = serde_json::json!({
        "original_filename": "test.pdf",
        "content_type": "application/pdf",
        "object_key": "documents/tenant-a/test.pdf",
        "size_bytes": 1024
    });

    let request = authorized_request(
        "POST",
        "/api/v1/documents",
        TENANT_A,
        USER_A,
        Some(&body.to_string()),
    );
    let response = router.oneshot(request).await.expect("router must respond");

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response_body(response).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["original_filename"], "test.pdf");
    assert_eq!(body["data"]["status"], "active");
    assert_eq!(body["data"]["version"], 1);
}
