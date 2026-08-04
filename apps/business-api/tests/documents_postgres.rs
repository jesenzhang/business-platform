#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use business_api::auth::AuthMiddlewareConfig;
use business_api::config::{
    AuthConfig, BusinessApiConfig, DatabaseBackend, DatabaseConfig, ObservabilityConfig,
    ServerConfig,
};
use business_api::routes;
use business_api::state::{AppState, DocumentServices, PostgresReadinessProbe};
use document::application::CreateDocumentMetadata;
use http_body_util::BodyExt;
use runtime_config::{RuntimeEnvironment, Secret, SecretUrl};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "test-dev-secret";
const USER_ID: &str = "11111111-1111-1111-1111-111111111111";

async fn setup_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running the PostgreSQL E2E test");
    PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .expect("failed to connect to PostgreSQL")
}

async fn run_migrations(pool: &sqlx::PgPool) {
    runtime_migration::MIGRATOR
        .run(pool)
        .await
        .expect("failed to run migrations");
}

fn test_router(pool: sqlx::PgPool) -> axum::Router {
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
            max_connections: 10,
            min_connections: 0,
            acquire_timeout_secs: 2,
        },
        observability: ObservabilityConfig {
            service_name: "business-api-postgres-test".to_string(),
            otlp_endpoint: None,
            log_level: "info".to_string(),
        },
        auth: AuthConfig {
            issuer_url: String::new(),
            audience: None,
            dev_secret: Some(Secret::new(SECRET.to_string())),
            dev_auth_enabled: true,
            dev_permissions: BTreeSet::new(),
        },
    };
    let unit_of_work = Arc::new(document_postgres::PostgresCreateDocumentUnitOfWork::new(
        pool.clone(),
    ));
    let state = Arc::new(AppState {
        documents: DocumentServices {
            create: Arc::new(CreateDocumentMetadata::new(unit_of_work)),
            detail: Arc::new(document_postgres::PostgresDocumentDetailQuery::new(
                pool.clone(),
            )),
            list: Arc::new(document_postgres::PostgresDocumentListQuery::new(
                pool.clone(),
            )),
        },
        processing: None,
        governance: None,
        readiness: Arc::new(PostgresReadinessProbe::new(pool)),
    });
    routes::create_router(
        state,
        AuthMiddlewareConfig {
            dev_auth_enabled: true,
            dev_secret: Some(SECRET.to_string()),
            dev_permissions: BTreeSet::new(),
        },
        &config.server,
    )
}

fn request(method: &str, uri: &str, tenant_id: Uuid, body: Option<String>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {SECRET}"))
        .header("x-tenant-id", tenant_id.to_string())
        .header("x-user-id", USER_ID)
        .header("idempotency-key", "postgres-e2e-document-1")
        .header("x-request-id", "postgres-e2e-request-1");
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, Body::from))
        .expect("request must build")
}

#[tokio::test]
#[ignore = "requires running PostgreSQL"]
#[allow(clippy::too_many_lines)]
async fn document_http_flow_is_atomic_idempotent_and_tenant_scoped() {
    let pool = setup_pool().await;
    run_migrations(&pool).await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();

    let body = serde_json::json!({
        "original_filename": "report.pdf",
        "content_type": "application/pdf",
        "object_key": "report.pdf",
        "size_bytes": 42
    })
    .to_string();
    let router = test_router(pool.clone());

    let created = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/documents",
            tenant_a,
            Some(body.clone()),
        ))
        .await
        .expect("router must respond");
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_payload: serde_json::Value = serde_json::from_slice(
        &created
            .into_body()
            .collect()
            .await
            .expect("body must collect")
            .to_bytes(),
    )
    .expect("created response must be JSON");
    let document_id = Uuid::parse_str(
        created_payload["data"]["id"]
            .as_str()
            .expect("created response must contain id"),
    )
    .expect("document id must be UUID");

    let counts = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT \
            (SELECT COUNT(*) FROM documents WHERE id = $1), \
            (SELECT COUNT(*) FROM audit_events WHERE tenant_id = $2 AND resource_id = $3), \
            (SELECT COUNT(*) FROM outbox_events WHERE tenant_id = $4 AND aggregate_id = $5), \
            (SELECT COUNT(*) FROM document_idempotency WHERE tenant_id = $2 AND idempotency_key = $6)",
    )
    .bind(document_id)
    .bind(tenant_a)
    .bind(document_id.to_string())
    .bind(tenant_a.to_string())
    .bind(document_id.to_string())
    .bind("postgres-e2e-document-1")
    .fetch_one(&pool)
    .await
    .expect("atomic rows must be queryable");
    assert_eq!(counts, (1, 1, 1, 1));

    let replayed = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/documents",
            tenant_a,
            Some(body.clone()),
        ))
        .await
        .expect("router must respond");
    assert_eq!(replayed.status(), StatusCode::OK);

    let mismatched = serde_json::json!({
        "original_filename": "other.pdf",
        "content_type": "application/pdf",
        "object_key": "other.pdf",
        "size_bytes": 99
    })
    .to_string();
    let conflict = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/documents",
            tenant_a,
            Some(mismatched),
        ))
        .await
        .expect("router must respond");
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    let own_get = router
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/v1/documents/{document_id}"),
            tenant_a,
            None,
        ))
        .await
        .expect("router must respond");
    assert_eq!(own_get.status(), StatusCode::OK);

    let cross_tenant_get = router
        .oneshot(request(
            "GET",
            &format!("/api/v1/documents/{document_id}"),
            tenant_b,
            None,
        ))
        .await
        .expect("router must respond");
    assert_eq!(cross_tenant_get.status(), StatusCode::NOT_FOUND);

    sqlx::query("DELETE FROM document_idempotency WHERE tenant_id = $1")
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("cleanup idempotency");
    sqlx::query("DELETE FROM documents WHERE tenant_id = $1")
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("cleanup documents");
    sqlx::query("DELETE FROM audit_events WHERE tenant_id = $1")
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("cleanup audit");
    sqlx::query("DELETE FROM outbox_events WHERE tenant_id = $1")
        .bind(tenant_a.to_string())
        .execute(&pool)
        .await
        .expect("cleanup outbox");
}
