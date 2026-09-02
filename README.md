# Business Platform

企业 AI 业务平台的 Rust 后端实现。系统以完整、独立的业务平台为核心，AI 文档能力属于后端业务能力，Agent 是可选、可替换的自然语言入口。

## 当前状态

处于 PLAN-0012 预生产候选（v0.1 candidate）阶段，首个垂直切片为 Document Intelligence：

- `business-api`：真实 OIDC/JWT 认证（JWKS/issuer/audience 校验，生产 fail-closed）、文档上传/列表/处理链路、审计与 Outbox；`/metrics` Prometheus 端点。
- `business-worker` / `ai-worker`：固定管道 `ValidateSource → DetectType → ExtractText → ExtractFields → ValidateCandidate → AwaitReview` 的持久化执行（lease/fence/重试/恢复）、审计哈希链；生产 JSON 日志与 worker `/metrics` 端点（`observability.metrics_addr`）。
- AI 提取：`ai-worker` 通过 vendored `jarvis-model-provider`（ADR-0023）接真实 LLM，429/5xx/超时具有界重试与 `Retry-After` 钳制；`deterministic` 模式保留为离线降级与测试路径。
- `agent-adapter`：只读 MCP 工具白名单，经 typed client 调用 Business API，不直连数据库。
- `migration`：PostgreSQL 迁移工具（manifest 校验）；SQLite 仅本地单进程。
- 尚未交付：Document Search（Deferred）、PLAN-0006 Workspace、通用 Workflow/Analytics Runtime；这些不在 v0.1 范围。

v0.1 完成审计与逐项 PASS/NOT RUN 记录见 `docs/plans/current/PLAN-0012-runnable-v1-auth-ai-provider-observability.md` 与 `docs/reports/`。

## 权威文档

文档入口为 [`docs/README.md`](docs/README.md)。核心文档包括：

- 架构状态：[`docs/architecture/ARCHITECTURE_STATUS.md`](docs/architecture/ARCHITECTURE_STATUS.md)
- 代码架构规范：[`docs/architecture/CODE_ARCHITECTURE.md`](docs/architecture/CODE_ARCHITECTURE.md)
- 持久化处理架构：[`docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`](docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md)
- 部署架构：[`docs/architecture/DEPLOYMENT_ARCHITECTURE.md`](docs/architecture/DEPLOYMENT_ARCHITECTURE.md)
- 可观测性：[`docs/architecture/OBSERVABILITY_ARCHITECTURE.md`](docs/architecture/OBSERVABILITY_ARCHITECTURE.md)
- 安全架构：[`docs/architecture/SECURITY_ARCHITECTURE.md`](docs/architecture/SECURITY_ARCHITECTURE.md)
- 运维 Runbook：[`docs/operations/RUNBOOK.md`](docs/operations/RUNBOOK.md)
- Rust 编码规范：[`docs/standards/RUST_CODING_STANDARD.md`](docs/standards/RUST_CODING_STANDARD.md)

## Workspace

```text
apps/
  business-api       对外业务 API（OIDC 认证、/metrics）
  business-worker    文档处理管道 Worker（lease/fence、/metrics）
  ai-worker          AI 提取任务 Worker（model-provider、/metrics）
  agent-adapter      可选 Agent/MCP 只读接入
  governance-worker  完整性/治理发现 Worker
  migration          数据库迁移工具

crates/
  document*          文档管理与持久化处理（含 postgres/sqlite 适配器）
  identity/...       身份与租户
  object-storage     对象存储适配（local/S3）
  messaging          事件与 Outbox
  observability      日志格式、tracing 与共享 Prometheus /metrics 端点
  business-api-client / public-api-contracts / agent-integration  契约与 typed client
```

## 本地开发

必需依赖：Rust 1.94+；本地开发可用 SQLite + local storage，无需外部服务。
契约测试依赖 PostgreSQL/MinIO 的测试标记为 `#[ignore]`，CI 以 `--include-ignored` 运行。

常用质量门禁：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
powershell -File scripts/check-architecture.ps1
powershell -File scripts/check-openapi.ps1
```

可观测性：三个服务进程暴露 Prometheus 文本端点——`business-api` 在自身公共端口 `/metrics`；两个 worker 在 `observability.metrics_addr`（生产必填，开发默认关闭）。抓取配置与最小 Dashboard 见 `deploy/observability/`。备份/恢复演练：`deploy/operations/drill-backup-restore.sh`。

## 开发约束

1. UI、OpenAPI、后台任务和 Agent 必须复用同一应用服务。
2. 领域层不得依赖 Axum、SQLx、Reqwest、NATS 或具体模型 SDK。
3. Agent 不直接访问数据库，不拥有通用 Shell、SQL 或任意 HTTP 工具。
4. AI 输出是候选结果，写入业务数据前必须经过确定性校验。
5. PostgreSQL 是权威业务状态；对象存储保存文件本体。
6. 初期采用模块化单体，只有出现明确运行边界时才拆微服务。
7. 指标标签只允许代码内枚举的有界值；tenant/document/correlation/路径/模型输出不得进入标签。

面向编码 Agent 的执行要求见 [`AGENTS.md`](AGENTS.md)。
