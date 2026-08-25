# PLAN-0007 Completion Audit

Document ID: REPORT-PLAN-0007-COMPLETION-AUDIT  
Status: Final  
Date: 2026-08-25  
Scope: PLAN-0007 completion-verification and archival only (PLAN-0012 Milestone M0). No business
code, `Cargo.toml`, migration or `openapi.json` was modified.

## Result

PLAN-0007 is `Integrated / Archived` on `main`, via a docs-only archival commit.

| Identity | SHA / run |
| --- | --- |
| Base / implementation SHA (milestone start, where all PLAN-0007 deliverables live) | `ec6cff141a89dcdf5de2f2ea2b8b001384f88755` |
| Archival type | docs-only (audit report + plan archival + status/README sync) |

All six completion-definition items and all six delivery slices are verified PASS or NOT RUN with
an explicit environment reason. No gate failed for a code reason and no code change was required to
pass audit.

## Completion-definition verification

PLAN-0007 完成定义（计划文档第 73-78 行）逐项记录如下。

| # | 完成定义项 | 判定 | 证据 |
|---|---|---|---|
| 1 | React 通过真实 API 完成上传→创建 Processing→查看候选→Review | PASS（浏览器端到端 NOT RUN） | console 视图与 REST 客户端：[pages.tsx](file:///f:/Workspace/business-platform/apps/business-console/src/pages.tsx)、[api.ts](file:///f:/Workspace/business-platform/apps/business-console/src/api.ts)；[api.test.ts](file:///f:/Workspace/business-platform/apps/business-console/tests/api.test.ts)（2 tests PASS）；API 路由 [mod.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/mod.rs)（`/api/v1/documents` 上传、`processing-jobs` list/start/candidate/review、`operations/overview`）、[processing.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/processing.rs)（`review_candidate` 含 candidate version 乐观锁）、[upload.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/upload.rs）。上连接链路测试见 [documents.rs](file:///f:/Workspace/business-platform/apps/business-api/tests/documents.rs)（`multipart_upload_is_idempotent_and_does_not_expose_object_key`、`upload_compensates_object_storage_when_document_persistence_fails`）。Playwright smoke [smoke.spec.ts](file:///f:/Workspace/business-platform/apps/business-console/e2e/smoke.spec.ts) 存在但本地 NOT RUN（见门禁表）。 |
| 2 | CLI 通过远程 API 输出稳定 JSON/table | PASS | [main.rs](file:///f:/Workspace/business-platform/apps/business-cli/src/main.rs)：`status`、`documents`、`processing`、`candidate`、`findings`、`audit`，支持 `--json`/`--table`；命令参数单元测试随 `cargo test` PASS。 |
| 3 | MCP discover 并调用固定 read-only tools | PASS | [main.rs](file:///f:/Workspace/business-platform/apps/agent-adapter/src/main.rs)：HTTP MCP `protocolVersion "2026-07-28"`；`tools/list` + `tools/call`；read-only allow-list（`document.list/get`、`document.processing.list/get`、`document.candidate.get`、`operations.overview`、`governance.findings.list`、`governance.finding.get`、`audit.events.list`、`audit.event.get`）；无写工具，参数 allow-list 强制校验；MCP 不接受 tenant 参数（使用可信 principal）。 |
| 4 | 三者共享 tenant/权限/业务事实 | PASS | business-api 安全/隔离测试：[security.rs](file:///f:/Workspace/business-platform/apps/business-api/tests/security.rs)（`dev_auth_with_valid_token_and_tenant_passes_auth`、`forged_permission_header_cannot_read_integrity_findings`、`dev_auth_without_client_tenant_header_uses_trusted_principal`）、[documents.rs](file:///f:/Workspace/business-platform/apps/business-api/tests/documents.rs)（`cross_tenant_get_is_not_found`）。CLI/MCP/React 复用同一 Business API。 |
| 5 | 所有门禁有明确 PASS 或带原因 NOT RUN | PASS | 见下方门禁结果表。 |

## Delivery-slice verification

| 切片 | 交付物 | 判定 | 证据 |
|---|---|---|---|
| 1 | `public-api-contracts`、`business-api-client`、版本化 `openapi.json` | PASS | [crates/public-api-contracts](file:///f:/Workspace/business-platform/crates/public-api-contracts)、[crates/business-api-client](file:///f:/Workspace/business-platform/crates/business-api-client)、根 [openapi.json](file:///f:/Workspace/business-platform/openapi.json)；`check-openapi.ps1` PASS；二者随 workspace tests PASS。 |
| 2 | Business API upload、processing list、operations overview、公共 DTO 映射 | PASS | 路由 [mod.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/mod.rs)、[upload.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/upload.rs)、[processing.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/processing.rs)、[operations.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/operations.rs)、[public_dto.rs](file:///f:/Workspace/business-platform/apps/business-api/src/routes/public_dto.rs)；`overview_fails_closed_when_processing_service_is_unavailable`、`document_http_responses_never_expose_storage_locations` PASS。 |
| 3 | `apps/business-console`：Dashboard/Documents/Document Detail/Processing/Candidate Review/Integrity/Repairs/Audit | PASS | [pages.tsx](file:///f:/Workspace/business-platform/apps/business-console/src/pages.tsx)；lint/typecheck/test/build 全 PASS；Playwright smoke 本地 NOT RUN。 |
| 4 | `business-cli`：status/documents/processing/candidate/findings/audit | PASS | [main.rs](file:///f:/Workspace/business-platform/apps/business-cli/src/main.rs)。 |
| 5 | `agent-adapter` HTTP MCP 协议 `2026-07-28`、固定 read-only tools | PASS | 见完成定义项 3 证据。 |
| 6 | Demo compose、确定性 seed/provider 边界、MCP client 配置、一键脚本 | PASS（端到端运行 NOT RUN） | [deploy/demo/docker-compose.yml](file:///f:/Workspace/business-platform/deploy/demo/docker-compose.yml)、[scripts/demo-up.ps1](file:///f:/Workspace/business-platform/scripts/demo-up.ps1)、[demo-down.ps1](file:///f:/Workspace/business-platform/scripts/demo-down.ps1)、[demo-reset.ps1](file:///f:/Workspace/business-platform/scripts/demo-reset.ps1)、[demo-seed.ps1](file:///f:/Workspace/business-platform/scripts/demo-seed.ps1)；实机 demo 运行需 PostgreSQL/MinIO，本机 NOT RUN（CI 覆盖）。 |

## Gate results

| 门禁 | 结果 | 说明 |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | exit 0 |
| `cargo check --workspace --all-targets --all-features` | PASS | exit 0（仅有 hard-link 缓存提示） |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | exit 0 |
| `cargo test --workspace --all-features` | PASS | exit 0，0 failed；34 个 PostgreSQL/MinIO `#[ignore]` 测试 NOT RUN |
| `pwsh ./scripts/check-architecture.ps1` | PASS | `Cargo metadata architecture fitness: PASS`；`OpenAPI contract: PASS`；`Architecture fitness: PASS` |
| `pwsh ./scripts/check-openapi.ps1` | PASS | `OpenAPI contract: PASS` |
| `apps/business-console` `npm run lint` | PASS | exit 0 |
| `apps/business-console` `npm run typecheck` | PASS | exit 0 |
| `apps/business-console` `npm test` | PASS | 2 tests passed |
| `apps/business-console` `npm run build` | PASS | exit 0（仅 chunk-size 信息性提示） |
| `apps/business-console` Playwright smoke（`npm run test:e2e` / `e2e/smoke.spec.ts`） | NOT RUN | Trae 沙箱阻止 Playwright 浏览器安装：写入 `%LOCALAPPDATA%\ms-playwright` 缓存目录报 `EPERM`，工作区内部浏览器路径因 `__dirlock` 校验失败。GitHub CI（Linux）Playwright job 覆盖。 |
| OpenAPI JSON parse 与敏感字段回归 | PASS | `check-openapi.ps1` + `document_http_responses_never_expose_storage_locations` + MCP DTO 测试覆盖 |

## NOT RUN items and reasons

- **34 个 PostgreSQL/MinIO `#[ignore]` 集成测试**：本机长期无 PostgreSQL/MinIO。GitHub CI（Linux）为这些用例的验收证据（与 PLAN-0008 closeout 及后续计划口径一致）。
- **Playwright smoke（`e2e/smoke.spec.ts`）**：受沙箱限制无法安装 Chromium（`EPERM` 于 ms-playwright 缓存目录、工作区内 `__dirlock` 校验失败）。由 GitHub CI（Linux）Playwright job 覆盖。
- **Demo 一键端到端运行（`demo-up.ps1` → console/CLI/MCP 直连真实栈）**：需要 PostgreSQL/MinIO 运行环境，本机 NOT RUN。
- **外部安全扫描（cargo-audit/cargo-deny/gitleaks/trivy/syft/grype/osv-scanner）**：环境未配置，长期记录为已知缺口（与 `ARCHITECTURE_STATUS.md` 现口径一致）。

## Accepted risks

- 全端到端链路（真实浏览器 × 真实 Business API × PostgreSQL/MinIO）未在本地同屏验证，依赖 GitHub CI 单点覆盖。
- Demo 一键脚本未被本机实机执行验证。
- 本里程碑只做文档收尾与验证，未新增任何业务代码，未触碰正式业务事实。

## Follow-up

PLAN-0007 已从 `docs/plans/current/` 移至 `docs/plans/archive/2026/`，状态为 `Integrated / Archived`。
`docs/plans/README.md` 与 `docs/architecture/ARCHITECTURE_STATUS.md` 已同步。PLAN-0012（Runnable v0.1）
为当前唯一 Active 计划，M0 收尾完成，M1（model-provider 集成决策与 ADR-0023）可从干净起点启动。