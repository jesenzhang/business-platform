# 架构决策记录（ADR）

本目录保存长期、跨模块或难以逆转的架构决策。

## 状态

- `Proposed`
- `Accepted`
- `Rejected`
- `Superseded`
- `Deprecated`

## 编号

使用连续四位编号：

```text
ADR-0001-s3-sdk-selection.md
ADR-0002-outbox-claim-retry.md
ADR-0003-domain-driven-layered-backend.md
```

编号一经创建不得复用，即使 ADR 被拒绝。

## 当前登记

| 编号 | 标题 | 状态 | 说明 |
|---|---|---|---|
| ADR-0001 | S3 SDK 选型 | Reserved / PLAN-0001 | 正在实施的 PLAN-0001 预留，合并前必须补齐文件和登记 |
| ADR-0002 | Outbox Claim 与重试 | Reserved / PLAN-0001 | 正在实施的 PLAN-0001 预留，合并前必须补齐文件和登记 |
| [`ADR-0003`](ADR-0003-domain-driven-layered-backend.md) | 服务端采用领域驱动的分层架构 | Accepted | 战略 DDD、模块化单体、领域/应用/适配器分层和基础设施独立性 |

后续建议形成：

1. PostgreSQL 或其他权威状态存储的角色边界；
2. 生产对象存储产品选型；
3. Agent 为可插拔入口；
4. 认证、授权与多租户模型；
5. Bounded Context Map 和跨上下文协作策略；
6. 长时任务核心与业务过程的边界。

## 决策原则

ADR 应优先记录稳定的架构语义和约束，而不是把当前产品名称提升为业务核心概念。

需要记录具体技术时，应明确区分：

```text
核心能力要求
当前适配器选择
替换条件
迁移和回滚
```

模板见 [`../templates/ADR_TEMPLATE.md`](../templates/ADR_TEMPLATE.md)。

ADR 的创建、接受、替代和归档遵循 [`../governance/DOCUMENT_MANAGEMENT.md`](../governance/DOCUMENT_MANAGEMENT.md)。
