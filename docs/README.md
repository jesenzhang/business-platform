# 项目文档中心

本目录是项目文档的统一入口。文档位置、状态和权威关系由 [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) 管理。

## 1. 权威顺序

```text
总体产品与系统架构
→ 服务端架构清单
→ 服务端总体与专题架构
→ 代码架构和标准
→ 当前计划
→ 当前实现
```

发生冲突时，当前代码不能自动覆盖 Baseline；应修正实现或通过 ADR 修改架构。

## 2. 总体与服务端架构

| 类别 | 文档 | 状态 | 作用 |
|---|---|---|---|
| 总体架构 | [`../企业AI业务平台与智能助手总体架构方案_v2.md`](../企业AI业务平台与智能助手总体架构方案_v2.md) | Baseline | 产品、系统主体、Agent 与总体部署边界 |
| 完整服务端架构清单 | [`architecture/BACKEND_ARCHITECTURE_MANIFEST.md`](architecture/BACKEND_ARCHITECTURE_MANIFEST.md) | Baseline | 定义完整架构文档集、权威关系和任务准入 |
| 服务端总体架构 | [`architecture/SERVER_BACKEND_ARCHITECTURE.md`](architecture/SERVER_BACKEND_ARCHITECTURE.md) | Baseline | 战略 DDD、模块化单体、分层、数据所有权和质量属性 |
| Bounded Context Map | [`architecture/BOUNDED_CONTEXT_MAP.md`](architecture/BOUNDED_CONTEXT_MAP.md) | Baseline | 业务能力、上下文职责与协作关系 |
| 数据与一致性 | [`architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`](architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md) | Baseline | 数据所有权、事务、事件、幂等和补偿 |
| 长时任务 | [`architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`](architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md) | Baseline | 业务状态、流程协调和可靠执行边界 |
| 质量属性 | [`architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`](architecture/QUALITY_ATTRIBUTE_SCENARIOS.md) | Baseline | 性能、可用性、安全、恢复和容量验收 |
| 安全架构 | [`architecture/SECURITY_ARCHITECTURE.md`](architecture/SECURITY_ARCHITECTURE.md) | Baseline | 身份、租户、授权、文件、AI 和 Agent 安全 |
| 部署架构 | [`architecture/DEPLOYMENT_ARCHITECTURE.md`](architecture/DEPLOYMENT_ARCHITECTURE.md) | Baseline | 进程、网络、环境、发布和扩缩容 |
| 可观测性 | [`architecture/OBSERVABILITY_ARCHITECTURE.md`](architecture/OBSERVABILITY_ARCHITECTURE.md) | Baseline | 日志、指标、追踪、审计和告警 |
| 遗留迁移 | [`architecture/LEGACY_MIGRATION_ARCHITECTURE.md`](architecture/LEGACY_MIGRATION_ARCHITECTURE.md) | Baseline | 从现有系统渐进迁移与退出策略 |
| 代码架构 | [`architecture/CODE_ARCHITECTURE.md`](architecture/CODE_ARCHITECTURE.md) | Baseline | crate、层次、依赖和运行边界 |
| 架构状态 | [`architecture/ARCHITECTURE_STATUS.md`](architecture/ARCHITECTURE_STATUS.md) | Living | 当前实现符合程度和计划门禁 |

以上文档共同构成完整服务端架构，不能只选择其中一份解释系统设计。

## 3. 标准与基础设施

| 类别 | 文档 | 状态 | 作用 |
|---|---|---|---|
| API 与事件 | [`standards/API_AND_EVENT_CONTRACT_STANDARD.md`](standards/API_AND_EVENT_CONTRACT_STANDARD.md) | Baseline | 命令、查询、事件、版本、幂等和兼容 |
| 架构门禁 | [`standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`](standards/ARCHITECTURE_FITNESS_FUNCTIONS.md) | Baseline | CI 依赖检查、契约测试和发布证据 |
| Rust 编码 | [`standards/RUST_CODING_STANDARD.md`](standards/RUST_CODING_STANDARD.md) | Baseline | Rust 代码、错误、异步、测试和安全规则 |
| 查询与数据库适配 | [`standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md`](standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md) | Baseline | Query Object、Read DTO、分页、SQL/ORM 与多数据库规则 |
| 基础设施验证 | [`../企业AI业务平台基础设施开发验证与预生产方案_v1.md`](../企业AI业务平台基础设施开发验证与预生产方案_v1.md) | Baseline | 本地、测试、CI、预生产与恢复 |
| 文档治理 | [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) | Baseline | 文档目录、生命周期、变更和归档 |

## 4. 已接受架构决策

