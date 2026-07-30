# 架构决策记录（ADR）

本目录保存长期、跨模块或难以逆转的架构决策。

## 1. 状态

- `Proposed`
- `Accepted`
- `Rejected`
- `Superseded`
- `Deprecated`

## 2. 编号

使用连续四位编号：

```text
ADR-0001-s3-sdk-selection.md
ADR-0002-outbox-claim-retry.md
ADR-0003-domain-driven-layered-backend.md
```

编号一经创建不得复用，即使 ADR 被拒绝。

## 3. 当前登记

| 编号 | 标题 | 状态 | 说明 |
|---|---|---|---|
| [`ADR-0001`](ADR-0001-s3-sdk-selection.md) | S3 SDK 选型 | Accepted | PLAN-0001 对 S3 SDK 适配器的选型决策 |
| [`ADR-0002`](ADR-0002-outbox-claim-retry.md) | Outbox Claim 与重试 | Accepted | Outbox claim/lease/retry 设计 |
| [`ADR-0003`](ADR-0003-domain-driven-layered-backend.md) | 服务端采用领域驱动的分层架构 | Accepted | 战略 DDD、数据所有权、显式一致性、端口适配、质量属性与自动架构门禁 |

## 4. ADR-0003 的完整 Baseline

ADR-0003 不是只约束代码目录，而是由完整服务端架构文档集落实：

- `docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md`
- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/BOUNDED_CONTEXT_MAP.md`
- `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`
- `docs/architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`
- `docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`
- `docs/architecture/SECURITY_ARCHITECTURE.md`
- `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`
- `docs/architecture/LEGACY_MIGRATION_ARCHITECTURE.md`
- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`

后续实现任务必须遵循以上文档，而不是只引用 ADR 标题。

## 5. 后续 ADR 触发条件

以下变化必须创建或替代 ADR：

- 新增、合并或拆分 Bounded Context；
- 改变权威数据所有者；
- 改变跨上下文一致性和补偿模型；
- 新增独立部署单元或拆分微服务；
- 改变身份、租户和授权模型；
- 引入全局基础设施、框架或供应商；
- 改变长时任务 claim、lease、重试和恢复语义；
- 改变 API/Event 兼容性策略；
- 调整关键质量属性、RPO/RTO 或安全风险接受；
- 建立或废弃遗留迁移路径。

## 6. 决策原则

ADR 优先记录稳定的架构语义和约束，而不是把当前产品名称提升为业务核心概念。

涉及具体技术时必须区分：

```text
核心能力要求
当前适配器选择
质量属性影响
替换条件
迁移和回滚
```

## 7. 模板与治理

模板见 [`../templates/ADR_TEMPLATE.md`](../templates/ADR_TEMPLATE.md)。

ADR 的创建、接受、替代和归档遵循 [`../governance/DOCUMENT_MANAGEMENT.md`](../governance/DOCUMENT_MANAGEMENT.md)。
