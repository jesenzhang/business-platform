# 数据所有权与一致性架构

> 文档 ID：ARCH-DATA-001  
> 版本：1.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：业务数据、事务、事件、缓存、文件和跨上下文协作

## 1. 目的

本文定义：

- 哪个 Bounded Context 拥有哪类权威数据；
- 哪些操作要求强一致；
- 哪些协作采用最终一致；
- 如何处理并发、幂等、重复消息、补偿和数据修复；
- 数据库、对象存储、消息和读模型之间如何协调。

## 2. 单一权威数据所有者

每份可变权威数据必须有且只有一个拥有者。

拥有者负责：

- 数据语义；
- 写入规则；
- 生命周期；
- 版本和并发；
- 对外命令和查询；
- 产生业务事件；
- 数据迁移和兼容性。

非拥有者只能通过公开 Application API、事件或专用 Read Model 使用数据。

## 3. 初始数据所有权矩阵

| 数据 | 所有上下文 | 其他上下文允许方式 |
|---|---|---|
| Principal、Delegation | Identity and Access | 解析调用上下文、只读引用 |
| OrganizationUnit、Membership | Organization | 只读查询或组织事件 |
| Role、Policy、Permission | Policy | 授权判定接口 |
| Customer | Customer Management | CustomerId 引用、摘要查询 |
| Contract、ContractVersion | Contract Management | ContractId 引用、事件、公开查询 |
| Project、Milestone | Project Management | ProjectId 引用、公开查询 |
| ApprovalInstance、Decision | Approval Management | 发起审批、接收决定事件 |
| PaymentPlan、PaymentRecord | Finance Operations | 公开命令和财务事件 |
| Document、DocumentVersion | Document Management | DocumentId/Version 引用 |
| OCR、Extraction、Suggestion | Document Intelligence | 查询候选结果、请求应用 |
| Job、JobStep、Attempt | Durable Task Execution | JobId 引用、状态查询、执行事件 |
| AuditEvent | Audit | 只读审计查询 |
| NotificationIntent、Delivery | Notification | 创建通知意图、查询结果 |

## 4. 数据分类

### 4.1 权威业务状态

决定当前业务事实，例如合同状态、审批决定、文档版本。必须由所属上下文以业务规则修改。

### 4.2 执行状态

描述任务、投递、重试和运行过程。它不自动成为业务事实。

### 4.3 不可变历史

包括版本、审计、领域事件和处理产物。原则上追加而非覆盖。

### 4.4 派生读模型

为查询、报表或搜索构建，可重建，不应作为正式写入来源。

### 4.5 外部镜像数据

来自遗留或第三方系统，必须标记来源、同步时间和冲突策略，不能伪装成本系统直接拥有的数据。

## 5. 事务边界

### 5.1 聚合内

聚合是强一致边界。一个事务内维护其不变量和版本。

### 5.2 同一上下文内

应用用例可以在一个本地事务中协调多个 Repository，但应避免无限扩大事务范围。

### 5.3 跨上下文

默认不使用跨上下文共享写事务。采用：

```text
本地事务
→ Outbox/领域事件
→ 消费方幂等处理
→ Process Manager 跟踪
→ 必要时补偿
```

### 5.4 例外

若初期模块化单体中两个上下文临时共享一个数据库实例，也不得因此默认共享业务事务。任何例外必须记录原因、退出条件和迁移计划。

## 6. 一致性等级

### 强一致

适用于：

- 单一聚合状态转换；
- 版本冲突检查；
- 正式业务写入与其 Outbox；
- 权限确认和高风险写入；
- 唯一业务编号分配。

### 读己之写一致

用户完成写入后，应在拥有上下文的查询中立即看到结果。

### 最终一致

适用于：

- 跨上下文通知；
- 搜索索引；
- 报表和分析；
- 外部系统同步；
- 审批决定驱动业务对象后续转换；
- AI 候选结果生成。

### 有界陈旧

允许读模型在明确时限内滞后，必须记录目标窗口和失败告警。

## 7. 乐观并发控制

重要业务聚合必须包含版本号。

写入命令携带期望版本：

```text
expected_version
```

版本不匹配返回稳定冲突错误，不自动覆盖。

适用对象包括：

- Contract；
- ApprovalInstance；
- DocumentVersion 元数据；
- Customer；
- Project；
- FillSuggestion 应用目标。

## 8. 幂等

### 8.1 API 命令

所有可能重试的正式写命令支持稳定 `Idempotency-Key` 或等价业务键。

服务端保存：

- 调用主体；
- 租户；
- 用例名称；
- 幂等键；
- 请求摘要；
- 结果引用；
- 状态和过期策略。

相同键但不同请求摘要必须拒绝。

### 8.2 消息消费

消费者按至少一次交付设计。幂等依据可以是：

- event_id；
- 业务操作唯一键；
- 聚合版本；
- provider_request_id。

### 8.3 外部调用

