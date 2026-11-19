# Wave 5A Repair 1 — 70 个 ignored 到底是什么

Wave 5A 记录里写过「70 个 ignored 是既有 PostgreSQL lane」。这句话不是从测试清单得出
的，而且不成立：70 个里 41 个与 PostgreSQL 有关，28 个只要求 S3 兼容对象存储 / MinIO
(与数据库无关)，1 个要求可访问的内网模型 provider。下面这份清单由本次运行的
`cargo test --workspace --all-features` 输出（`gates/06-test-workspace.log`）逐行生成：
每条 `test ... ignored, <reason>`，包名由测试源文件解析得到。ignored 用例没有被执行，
一个都不计入通过数。

| 类别 | 数量 |
| --- | --- |
| PostgreSQL | 38 |
| object storage (S3 / MinIO) | 28 |
| PostgreSQL + object storage | 3 |
| external model provider | 1 |
| **ignored 合计** | **70** |

分类规则（按 `#[ignore]` 原因文本，依序匹配）：同时提到 PostgreSQL 与 MinIO/S3 →
PostgreSQL + object storage；提到 PostgreSQL → PostgreSQL；提到 MinIO/S3 → object storage；
其余 → external model provider。6 行写了多个包，是因为同名测试存在于多个 crate 而 cargo 日志
只标出测试*文件*不标包名；这些行的归属按「文件路径后缀 + 测试函数名 + 测试二进制名」取候选集。

