# Business Platform

企业 AI 业务平台的 Rust 后端实现。系统以独立、可审计、可恢复的业务平台为核心；AI 文档处理属于平台能力，Agent/Workspace 是可选且可替换的产品入口。

## 当前状态

**v0.1 已发布**。发布基线为 PLAN-0012，annotated tag v0.1 指向 2383651。发布后主干已完成 rustls 安全补丁、MinIO 镜像源迁移和文档同步。

已具备的主要运行能力：

- business-api：真实 OIDC/JWT 认证（JWKS / issuer / audience / expiry，生产 fail-closed）、Document API、Review、Runtime Governance API、Prometheus /metrics。
- business-worker / ai-worker：固定 Document Intelligence Pipeline、Lease/Fence/Heartbeat/Retry/Crash Recovery、真实 OpenAI-compatible model provider、可观测指标。
- Document Management：Document Revision、processing binding、Artifact/Evidence 基础、PostgreSQL 权威状态、S3/MinIO 对象存储。
- Runtime Governance：Audit、Integrity Finding、Controlled Repair、Repair Ledger。
- 外部访问：REST/OpenAPI、CLI、React Business Console、窄范围 read-only MCP adapter。
- Business Application Foundation：package/contribution/compiler/dry-plan/architecture fitness 基础已集成。

尚未完整实现的关键能力：

- **用户、角色、组织与业务授权管理**：生产认证已完成，但 identity / organization 仍为骨架；当前只存在少量固定 Management Permission。
- **真实业务模块**：Contract、Finance、Approval、Legal、Project 等大部分仍为骨架。
- **Enterprise AI Workspace**：Workspace/Turn/AgentRun/Capability/Observation 尚未实现；现有 MCP adapter 不等于完整 Agent Runtime。
- Analytics/Semantic Runtime、Knowledge/RAG、通用 Workflow、Generated App Sandbox 尚未实现。

下一实施候选为 PLAN-0013 Identity and Authorization Foundation。先补齐最小企业用户/组织/角色/权限闭环，再以 Contract 真实业务垂直切片验证平台；PLAN-0006 Enterprise AI Workspace 已重写为 Revision 1，保持 Proposed，在 Identity + Contract 基础完成前不激活。

## 权威文档

文档入口为 [docs/README.md](docs/README.md)。核心文档包括：

- 架构状态：[docs/architecture/ARCHITECTURE_STATUS.md](docs/architecture/ARCHITECTURE_STATUS.md)
- 身份与授权：[docs/architecture/IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md](docs/architecture/IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md)
- 代码架构：[docs/architecture/CODE_ARCHITECTURE.md](docs/architecture/CODE_ARCHITECTURE.md)
- 持久化处理：[docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md](docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md)
- 安全：[docs/architecture/SECURITY_ARCHITECTURE.md](docs/architecture/SECURITY_ARCHITECTURE.md)
- 可观测性：[docs/architecture/OBSERVABILITY_ARCHITECTURE.md](docs/architecture/OBSERVABILITY_ARCHITECTURE.md)
- 运维：[docs/operations/RUNBOOK.md](docs/operations/RUNBOOK.md)
- 当前计划：[docs/plans/README.md](docs/plans/README.md)

## Workspace

~~~text
apps/
  business-api       对外业务 API（OIDC、业务/治理 API、/metrics）
  business-worker    文档处理 Worker（lease/fence、恢复、/metrics）
  ai-worker          AI 提取 Worker（model-provider、/metrics）
  agent-adapter      可选 read-only MCP/Agent 接入
  governance-worker  完整性/治理发现 Worker
  migration          数据库迁移工具
  business-console   React 前端

crates/
  document*          文档、Revision、持久化处理与 Evidence
  audit*             Runtime Audit
  data-integrity     Integrity Finding
  data-repair        Controlled Repair
  identity           身份/用户领域（待 PLAN-0013 实现）
  organization       组织领域（待 PLAN-0013 最小实现）
  contract/...       业务领域骨架，后续按真实垂直切片实现
  object-storage     local/S3 对象存储适配
  business-api-client / public-api-contracts / agent-integration
~~~

## 本地开发

必需依赖：Rust 1.94+。本地开发可使用 SQLite + local storage；真实多进程/生产语义以 PostgreSQL 为权威。

常用质量门禁：

~~~bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
powershell -File scripts/check-architecture.ps1
powershell -File scripts/check-openapi.ps1
~~~

## 开发约束

1. UI、OpenAPI、后台任务、CLI、MCP 和 Agent 必须复用同一 Application Use Case / Published Contract。
2. 领域层不得依赖 Axum、SQLx、Reqwest、NATS 或供应商 SDK。
3. 外部 OIDC IdP 负责人员认证与凭证签发；Business Platform 负责租户、业务授权、Policy 和 Audit。
4. Agent 不直接访问数据库，不拥有通用 Shell、SQL、文件系统或任意 HTTP 工具。
5. AI 输出是候选结果，正式业务写入必须经过确定性校验与拥有者 Application Use Case。
6. PostgreSQL 是生产权威业务状态；对象存储只保存文件本体和大对象。
7. 初期采用模块化单体；只有出现明确部署/隔离/扩缩容需求时才拆微服务。
8. 指标标签只能使用有界值，禁止 tenant/document/correlation/用户输入进入指标标签。

面向编码 Agent 的执行要求见 [AGENTS.md](AGENTS.md)。
