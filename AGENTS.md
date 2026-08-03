# AGENTS.md

本文件定义 Codex 及其他编码 Agent 在本仓库中的默认执行边界。所有后续实现任务必须遵循完整服务端架构 Baseline，而不是只参考当前代码。

## 1. 权威顺序

发生冲突时按以下顺序执行：

1. 用户当前明确指令；
2. 已接受的 ADR；
3. `docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md`；
4. `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`；
5. 各专题架构 Baseline；
6. `docs/architecture/CODE_ARCHITECTURE.md`；
7. `docs/standards/*`；
8. 当前执行计划；
9. 当前实现。

当前实现与权威文档冲突时，不应把现状自动解释为正确设计。

## 2. 必读架构文档

开始服务端功能、重构、基础设施或迁移任务前，按范围阅读：

- `docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md`
- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/BOUNDED_CONTEXT_MAP.md`
- `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`
- `docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`
- `docs/architecture/SECURITY_ARCHITECTURE.md`
- `docs/architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`，涉及后台或长任务时
- `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`，涉及进程、配置或运行时变更时
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`
- `docs/architecture/LEGACY_MIGRATION_ARCHITECTURE.md`，涉及现有系统迁移时
- `docs/architecture/CODE_ARCHITECTURE.md`
- `docs/architecture/PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`，涉及持久化、查询或数据库适配时
- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`
- `docs/standards/RUST_CODING_STANDARD.md`
- `docs/standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md`，涉及查询模型或数据库适配时

## 3. 架构预检

任何新增能力进入编码前必须明确：

1. 目标业务能力和 Bounded Context；
2. 统一语言和业务不变量；
3. 权威数据所有者；
4. 业务状态、流程状态和执行状态的归属；
5. 公开 Application Commands/Queries；
6. 同步 API、异步事件或 Anti-Corruption Layer；
7. 事务、幂等、并发、重试和补偿；
8. 身份、租户、授权和数据分类；
9. 需要满足的质量属性场景；
10. 测试、架构门禁和文档更新。

关键项无法确定时，先完成设计和计划，不得直接从表结构、Handler 或 SDK 开始实现。

## 4. 默认工作方式

- 修改前阅读目标 crate 的 `Cargo.toml`、`src/lib.rs`、相关计划和权威架构文档。
- 以完整业务能力或明确平台能力为修改单位。
- 优先在现有模块化单体边界内完成，不随意新增微服务。
- 不以“未来可能复用”为理由提前抽象。
- 不修改与任务无关的文件。
- 新增 TODO 必须关联明确计划、Issue 或阶段。
- 当前任务如改变上下文、数据所有权、部署边界或公开契约，必须同步 ADR。

## 5. DDD 与业务边界

- 战略 DDD 强制采用，战术 DDD 按复杂度选择。
- 业务边界由业务能力、统一语言、业务不变量和数据所有权定义。
- 不按数据库表、页面、Controller、消息 Topic、基础设施产品或 Agent Skill 划分领域。
- 每份可变权威数据必须只有一个 Bounded Context 拥有。
- 其他上下文不得直接写入拥有者私有数据。
- 简单查询和配置保持轻量，不机械创建空洞 Aggregate、Repository 和 Domain Service。

## 6. 分层与依赖倒置

- Domain 表达业务规则、状态和不变量。
- Application 表达用例、权限、事务意图、幂等和跨端口协调。
- Delivery 只进行协议转换和调用上下文提取。
- Infrastructure 实现数据库、消息、存储和外部能力适配。
- Composition Root 选择并注入具体实现。
- 外层依赖内层，具体实现依赖核心定义的抽象。
- 核心层不得依赖 Delivery、Infrastructure、`apps/*` 或供应商实现。
- `shared-kernel` 必须保持小而稳定，不得成为跨领域杂物箱。

## 7. 数据与一致性

- 每个 Bounded Context 拥有自己的数据模型和写入规则。
- 单一聚合和单一上下文内优先本地事务。
- 跨上下文默认使用 Application API、领域/集成事件、幂等、Process Manager 和补偿。
- 不使用共享数据库事务掩盖上下文边界。
- 正式写入必须具有租户、权限、版本和幂等策略。
- 消费者按至少一次交付设计并处理重复、乱序和重放。
- Read Model 可重建，不得成为正式写入权威。
- 文件与业务元数据通过状态机、checksum、补偿和一致性扫描协调。

## 8. 工作流和长时任务

- 业务领域拥有业务过程状态。
- Durable Task Execution 拥有任务、步骤、租约、重试、取消和恢复状态。
- Job Completed 不自动等于业务完成。
- Worker 不直接修改其他上下文正式业务数据。
- 高成本步骤具有持久化检查点。
- claim/lease 需要多 Worker 安全和 fencing 语义。
- 消息用于唤醒和分发，不是权威任务状态。
- 不使用裸 `tokio::spawn` 作为唯一可靠长任务机制。

## 9. API 与事件

- 命令表达意图，事件表达已经发生的事实。
- API/Event DTO 与 Domain Model、数据库 Row、供应商 DTO 分离。
- 公开协议必须版本化并有兼容策略。
- 可重试写命令支持幂等键。
- 重要更新使用乐观锁。
- 事件包含唯一 ID、版本、租户、关联和因果信息。
- 事件消费者必须处理重复和乱序。
- 外部回调通过签名、重放保护和应用用例验证。

## 10. 安全边界

- 默认拒绝，生产配置 fail-closed。
- 不提交真实密钥、Token、密码、生产 URL 或未脱敏数据。
- 身份、租户和权限在调用 Application 前建立。
- Repository、ObjectKey、Cache、Event 和日志均保持租户隔离。
- 外部 AI、OCR、文档、消息和 Tool 输出一律视为不可信输入。
- 不新增通用 SQL、Shell、文件系统或任意 HTTP Agent 工具。
- 高风险写操作使用 Prepare → Preview → Confirm → Execute，并绑定主体、租户、资源版本和过期时间。
- 错误响应、日志和 trace 不得泄漏底层错误、连接字符串、Secret 或敏感正文。

## 11. 质量属性

实现必须说明对以下目标的影响：

- 性能和容量；
- 可用性和故障隔离；
- 幂等、恢复和 RPO/RTO；
- 安全和多租户；
- 可维护性和可替换性；
- 可观测性和运维成本；
- API/Event 兼容性。

不得使用“高性能”“高可用”代替可测量验收。

## 12. 测试与架构门禁

提交前至少运行：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

同时按变更范围执行：

- Domain 纯单元测试；
- Application + Fake Ports；
- 真实数据库、对象存储、消息和 Provider 契约测试；
- API 安全、幂等、版本和兼容测试；
- 长任务崩溃、重复、重试和恢复测试；
- `scripts/check-architecture.ps1` 或等价架构适配检查；
- Secret、漏洞、许可证和镜像扫描。

无法运行时明确记录 `NOT RUN` 或 `BLOCKED`，不得声称通过。

## 13. 计划和 PR 要求

计划必须包含：

- 目标 Bounded Context 和数据所有者；
- 架构影响；
- API/Event；
- 一致性和安全；
- 质量属性；
- Fitness Functions；
- 文档同步；
- 回滚和完成定义。

PR 必须说明：

- 哪些架构文档适用；
- 是否改变边界、所有权或契约；
- 运行了哪些门禁；
- 未完成项和接受风险。

功能测试通过但架构门禁失败，任务仍未完成。

## 14. 文档同步

以下变更必须在同一变更中更新文档：

- 新增、合并或拆分 Bounded Context；
- 修改数据所有权；
- 修改 API、事件 Schema、错误码或回调；
- 修改事务、幂等、重试、补偿和恢复；
- 新增部署单元或基础设施；
- 修改身份、租户、权限和数据分类；
- 修改质量属性和 SLO；
- 修改迁移、备份、恢复和 Runbook。

文档生命周期遵循 `docs/governance/DOCUMENT_MANAGEMENT.md`。

## 15. PLAN-0003 Revision 1 persistence rules

PLAN-0003 Revision 1 的 Document Management 参考切片必须遵循以下额外门禁：

- `DocumentMetadata` 聚合字段保持私有；持久化适配器只能通过经过校验的
  `rehydrate` 恢复，不能直接构造聚合；生命周期变更必须校验状态并递增版本。
- HTTP 响应不得返回 `object_key`、`storage_key`、bucket 名称或内部文件路径；
  列表游标必须是版本化、不可依赖数据库字段的 opaque token。
- Document Search 当前为 Deferred；不得保留不完整 Search Port 或适配器。后续
  搜索应评估 PostgreSQL full-text/`pg_trgm` 或独立搜索索引。
- SQLite 仅用于本地单进程；连接池上限为 4，写入使用显式 single-writer 语义，
  并验证同 key 幂等、指纹冲突、审计/Outbox 原子性和重启重放。
- LIKE 文本必须转义 `\\`、`%`、`_` 并声明 `ESCAPE '\\'`；契约只保证 ASCII
  大小写不敏感，不声明完整 Unicode 大小写等价。
- PostgreSQL 测试在本地无数据库时使用带原因的 `#[ignore]`，CI 必须
  `--include-ignored`；迁移 status 只可忽略明确的 migration table 缺失，其他
  权限、连接和数据库错误必须失败。
