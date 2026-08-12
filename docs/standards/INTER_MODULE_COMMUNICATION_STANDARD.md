# Inter-Module Communication Standard

> 文档 ID：STD-MODULE-COMM-001  
> 版本：0.1  
> 状态：Proposed input to ADR-0022  
> 日期：2026-08-12  
> 适用范围：Business Module 之间，以及 Business Module 与 Platform Capability 之间的协作

## 1. 目的与边界

本标准把跨模块协作收敛为少数可审计的机制。它不创建新的 Bounded Context、消息运行时、Durable Task Runtime 或跨模块数据库事务。

每个 Bounded Context 仍只有一个权威数据所有者。Business Module A 不得通过 Repository、私有 Domain 类型、私有表、裸表名、任意 SQL 或数据库凭证访问 Business Module B。跨模块协作必须经过本标准定义的公开边界。

```text
Domain state and invariant
        owned by Module B
              │
              ├── Published Application Query / Command
              ├── Versioned Integration Event
              ├── ResourceRef
              ├── Reference + immutable Snapshot
              ├── Published Read Projection
              └── Published Extension Point
```

### 1.1 统一语言

| 术语 | 本项目含义 | 不是 |
|---|---|---|
| Application Query | Owner Module 提供的只读用例端口 | Repository 查询或私有表访问 |
| Application Command | Owner Module 接受的业务意图 | 技术性 `UpdateRow` |
| Domain Event | Owner Context 内已发生的领域事实 | 给未知消费者的内部聚合 dump |
| Integration Event | 面向跨模块消费者的版本化事实 | Broker topic 本身 |
| ResourceRef | `module/resource-kind/resource-id` 的稳定公开引用 | private foreign key |
| Published Projection | 可重建的跨模块查询模型 | 正式业务写入权威 |
| Extension Point | Owner 主动发布、版本化、可撤销的扩展槽位 | 任意 metadata 注入 |
| Process Manager / Saga | 拥有跨模块业务过程状态的协调者 | Durable Task Execution 的技术 Job |

## 2. 选择规则

调用方先判断协作目的，再选择机制：

| 目的 | 首选机制 | 一致性 | 典型例子 |
|---|---|---|---|
| 立即读取 Owner 的当前公开状态 | Synchronous Application Query | 调用时读取，受超时约束 | 判断资源是否可操作 |
| 立即请求 Owner 做一个业务决策 | Synchronous Command | Owner 本地事务强一致；跨模块整体最终一致 | 请求创建审批 |
| 通知未知或多个消费者已经发生的事实 | Integration Event | 至少一次、最终一致 | `ContractSigned.v1` |
| 持久保存跨模块关系 | ResourceRef | 引用存在性与授权运行时校验 | Project 引用 Contract |
| 解释历史业务事实 | Reference + Snapshot | Snapshot 不可变 | 付款保存合同编号快照 |
| 供列表、报表、分析或批量查询 | Published Read Projection | 最终一致、可重建 | 合同+项目列表 |
| 协调多个业务步骤 | Process Manager / Saga | 每步本地事务、最终一致、显式补偿 | 签署后创建项目并通知 |

禁止为了获得“看起来的强一致”而共享数据库事务。若业务要求跨模块不可分割的原子不变量，应重新审查 Bounded Context 边界，而不是添加 distributed transaction。

## 3. Synchronous Application Query

### 3.1 合法调用形态

```text
Module A Application Service
  → Module B Published Query Port
  → Module B Application Layer
  → Module B owned read model/domain query
```

调用方只依赖 B 发布的版本化 Query DTO/Port。不得依赖 B 的 Repository、Aggregate、Row、SQL、ORM、私有错误类型或基础设施连接池。

### 3.2 必须声明

- query name、schema version 和兼容窗口；
- caller deadline 和 adapter timeout；未声明 timeout 的调用 fail closed；
- tenant context、principal、delegation/correlation/trace context；
- owner 授权策略和字段分类；
- `NotFound`、`Forbidden`、`Conflict`、`Unavailable`、`Timeout` 的稳定错误映射；
- 最大结果量、分页和新鲜度/版本语义；
- 允许的重试次数；Query 必须是无副作用且幂等的才可重试。

