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
ADR-0001-modular-monolith-first.md
ADR-0002-object-storage-provider.md
```

编号一经创建不得复用，即使 ADR 被拒绝。

## 当前登记

暂无已接受 ADR。总体架构中的关键决定尚未拆分为独立 ADR；首次触及对应实现时应补录，而不是复制总体架构正文。

建议优先形成：

1. 模块化单体优先；
2. PostgreSQL 作为权威状态；
3. S3 兼容对象存储及生产选型；
4. NATS JetStream 与 Outbox；
5. Agent 为可插拔入口；
6. 认证、授权与多租户模型。

模板见 [`../templates/ADR_TEMPLATE.md`](../templates/ADR_TEMPLATE.md)。

ADR 的创建、接受、替代和归档遵循 [`../governance/DOCUMENT_MANAGEMENT.md`](../governance/DOCUMENT_MANAGEMENT.md)。