对支持幂等键的供应商传递稳定键；状态不明确时优先查询结果，不盲目重复产生副作用。

## 9. Outbox 与 Inbox

### Outbox

业务状态和待发布事件在同一数据库事务中提交。

Outbox 实现必须支持：

- 唯一事件 ID；
- claim/lease；
- 尝试次数；
- available_at；
- 最后错误；
- published_at；
- 多 Worker 安全。

### Inbox/消费登记

关键消费者需要记录已处理事件或业务幂等结果，防止重复副作用。

Outbox 和 Inbox 是可靠性适配实现，不进入业务统一语言。

### PLAN-0004 Revision 1 execution boundary

Document Intelligence owns `ProcessingJob`, processing steps, AI tasks,
extraction candidates, and reviews. Durable Task Execution supplies the lease,
fence, retry, cancellation, and recovery mechanics without becoming a second
business-state owner. Worker transitions that touch these records, Audit, or
Outbox use one adapter-owned local transaction. PostgreSQL is the multi-worker
authority; SQLite uses `BEGIN IMMEDIATE` and remains single-process/inline-AI
only. Text artifacts remain in Object Storage and are referenced by bounded,
tenant-scoped metadata; raw text and internal keys do not cross the public
contract boundary.

## 10. Process Manager

跨上下文长期流程由 Process Manager 或 Saga 协调。

它负责：

- 记录流程实例；
- 关联参与上下文和业务 ID；
- 接收事件；
- 发出下一步命令；
- 处理超时；
- 触发补偿；
- 提供业务流程可见性。

它不负责重写参与上下文的内部业务规则。

## 11. 补偿语义

补偿不是技术回滚，而是新的业务动作。

每个跨上下文流程必须定义：

- 哪些步骤可补偿；
- 补偿的业务含义；
- 哪些结果不可逆；
- 人工介入点；
- 超时和终止状态；
- 审计要求。

## 12. 数据库与对象存储协调

文件二进制和大型产物存放于 Artifact Store；数据库保存业务元数据、引用、checksum、大小、版本和状态。

推荐流程：

```text
写入临时对象
→ 校验 checksum/大小
→ 本地事务登记元数据和业务状态
→ 标记对象已提交
→ 异步清理超时临时对象
```

对象状态至少区分：

- Temporary；
- Committed；
- PendingDelete；
- Deleted；
- Orphaned；
- Missing。

## 13. 一致性扫描

必须支持定期检查：

- 数据库有引用但对象缺失；
- 对象存在但数据库无引用；
- 临时对象超过 TTL；
- PendingDelete 长期未完成；
- 派生读模型落后；
- Outbox 长期未发布；
- 任务 lease 长期异常；
- 外部同步游标停滞。

扫描结果必须可观测、可审计、可重试，并区分自动修复和人工处理。

## 14. 删除与保留

业务删除必须区分：

- 逻辑停用；
- 软删除；
- 法规或合同保留；
- 匿名化；
- 物理删除；
- 对象版本清理。

任何级联删除不得跨上下文直接执行。由拥有者发布事件，各上下文按自己的保留规则处理。

## 15. Read Model 和查询

复杂跨上下文查询不应通过任意跨表 JOIN 破坏所有权。

可选方式：

- API Composition；
- 专用查询服务；
- 事件驱动投影；
- 搜索索引；
- 数据仓库或报表库。

Read Model 必须标明：

- 来源；
- 更新方式；
- 允许陈旧时间；
- 重建方式；
- 是否可用于正式写入判断。

## 16. Schema 与迁移

- 每个迁移文件不可修改历史；
- Migration 必须标明所属上下文；
- 先兼容扩展，再迁移数据，再切换读取，最后清理旧结构；
- 破坏性变更必须有兼容窗口和回滚策略；
- 事件 Schema 与数据库 Schema 独立演进；
- 不对外暴露内部表结构作为长期协议。

## 17. 缓存

缓存不是权威数据源。

必须定义：

- key 所属上下文；
- 失效策略；
- 最大陈旧时间；
- 缓存不可用时行为；
- 是否允许负缓存；
- 租户隔离。

禁止使用缓存绕过权限和版本校验。

## 18. 数据安全

- 所有数据带明确租户边界或全局分类；
- Repository 默认按 tenant 约束；
- 敏感字段按分类执行加密、脱敏和访问审计；
- 日志和事件不得泄漏不必要的完整业务数据；
- 测试和预生产数据必须脱敏。

## 19. 验收清单

- [ ] 每个可变业务数据有唯一拥有者；
- [ ] 跨上下文没有直接写表；
- [ ] 强一致和最终一致场景已分类；
- [ ] 正式写入具备版本和幂等策略；
- [ ] 消费者可承受重复消息；
- [ ] 跨上下文流程具有超时和补偿；
- [ ] 文件与数据库有一致性扫描；
- [ ] Read Model 可重建且不作为权威写入源；
- [ ] 迁移和删除策略明确；
- [ ] 租户和敏感数据边界通过测试。
