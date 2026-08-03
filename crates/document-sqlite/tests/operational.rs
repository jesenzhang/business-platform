use sqlx::migrate::MigrateDatabase;

#[tokio::test]
async fn file_database_uses_wal_and_busy_timeout_and_survives_reopen() {
    let path = std::env::temp_dir().join(format!("business-platform-{}.db", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", path.to_string_lossy().replace('\\', "/"));
    assert!(sqlx::Sqlite::create_database(&url).await.is_ok());
    let pool = document_sqlite::connect(&url, 1).await;
    let Ok(pool) = pool else { unreachable!() };
    assert!(document_sqlite::MIGRATOR.run(&pool).await.is_ok());
    let journal_mode = sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await;
    let busy_timeout = sqlx::query_scalar::<_, i64>("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await;
    assert_eq!(journal_mode.ok().as_deref(), Some("wal"));
    assert_eq!(busy_timeout.ok(), Some(5_000));
    let invalid_state = sqlx::query(
        "INSERT INTO documents (id, tenant_id, original_filename, content_type, object_key, status, version, created_by, created_at, updated_at) VALUES ('bad', 'tenant', 'bad.pdf', 'application/pdf', 'bad.pdf', 'unknown', 1, 'user', '2026-08-03T00:00:00Z', '2026-08-03T00:00:00Z')",
    ).execute(&pool).await;
    assert!(invalid_state.is_err());
    pool.close().await;

    let reopened = document_sqlite::connect(&url, 1).await;
    let Ok(reopened) = reopened else {
        unreachable!()
    };
    let table_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'documents'",
    )
    .fetch_one(&reopened)
    .await;
    assert_eq!(table_count.ok(), Some(1));
    reopened.close().await;
    let _ = std::fs::remove_file(path);
}
