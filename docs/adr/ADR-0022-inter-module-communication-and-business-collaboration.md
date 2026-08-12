# ADR-0022：Inter-Module Communication and Business Collaboration

> 状态：Proposed
> 日期：2026-08-12
> 决策范围：Business Module 间的查询、命令、事件、引用、快照、投影、Saga 与扩展协作
> 前置：ADR-0003、ADR-0008、ADR-0017、ADR-0018、ADR-0019、ADR-0020

## 1. 背景

ADR-0020 已建立 Platform Core、Business Module 和 Semantic Contract 的边界，但尚未把 Module A 与 Module B 的协作方式收敛为完整、可测试的标准。如果缺少统一边界，模块化单体很容易退化为共享数据库、互相调用 Repository 或用一个万能事件/JSON 层隐藏所有权。

本 ADR 与 [`INTER_MODULE_COMMUNICATION_STANDARD.md`](../standards/INTER_MODULE_COMMUNICATION_STANDARD.md) 配套，补足六类跨模块机制和唯一的 Process Manager/Saga 模型。它不授权新的 runtime、数据库迁移或具体业务实现。

## 2. 提案决策

### 2.1 允许的六类公开协作

1. **Synchronous Application Query**：调用 Owner 的版本化只读 Application/Public Contract；必须有 timeout、tenant、authorization、version、分页、新鲜度与稳定错误映射。
2. **Synchronous Command**：表达对 Owner 的业务意图；事务、授权、幂等和版本由 Owner 拥有；不允许跨上下文共享事务、nested transaction 或 2PC。
3. **Integration Event**：Owner Domain Event 经本地事务 Outbox 映射为版本化事实；发布者不感知消费者；消费者按至少一次、重复、乱序、重试和演进设计。
4. **ResourceRef**：用稳定的 `module/resource-kind/resource-id` 引用替代 private FK；引用不授予权限、不保证存在，使用时重新校验租户、授权、生命周期和版本。
5. **Reference + Snapshot**：对必须解释的历史事实保存 ResourceRef 与不可变、带来源版本的业务快照；快照不代替当前 Owner 状态，也不复制完整可变领域对象。
6. **Published Read Projection**：为列表、批量查询、报表、Dashboard 和分析提供明确 owner、版本、新鲜度、权限、重建方式的非权威读模型；禁止跨模块 private SQL JOIN。

### 2.2 Process Manager / Saga 是唯一跨模块业务过程模型

需要等待事件、人工审批、超时、重试、补偿或多个 Owner 因果协作时，由明确的 Process Manager/Saga 拥有业务过程状态。它只保存协调所需的引用、步骤、等待条件、期限、补偿和人工状态，不复制参与模块的完整领域状态。

Durable Task Execution 继续只拥有 Job/Step/Attempt/Lease/Fence/Retry/Cancel/Recovery 等技术执行状态。两者可由同一 worker 驱动，但不允许建设第二套 Durable Task/Workflow Runtime。补偿是新的 Owner Command，不是数据库回滚；人工审批进入 Owner Application Use Case；消息只负责唤醒，不是权威状态。

### 2.3 跨模块失败默认 fail closed

Query 超时、Command 授权失败、未知 event version、失效 ResourceRef、projection lag、consumer duplicate/乱序和 stale worker 不得被当成成功。系统必须选择显式失败、等待、有限降级读模型或人工介入，并留下可审计的 correlation/causation 证据。

## 3. 明确禁止

- Module A → Module B private Repository/domain/table/SQL/schema/connection pool；
- 业务模块之间共享数据库事务、跨上下文 nested transaction 或 2PC；
- 通过裸表名、private FK 或 storage key 建立跨模块契约；
- 用 Integration Event payload 复制另一个模块的可变完整领域对象；
- 用 Projection、Semantic Contract、Metadata 或 Agent Artifact 作为正式业务写入权威；
- 用通用 Durable Task 运行时承载第二套 Saga 语义；
- 用发布者感知 subscriber 的“事件回调”伪装成 Integration Event；
- 用模块卸载自动 purge 另一个模块的业务数据。

## 4. 事务与一致性模型

单一聚合/上下文内保持本地强一致；跨上下文默认采用：

```text
Owner local transaction
  → Outbox / Integration Event
  → idempotent consumer or Saga
  → next Owner Command
  → explicit compensation or manual intervention
```

Owner 的本地事务同时提交正式状态、Audit 和 Outbox。消费者的 Inbox/幂等登记与自己的业务变更使用自己的本地事务。跨模块“整体完成”是 Process Manager 的业务状态，不由单个 Job Completed 推导。

## 5. 兼容性与安全

公开 Query/Command/Event/Projection/Extension Point 都必须有稳定 ID、schema version、SemVer 兼容窗口、分类和 owner。身份、租户、授权、delegation、correlation 与 trace 从入口传播，并由每个 Owner 重新鉴权。分类跨边界只能保持或收紧，公开协议不得含 raw content、secret、storage key、数据库 URL 或 provider DTO。

## 6. 质量属性影响

| 属性 | 决策影响 | 可测量证据 |
|---|---|---|
| 性能/容量 | 同步调用有 deadline/最大结果；批量使用 projection；事件消费有界并发 | query timeout rate、p95/p99、projection lag、consumer backlog |
| 可用性/隔离 | Owner 故障不扩大为共享事务故障；Saga 可等待/恢复 | dependency failure、recovery scan、blocked process evidence |
| 幂等/恢复 | Outbox/Inbox、版本、fence、重试和补偿显式化 | duplicate/replay/ordering/crash tests |
| 安全/多租户 | 每个边界带 tenant/principal/classification；无私有存储访问 | cross-tenant/unauthorized/private-ref rejection tests |
| 可维护性 | 协作依赖稳定公开契约而非内部实现 | architecture scans、contract compatibility tests |
| 可观测性 | correlation/causation、版本、延迟、拒绝、补偿可追踪 | audit/trace/metrics evidence |
| API/Event 兼容 | Schema version、SemVer、unknown optional field 与 breaking change 策略 | schema/replay/evolution tests |

## 7. 取舍与未决审阅点

- 本提案接受最终一致性和更多补偿建模成本，换取明确所有权和故障隔离。
- ResourceRef 的具体公开序列化格式、Projection Registry 持久化和 Saga storage adapter 留待后续实现计划；本 ADR 不预先选择数据库/消息产品。
- 是否允许同一模块化单体内的同步 in-process adapter，需在实现计划中证明其仍只依赖 Published Port，且可替换为 remote adapter。
- 本 ADR 仍为 Proposed；在接受前不得实现跨模块 runtime。

## 8. 验收门禁

接受本 ADR 前必须形成 synthetic `module-a/module-b/module-extension` fixture 设计，并证明：发布 query/command/event/ref/projection/extension point 的合法路径通过，private repository/schema/FK/SQL 路径失败，live dependency 的 module removal 为 Blocked，Saga 与 Durable Task 状态分离，注册顺序 deterministic，Platform Core 不含 fixture-business knowledge。

## 9. 关联文档

- [`ADR-0020`](ADR-0020-business-module-isolation-and-semantic-contract.md)
- [`BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md`](../architecture/BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE.md)
- [`INTER_MODULE_COMMUNICATION_STANDARD.md`](../standards/INTER_MODULE_COMMUNICATION_STANDARD.md)
- [`WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`](../architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md)
- [`DATA_OWNERSHIP_AND_CONSISTENCY.md`](../architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md)