### 3.3 失败语义

超时、不可用和授权失败不得被转换为“资源不存在”以外的业务成功。调用方必须选择 fail closed、降级为明确的有界陈旧 projection，或把过程置为等待状态；不得用空对象继续写入正式业务事实。

## 4. Synchronous Command

### 4.1 所有权

Command 的业务事务由接收命令的 Owner Module 拥有。Owner 在自己的本地事务中验证授权、租户、幂等键、期望版本、不变量、Audit 和 Outbox。调用方不拥有也不控制 Owner 的事务。

### 4.2 限制

- 不使用跨 Bounded Context 的 nested transaction、共享 connection 或 2PC；
- 同步调用只用于调用方必须立即获得 Owner 决策的场景；
- 需要跨多个 Owner 完成的过程使用 Process Manager/Saga；
- Command 必须带主体、租户、目标 ResourceRef、expected version（适用时）和 Idempotency-Key；
- 授权在 Owner 重新执行，不能只信任 caller 的预检查；
- 重试只能在幂等键和请求摘要一致时进行。

### 4.3 结果

Owner 可以返回已提交的业务结果，也可以返回 `Accepted + process/job reference`。技术执行完成不代表被调用业务过程已经完成；后续状态以 Owner 查询或 Integration Event 为准。

## 5. Integration Event

### 5.1 发布链

```text
Owner Domain Event
  → owner application mapping
  → local transaction + Outbox
  → versioned Integration Event
  → broker/delivery adapter
  → independent idempotent consumers
```

Publisher 不知道具体 subscriber。消费者不能把收到事件当作同步确认，也不能直接写 Publisher 的数据。

### 5.2 Envelope 与演进

Integration Event 必须遵循 `API_AND_EVENT_CONTRACT_STANDARD.md` 的 envelope，至少包含：`event_id`、`event_type`、`schema_version`、`occurred_at`、`producer`、`correlation_id`、`causation_id`、`trace_id`、`tenant_id`、`subject_ref` 和 bounded `payload`。

- `event_id` 全局唯一；
- 消费者按至少一次交付设计，记录 Inbox/幂等结果；
- 默认不保证全局顺序；需要顺序时使用 subject/aggregate 分区和版本检测；
- 重复不得产生重复正式副作用；乱序进入等待、重建或人工处理；
- transient failure 有界重试，永久失败进入可审计 dead-letter/attention 状态；
- 新增可选字段通常兼容；改变语义、类型或必填性必须新 schema version/type；
- 大型正文、密钥、storage key、prompt 和 provider 原文不得进入公开事件。

## 6. ResourceRef

公开引用的逻辑模型为：

```text
ResourceRef {
  module_id: StableModuleId,
  resource_kind: StableResourceKind,
  resource_id: StableResourceId,
  tenant_scope: TenantScope,
}
```

序列化格式和内部 UUID/数据库 key 由公开契约单独定义；不能暴露表名、bucket、object key 或路径。

引用消费者必须在使用时处理：

- owner module 未安装、已禁用或已卸载；
- resource 不存在、已撤销或版本过期；
- tenant 不匹配；
- caller 无权读取；
- resource kind/schema version 不兼容。

ResourceRef 不证明资源仍存在，也不自动授予权限。若关系必须长期解释，ResourceRef 必须与不可变 Snapshot 组合。

## 7. Reference + Snapshot

### 7.1 必须保存 Snapshot 的场景

当历史事实必须在 Owner 后续改名、归档、删除或模块卸载后仍可解释时，保存：

```text
current/reference: ResourceRef
historical/fact: immutable business snapshot
```

典型字段包括业务编号、交易时的对方显示名称、金额/币种、合同版本或分类标签。Snapshot 必须标记来源 ResourceRef、source version、captured_at、classification 和 schema version。

### 7.2 禁止复制事实

消费者不得把 Snapshot 当作 Owner 当前状态，也不得复制可变的完整领域对象。当前决策必须回到 Owner Query/Command；历史解释使用 Snapshot。Snapshot 更新只能通过新的业务事实或明确的 owner-published snapshot policy，不能静默覆盖历史。

## 8. Published Read Projection