| 包 | 测试目标 | 测试 | 类别 | 源码里记录的 ignore 原因 |
| --- | --- | --- | --- | --- |
| ai-worker | `unittests src\main.rs` | `extractor::tests::real_provider_smoke_extracts_candidate` | external model provider | requires a reachable intranet model provider; run manually with --ignored |
| object-storage | `tests\s3_lane_cleanup.rs` | `a_cleanup_failure_fails_the_step_and_is_not_reported_as_success` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_fault_injection.rs` | `a_failed_contract_run_is_still_cleaned_up` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_fault_injection.rs` | `a_failure_and_its_cleanup_leave_other_prefixes_untouched` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_fault_injection.rs` | `an_interrupted_multipart_upload_is_found_and_aborted` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_fault_injection.rs` | `child_entry_point` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_cleanup.rs` | `child_final_cleanup_step` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_cleanup.rs` | `cleanup_leaves_objects_outside_the_run_prefix_untouched` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_cleanup.rs` | `cleanup_never_deletes_a_protected_bucket` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_lane_cleanup.rs` | `cleanup_refuses_to_run_without_a_run_id` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\minio_contract.rs` | `content_type_is_preserved` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\s3_contract_lane.rs` | `contract_step_reports_its_scope` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\minio_contract.rs` | `delete_is_idempotent` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\minio_contract.rs` | `delete_removes_object` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\minio_contract.rs` | `exists_returns_false_for_missing` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\s3_lane_cleanup.rs` | `final_cleanup_leaves_no_objects_and_no_incomplete_uploads` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\minio_contract.rs` | `get_missing_returns_not_found` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\s3_contract_lane.rs` | `lane_delete_of_absent_object_is_idempotent` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_contract_lane.rs` | `lane_presigned_url_is_key_scoped_and_expires` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_contract_lane.rs` | `lane_records_provider_capabilities` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_contract_lane.rs` | `lane_reports_missing_object_as_not_found` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\s3_contract_lane.rs` | `lane_round_trips_object_metadata_and_payload` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\minio_contract.rs` | `large_object_is_streamed_without_small_object_buffering` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\s3_lane_fault_injection.rs` | `many_incomplete_uploads_on_one_key_are_all_found_and_aborted` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\minio_contract.rs` | `presigned_url_is_valid` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\minio_contract.rs` | `put_and_get_roundtrip` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\s3_lane_cleanup.rs` | `report_the_cleanup_scope` | object storage (S3 / MinIO) | requires a running S3-compatible provider (WAVE4_S3_* environment) |
| object-storage | `tests\minio_contract.rs` | `special_character_keys` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| object-storage | `tests\minio_contract.rs` | `user_metadata_is_preserved` | object storage (S3 / MinIO) | requires running MinIO (docker compose up) |
| audit-postgres | `tests\chain_pg.rs` | `chain_detects_mutation_and_interior_deletion_but_not_tail_truncation` | PostgreSQL | requires running PostgreSQL |
| audit-postgres | `tests\chain_pg.rs` | `concurrent_appends_form_one_verified_tenant_chain` | PostgreSQL | requires running PostgreSQL |
| audit-postgres | `tests\chain_pg.rs` | `legacy_boundary_rows_stay_readable_and_new_chain_continues_after_them` | PostgreSQL | requires running PostgreSQL |
| audit-postgres | `tests\chain_pg.rs` | `nanosecond_input_verifies_after_a_postgres_precision_round_trip` | PostgreSQL | requires running PostgreSQL |
| business-api | `tests\e2e_admin_bootstrap_postgres.rs` | `bootstrap_admin_grant_suspend_reactivate_revoke_chain_postgres` | PostgreSQL | requires running PostgreSQL (DATABASE_URL) |
| business-api | `tests\documents_postgres.rs` | `document_http_flow_is_atomic_idempotent_and_tenant_scoped` | PostgreSQL | requires running PostgreSQL |
| business-api | `tests\e2e_admin_bootstrap_postgres.rs` | `no_bootstrap_leaves_the_api_adminless_postgres` | PostgreSQL | requires running PostgreSQL (DATABASE_URL) |
| document-postgres | `tests\query_contracts.rs` | `postgres_satisfies_shared_document_persistence_contract` | PostgreSQL | requires PostgreSQL |
| document-postgres | `tests\tenant_fence_pg.rs` | `replay_never_returns_a_foreign_tenants_document` | PostgreSQL | requires PostgreSQL |
| document-processing-postgres | `tests\cancel_factuality_pg.rs` | `cancel_fact_lands_exactly_once_at_the_step_boundary` | PostgreSQL | requires PostgreSQL and migrations |
| document-processing-postgres | `tests\cancel_factuality_pg.rs` | `cancelled_failure_disposition_audits_the_fact` | PostgreSQL | requires PostgreSQL and migrations |
| document-processing-postgres | `tests\cancel_factuality_pg.rs` | `cooperative_cancel_is_intent_only` | PostgreSQL | requires PostgreSQL and migrations |
| document-processing-postgres | `tests\cancel_factuality_pg.rs` | `reclaim_of_cancelled_expired_job_publishes_the_fact` | PostgreSQL | requires PostgreSQL and migrations |
| document-processing-postgres | `tests\cancel_factuality_pg.rs` | `waiting_for_ai_cancel_publishes_fact_once` | PostgreSQL | requires PostgreSQL and migrations |
| document-processing-postgres / document-processing-sqlite / document-sqlite | `tests\contracts.rs` | `postgres_processing_contract_claims_and_restarts` | PostgreSQL | requires PostgreSQL and migrations |
| document-processing-postgres / document-processing-sqlite / document-sqlite | `tests\contracts.rs` | `postgres_processing_contract_reclaims_expired_ai_task` | PostgreSQL | requires PostgreSQL and migrations |
| document-processing-postgres / document-processing-sqlite / document-sqlite | `tests\contracts.rs` | `postgres_processing_revision_one_uow_is_atomic_and_replayable` | PostgreSQL | requires PostgreSQL and migrations |
| governance-worker | `tests\postgres_governance.rs` | `postgres_scan_and_requeue_repair_are_durable` | PostgreSQL | requires CI PostgreSQL and the shared migration catalog |
| identity-postgres | `tests\perf_resolve.rs` | `resolve_hot_path_percentiles_on_postgres` | PostgreSQL | requires PostgreSQL (set DATABASE_URL; perf evidence harness, run with --ignored --nocapture) |
| identity-postgres / identity-sqlite / organization-postgres / organization-sqlite / policy-postgres / policy-sqlite | `tests\contract.rs` | `postgres_identity_contract_suite_passes` | PostgreSQL | requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored) |
| identity-postgres / identity-sqlite / organization-postgres / organization-sqlite / policy-postgres / policy-sqlite | `tests\contract.rs` | `postgres_organization_contract_suite_passes` | PostgreSQL | requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored) |
| identity-postgres / identity-sqlite / organization-postgres / organization-sqlite / policy-postgres / policy-sqlite | `tests\contract.rs` | `postgres_policy_contract_suite_passes` | PostgreSQL | requires PostgreSQL (set DATABASE_URL; CI runs --include-ignored) |
| messaging | `tests\outbox_reliability.rs` | `concurrent_claim_100_has_unique_ownership` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\outbox_reliability.rs` | `concurrent_claim_no_duplicates` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\inbox_idempotency.rs` | `concurrent_duplicate_commits_one_side_effect` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\inbox_idempotency.rs` | `consumers_have_independent_idempotency_scopes` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\outbox_reliability.rs` | `deterministic_claim_order` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\outbox_reliability.rs` | `expired_lease_recovery` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\outbox_reliability.rs` | `failure_schedules_retry` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\outbox_reliability.rs` | `max_attempts_reached` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\inbox_idempotency.rs` | `rollback_releases_marker_and_side_effect` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\inbox_idempotency.rs` | `sequential_duplicate_commits_one_side_effect` | PostgreSQL | requires running PostgreSQL |
| messaging | `tests\outbox_reliability.rs` | `stale_worker_cannot_publish_or_fail` | PostgreSQL | requires running PostgreSQL |
| migration | `tests\migration_test.rs` | `apply_migrations_from_empty` | PostgreSQL | requires running PostgreSQL |
| migration | `tests\migration_test.rs` | `document_integrity_upgrade_fails_closed_for_invalid_legacy_rows` | PostgreSQL | requires running PostgreSQL |
| migration | `tests\migration_test.rs` | `document_integrity_upgrade_preserves_legal_legacy_rows_and_enforces_constraints` | PostgreSQL | requires running PostgreSQL |
| migration | `tests\migration_test.rs` | `migrations_are_idempotent` | PostgreSQL | requires running PostgreSQL |
| policy-postgres | `tests\perf_authorize.rs` | `authorize_hot_path_percentiles_on_postgres` | PostgreSQL | requires PostgreSQL (set DATABASE_URL; perf evidence harness, run with --ignored --nocapture) |
| business-api | `tests\plan_0008_postgres_minio.rs` | `plan_0008_postgres_minio_multipart_upload_is_idempotent` | PostgreSQL + object storage | requires real PostgreSQL, PLAN-0008 migrations, and MinIO |
| business-api | `tests\plan_0008_postgres_minio.rs` | `plan_0008_postgres_minio_revision_evidence_contract` | PostgreSQL + object storage | requires real PostgreSQL, PLAN-0008 migrations, and MinIO |
| business-worker | `tests\postgres_minio_processing.rs` | `postgres_minio_processing_adapter_round_trip` | PostgreSQL + object storage | requires PostgreSQL, migrations, and MinIO |
