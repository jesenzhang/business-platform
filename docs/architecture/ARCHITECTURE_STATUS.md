# 架构实施状态

> 文档类型：Living Document  
> 最后更新：2026-07-30  
> 当前阶段：Phase 0 → 基础服务加固  
> 当前计划：`PLAN-0001-foundation-hardening`  
> 当前架构 PR：`#2 docs/ddd-backend-architecture`

## 1. 当前权威结论

- Rust 业务平台是系统主体，Agent 是可选入口；
- 服务端采用战略 DDD 主导的模块化单体；
- 服务端内部采用 Domain、Application、Delivery、Infrastructure 和 Composition Root 分层；
- 业务能力、统一语言和数据所有权决定 Bounded Context；
- 核心层只表达业务和通用能力语义；
- 基础设施产品通过适配器、配置、部署和 ADR 接入；
- 战术 DDD 按复杂度采用，不对简单 CRUD 过度建模；
- 每份可变权威业务数据只有一个拥有上下文；
- 同一上下文内优先本地事务，跨上下文通过事件、幂等、Process Manager 和补偿协作；
- 长时任务区分业务状态、业务流程状态、人工工作流和执行机制状态；
- API、事件、安全、质量属性、部署和可观测性属于正式架构资产；
- Web、Worker、OpenAPI 和 Agent 复用 Application 用例；
- 后续任务必须通过架构 Fitness Functions 提供持续符合证据。

## 2. 完整服务端架构文档集

入口：

- `docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md`

总体与专题 Baseline：

- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/BOUNDED_CONTEXT_MAP.md`
- `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`
- `docs/architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`
- `docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`
- `docs/architecture/SECURITY_ARCHITECTURE.md`
- `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`
- `docs/architecture/LEGACY_MIGRATION_ARCHITECTURE.md`
- `docs/architecture/CODE_ARCHITECTURE.md`

标准：

- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`
- `docs/standards/RUST_CODING_STANDARD.md`

决策：

- `docs/adr/ADR-0003-domain-driven-layered-backend.md`

## 3. 当前实现状态

当前仓库仍是 Phase 0 工程骨架，完整架构已经定义，但代码尚未完全符合。

已具备：

- `apps/*` 和领域/能力 crate 初始划分；
- Domain/Application/Infrastructure/API 的设计方向；
- 统一配置、错误、对象存储和消息骨架；
- Outbox 概念；
- 完整服务端架构 Baseline 和文档治理。

仍需收敛：

- `shared-kernel` 仍存在框架和数据库依赖；
- API 组合状态仍直接公开数据库连接；
- 部分端口、适配器和核心模型尚未物理分离；
- 当前领域 crate 尚未按 Bounded Context Map 完成统一语言和数据所有权落实；
- `workflow` 仍未实现 Durable Task Execution 核心；
- Worker、Migration 和 Agent Adapter 仍处于骨架阶段；
- API/Event 契约尚未全部形成可生成 Schema；
- 质量属性尚未形成性能和恢复证据；
- 架构依赖规则尚未进入自动化 CI；
- 生产 Runbook 尚未完成。

## 4. PLAN-0001 实施约束

`PLAN-0001` 已在实施中，其实现分支在合并前必须同步本架构 PR，并满足：

1. 首个 document metadata 垂直切片属于 Document Management Context；
2. Document Management 拥有文档身份、版本和元数据；
3. 不在该切片中提前让 Document Management 拥有 OCR、抽取或任务执行状态；
4. Domain 和 Application 不依赖具体 Delivery/Infrastructure；
5. Application 用例定义权限、租户、版本、幂等、事务和审计意图；
6. Infrastructure 类型、错误和 DTO 不向核心泄漏；
7. API 按契约规范实现认证、错误、版本、幂等和乐观锁；
8. 数据写入、Audit 和 Outbox 的一致性符合数据架构；
9. Local/S3 Adapter 符合安全、流式和契约测试要求；
10. Outbox 可靠性实现不被写成业务领域规则；
11. CI 增加初版架构依赖门禁；
12. PR 提供架构符合性、质量属性和安全验证结果。

## 5. 后续任务强制规则

PLAN-0001 之后所有计划必须包含：

- 目标 Bounded Context；
- 数据所有者；
- 业务不变量；
- Commands、Queries、API 和 Events；
- 事务、一致性、幂等和补偿；
- 安全与数据分类；
- 质量属性；
- 部署和可观测性影响；
- Fitness Functions；
- 文档和 ADR 更新。

缺少这些内容的计划不能直接进入实现。

## 6. 架构适配门禁

当前最低门禁：

- Domain 不依赖 Delivery、Infrastructure 和供应商实现；
- Application 不依赖具体 Adapter；
- Handler 和 Worker 入口不承载业务规则；
- 基础设施错误和 DTO 不向核心泄漏；
- 业务用例能以 Fake/In-Memory Ports 运行；
- 适配器有真实依赖契约测试；
- 跨上下文没有直接写入对方私有数据；
- 新增长时任务区分业务状态和执行状态；
- API/Event 兼容性有测试；
- 安全和租户边界 fail-closed；
- 架构相关文档与代码同步。

## 7. 当前判定

```text
完整服务端架构 Baseline：已形成，待 PR #2 合并
业务能力和 Context Map：已形成初始 Baseline
数据所有权和一致性：已形成 Baseline
长时任务架构：已形成 Baseline，代码尚未实现
API/Event 契约：已形成 Baseline，Schema 尚待落地
安全架构：已形成 Baseline，代码部分符合
质量属性：已形成初始目标，运行证据尚待建立
部署和可观测性：已形成 Baseline，Runbook 尚待建立
遗留迁移：已形成 Baseline，具体切片尚待计划
代码骨架：已存在
分层依赖：部分符合
基础设施隔离：部分符合
自动化架构门禁：尚未实现
PLAN-0001：实施中
```

## 8. 下一次更新条件

出现以下事件时更新本文：

- PR #2 合并；
- PLAN-0001 形成 PR；
- 首个垂直切片通过架构验收；
- 架构适配测试进入 CI；
- Bounded Context 或数据所有权调整；
- 新增部署单元或重大基础设施；
- 质量属性目标被实测或调整；
- PLAN-0001 完成并归档；
- 开始第一个遗留业务迁移切片。