- [`adr/ADR-0001-s3-sdk-selection.md`](adr/ADR-0001-s3-sdk-selection.md)：对象存储采用 `aws-sdk-s3`。
- [`adr/ADR-0002-outbox-claim-retry.md`](adr/ADR-0002-outbox-claim-retry.md)：Outbox claim/lease/retry 设计。
- [`adr/ADR-0003-domain-driven-layered-backend.md`](adr/ADR-0003-domain-driven-layered-backend.md)：服务端采用战略 DDD、模块化单体、领域/应用/适配器分层、数据所有权、质量属性与自动架构门禁。
- [`adr/ADR-0004-rust-msrv-toolchain.md`](adr/ADR-0004-rust-msrv-toolchain.md)：Rust 1.94.1 为当前锁定依赖的最低验证工具链。
- [`adr/ADR-0008-cqrs-query-model-and-read-projections.md`](adr/ADR-0008-cqrs-query-model-and-read-projections.md)：命令/查询分离与可重建 Read Projection。
- [`adr/ADR-0009-multi-database-persistence-adapters.md`](adr/ADR-0009-multi-database-persistence-adapters.md)：PostgreSQL 生产权威与 SQLite 本地适配策略。

## 5. 文档目录

```text
docs/
├── README.md
├── governance/       治理、流程和文档制度
├── architecture/     业务、应用、数据、技术和部署架构
├── standards/        编码、测试、安全和契约规范
├── adr/              长期架构决策
├── plans/            当前执行计划与归档
├── reviews/          审查和验收记录
├── runbooks/         部署、恢复、值守和故障手册
├── reference/        外部参考与调研结论
└── templates/        标准模板
```

现有两份中文长文档暂时保留在仓库根目录，属于已登记迁移例外。后续新增长文档不得继续放在根目录。

## 6. 文档状态

- `Draft`：讨论中；
- `Proposed`：等待接受；
- `Baseline`：当前权威规则，代码必须遵循；
- `Accepted`：ADR 已接受；
- `Living`：持续反映当前状态，不替代 Baseline；
- `Superseded`：已被替代；
- `Archived`：不再参与当前决策。

## 7. 新任务使用方式

开始任何服务端任务前：

1. 阅读 `BACKEND_ARCHITECTURE_MANIFEST.md`；
2. 确认 Bounded Context 和数据所有者；
3. 明确业务不变量、用例、API/Event 和一致性；
4. 检查安全和质量属性；
5. 在计划中增加架构符合性章节；
6. 确定需要运行的 Fitness Functions；
7. 改变长期边界时新增 ADR；
8. 代码、测试和文档在同一变更中更新。

禁止从数据库表、Handler、消息 Topic 或 SDK 直接推导业务边界。

## 8. 当前实施

- 当前计划：[`plans/current/PLAN-0003-persistence-query-architecture.md`](plans/current/PLAN-0003-persistence-query-architecture.md)
- 已归档：[`plans/archive/2026/PLAN-0001-foundation-hardening.md`](plans/archive/2026/PLAN-0001-foundation-hardening.md)（`Integrated`）
- 已归档：[`plans/archive/2026/PLAN-0002-foundation-integrity-and-closeout.md`](plans/archive/2026/PLAN-0002-foundation-integrity-and-closeout.md)（`Integrated`）
- 实时架构状态：[`architecture/ARCHITECTURE_STATUS.md`](architecture/ARCHITECTURE_STATUS.md)
- 初始审查：[`reviews/2026-07-30-initial-implementation-review.md`](reviews/2026-07-30-initial-implementation-review.md)
- PLAN-0001 实施审查：[`reviews/2026-07-30-plan-0001-implementation-review.md`](reviews/2026-07-30-plan-0001-implementation-review.md)

Phase 1 Foundation Integrity 已完成。PLAN-0002 采用 local solo
fast-forward 且不创建 PR，集成 SHA 为
`ad47544505b66d577ccdcb8f300812c294d3d7bf`；main CI run 30784568762
已通过真实 PostgreSQL/MinIO、Document E2E 与架构门禁。当前由 PLAN-0003
执行 Phase 2 persistence/query preparation；PLAN-0003 当前处于
`Active — Revision 1`，尚未集成。

## 9. 合并后的后续任务规则

所有新计划和实施指令必须明确引用：

```text
docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md
docs/architecture/BOUNDED_CONTEXT_MAP.md
docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md
docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md
docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md
```

涉及安全、长时任务、部署、可观测性或迁移时，同时引用对应专题 Baseline。

功能测试通过但架构门禁失败，任务不得声明完成。

## 10. 后续运行文档

架构设计已经形成，后续按实施阶段补充具体 Runbook：

- 本地开发和故障处理；
- 预生产部署；
- 正式生产部署；
- 数据库和对象存储恢复；
- 安全事件响应；
- Provider 故障和任务恢复；
- 遗留系统切流与回滚。

Runbook 是架构的操作实现，不得修改 Baseline 语义。

## 11. 模板

- [`templates/ADR_TEMPLATE.md`](templates/ADR_TEMPLATE.md)