Projection 由明确的 owner/producer 负责 schema、事件输入、重建方式、freshness SLO、版本和权限策略。它可以由一个 Query Service/API Composition 或多个事件源构建，但不得通过跨模块 private SQL JOIN 直接形成。

Projection 必须声明：

- source module/resource/contract versions；
- tenant and classification propagation；
- freshness target and observed watermark；
- rebuild command/process and replay compatibility；
- read-only/non-authoritative status；
- missing source、deleted module 和 lag 的处理；
- 是否允许用于展示、批量筛选、报表或正式写入预检查。

Projection 不得成为正式业务状态、审批决定、计费事实或权限事实的唯一来源。

## 9. Process Manager / Saga

跨模块业务过程由一个明确的 Process Manager/Saga 所有者拥有业务协调状态。它消费版本化事件并发出 Owner Commands，每个参与上下文只提交自己的本地事务。

Saga 状态至少记录：process id/version、tenant、principal/delegation、correlation/causation、current business step、expected events、deadline、attempt/retry classification、compensation state、manual intervention state 和 audit references。它不复制参与模块的完整 domain state。

### 9.1 使用条件

- 多个 Owner 的业务步骤存在因果关系；
- 步骤之间可能等待事件、人工审批或外部系统；
- 需要超时、重试、补偿、恢复和过程可见性。

不为单一上下文的简单事务创建 Saga。

### 9.2 与 Durable Task Execution 的关系

Process Manager 拥有“业务过程状态”；Durable Task Execution 只拥有 Job/Step/Attempt/Lease/Fence/Retry/Cancel/Recovery 等可靠执行状态。两者可以由同一 worker 驱动，但不能合并为第二套 workflow runtime。消息只负责唤醒，Durable Task Store 才是执行状态权威。

### 9.3 补偿与人工审批

补偿是新的业务 Command，不是数据库 rollback。每一步声明可补偿性、补偿命令、不可逆结果和人工介入条件。人工审批通过 Owner 的业务用例进入 Saga；不能由 worker 直接改参与模块的业务状态。崩溃后从持久化过程状态和执行检查点恢复，迟到事件按 correlation、版本和 fencing 规则拒绝或转人工处理。

## 10. 安全、租户与观测

- 默认拒绝；所有 query/command/event/ref/projection 带租户边界；
- caller principal 不得通过 DTO 覆盖已认证上下文；
- classification 在跨模块传播时只能保持或收紧；
- Repository、cache、outbox、inbox、projection 和日志均保持租户隔离；
- 错误、trace 和事件不泄漏 private schema、secret、raw content 或 storage location；
- 记录 correlation/causation、contract version、latency、retry、staleness、denial 和 compensation 指标；
- 高风险写操作继续使用 Prepare → Preview → Confirm → Execute，并绑定主体、租户、资源版本与过期时间。

## 11. 架构 Fitness Functions

必须可自动或通过 contract test 证明：

1. Module A 无法依赖 Module B private repository/domain/persistence；
2. 同步协作均有 timeout、tenant、authorization、version 和 error mapping；
3. Command 事务只归 Owner，跨模块没有共享事务/2PC；
4. Integration Event 有 envelope/version/outbox/idempotent consumer；
5. ResourceRef 无 private FK/table/key；
6. Snapshot 标注 immutable/history semantics，不能取代当前 owner state；
7. Projection 标记 owner/freshness/version/rebuildability/non-authority；
8. Saga 与 Durable Task Execution 状态分离；
9. Extension Point 被删除且仍有 consumer 时 removal 为 `Blocked`；
10. 所有失败路径 fail closed 或进入显式等待/人工状态。

## 12. 相关基线

- [`DATA_OWNERSHIP_AND_CONSISTENCY.md`](../architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md)
- [`WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`](../architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md)
- [`API_AND_EVENT_CONTRACT_STANDARD.md`](API_AND_EVENT_CONTRACT_STANDARD.md)
- [`ARCHITECTURE_FITNESS_FUNCTIONS.md`](ARCHITECTURE_FITNESS_FUNCTIONS.md)
- [`ADR-0022-inter-module-communication-and-business-collaboration.md`](../adr/ADR-0022-inter-module-communication-and-business-collaboration.md)
