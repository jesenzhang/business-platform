# Business Platform

企业 AI 业务平台的 Rust 后端实现。系统以完整、独立的业务平台为核心，AI 文档能力属于后端业务能力，Agent 是可选、可替换的自然语言入口。

## 当前状态

仓库处于初始骨架阶段：

- 已建立 Rust Workspace、应用进程和领域 crate 边界；
- `business-api` 已具备基础配置加载、PostgreSQL 连接和健康检查；
- 对象存储、消息、可观测性和共享类型已有初始接口；
- 业务领域、Worker、Migration、Agent Adapter 仍以占位实现为主；
- 当前实现不能视为可生产运行版本。

实现评审见 [`docs/reviews/2026-07-30-initial-implementation-review.md`](docs/reviews/2026-07-30-initial-implementation-review.md)。

## 权威文档

文档入口为 [`docs/README.md`](docs/README.md)。核心文档包括：

- 总体架构：[`企业AI业务平台与智能助手总体架构方案_v2.md`](企业AI业务平台与智能助手总体架构方案_v2.md)
- 基础设施开发验证：[`企业AI业务平台基础设施开发验证与预生产方案_v1.md`](企业AI业务平台基础设施开发验证与预生产方案_v1.md)
- 代码架构规范：[`docs/architecture/CODE_ARCHITECTURE.md`](docs/architecture/CODE_ARCHITECTURE.md)
- Rust 编码规范：[`docs/standards/RUST_CODING_STANDARD.md`](docs/standards/RUST_CODING_STANDARD.md)
- 文档管理规范：[`docs/governance/DOCUMENT_MANAGEMENT.md`](docs/governance/DOCUMENT_MANAGEMENT.md)

## Workspace

```text
apps/
  business-api       对外业务 API
  business-worker    业务工作流与领域事件 Worker
  ai-worker          AI 异步任务 Worker
  agent-adapter      可选 Agent/MCP 接入
  migration          数据库迁移工具

crates/
  shared-kernel      最小化、稳定的跨领域基础类型
  identity/...       领域模块
  workflow           工作流抽象
  ai-application     AI 应用能力
  agent-integration  Agent 集成模型
  object-storage     对象存储适配
  messaging          事件与 Outbox
  observability      可观测性
```

## 本地开发

当前代码需要 PostgreSQL 配置才能启动 `business-api`。本地 PostgreSQL、MinIO 和 NATS 的目标环境与验证策略见基础设施方案。

常用质量门禁：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

在数据库迁移、对象存储和集成测试基座完成前，上述命令不足以证明基础设施行为正确。

## 开发约束

1. UI、OpenAPI、后台任务和 Agent 必须复用同一应用服务。
2. 领域层不得依赖 Axum、SQLx、Reqwest、NATS 或具体模型 SDK。
3. Agent 不直接访问数据库，不拥有通用 Shell、SQL 或任意 HTTP 工具。
4. AI 输出是候选结果，写入业务数据前必须经过确定性校验。
5. PostgreSQL 是权威业务状态；对象存储保存文件本体。
6. 初期采用模块化单体，只有出现明确运行边界时才拆微服务。

面向编码 Agent 的执行要求见 [`AGENTS.md`](AGENTS.md)。
