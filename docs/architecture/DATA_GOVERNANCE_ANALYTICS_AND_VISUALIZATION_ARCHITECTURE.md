# 数据治理、分析与可视化架构

> 文档 ID：ARCH-ANALYTICS-001
> 版本：1.0
> 状态：Baseline
> 生效日期：2026-08-06
> 所有者/责任模块：Analytics and Visualization（平台能力，运行时待实施）
> 关联 ADR：ADR-0017；依赖 ADR-0008、ADR-0013、ADR-0014、ADR-0015、ADR-0016

## 1. 目标与非目标

本 Baseline 定义平台原生分析、可视化、报表和受控导出的边界。目标是让 UI、Open API、
Report 和 Agent 共享一套版本化指标语义、权限策略和查询执行层，并让派生结果可重放、
可重建、可观测和可审计。

本 Baseline 不新增业务事实所有者，不重新定义 Runtime Audit、Integrity Finding、
Controlled Repair、Repair Ledger 或 Lease/Fence Recovery，不建设通用工作流设计器、
任意 SQL Agent、数据仓库产品或独立微服务。运行时实现不属于本变更。

## 2. 边界和数据分类

| 数据类别 | 权威来源/所有者 | 分析侧处理 |
|---|---|---|
| 权威事务数据 | 对应业务 Bounded Context | 只读消费或受控 Query Port |
| Domain/Integration Event 与 Outbox | 产生事件的业务上下文 | 事件驱动投影输入，至少一次交付 |
| `AuditEvent` | Audit Context，遵循 ADR-0013 | 受控审计分析输入，不改变审计语义 |
| Finding、Repair、Ledger | Runtime Governance，遵循 ADR-0014～0016 | 只读运营/合规视图 |
| 分析投影、指标版本、Dataset、Dashboard/Report 定义 | Analytics | 可重建派生数据和定义，不是业务事实 |
| Technical Logs、Metrics、Traces | Observability | 诊断和 SLO，不冒充业务审计 |

正式业务状态、AuditEvent 和相关 Outbox 必须由数据所有者在同一本地事务中写入。AuditEvent
写入失败时业务事务回滚；Outbox 只负责后续发布。审计字段以
`change_summary`、`changed_field_names`、`resource_version`、策略允许时的
`redacted_before_after` 和 `stable_failure_code` 为边界，禁止 Secret、凭证、内部路径、
完整文件和未脱敏个人数据进入审计或分析载荷。

## 3. 分析数据流

```text
权威业务事务
  → 领域事件 / Outbox / 受控 AuditEvent 读取
  → Projection Consumer + Inbox
  → 分析读模型
  → Metric Semantic Layer
  → Analytics Query Service
  → UI / Open API / Report / Agent
```

业务上下文声明可供分析的稳定领域语义、事件 Schema、字段分类和允许陈旧窗口；Analytics
通过版本化事件或 Query Port 消费，不直接写入拥有者私有表。跨上下文查询若不适合事件
投影，使用带租户和权限过滤的只读 API Composition/Query Adapter。

## 4. 投影生命周期和可靠性

每个投影至少记录 `projection_name`、租户、来源、事件 ID、Schema/Projection 版本、
offset、最后应用时间、状态、失败原因、重试次数、血缘和质量结果。消费者按至少一次
交付设计，必须处理：

- 重复：Inbox/event ID 或等价业务键幂等；
- 乱序：按聚合版本或事件序列延迟、拒绝或补偿；
- 缺口：检测 offset/sequence 缺失并告警，不能静默跳过；
- 重放：从指定版本和 offset 重放，保留兼容映射；
- 重建：删除并从权威数据/事件重建，不依赖旧投影作为事实；
- 失败：隔离坏消息、记录稳定错误码、支持重试和人工处置。

投影更新与 Inbox 登记在同一本地事务中提交。投影延迟、吞吐、失败率、缺口数量、重放
进度、重建耗时和恢复时间必须有指标、告警和租户维度；消息只负责唤醒，offset 和状态
是持久化权威。

## 5. 指标语义层

指标定义必须版本化并声明：

- **Metric**：对外可见的业务指标及口径；
- **Measure**：可聚合的数值、聚合函数和空值规则；
- **Dimension**：允许切片的业务维度、分类和权限；
- **Time Dimension**：业务发生时间、记录时间、时区和粒度；
- **Dataset**：允许的来源字段、连接关系、租户范围和数据分类；
- **Metric Version**：公式、依赖字段、兼容窗口和生效时间；
- **Filter Policy**：主体、租户、行列级过滤、脱敏和导出规则；
- **Lineage**：来源上下文、事件/查询版本、投影和变换链。

业务模块必须为每个公开指标声明语义、所有者、版本、时间基准、延迟目标、权限分类、
异常/缺失处理和回滚/废弃策略。查询只能引用已发布的 Metric Version，不能由 UI、报表
模板或 Agent 临时重写公式。

## 6. 查询安全和执行预算

Analytics Query Service 在执行前建立最终用户身份、租户和授权上下文，并强制应用 Dataset
的行列级策略和脱敏规则。每次查询至少具有：

- 查询模板/指标版本、主体、租户、相关资源和 correlation ID；
- 超时、并发槽位、扫描量、结果行数、返回字节和导出时长预算；
- 取消、分页、采样或降级策略，以及超预算时的稳定错误；
- 查询审计事件和仅含摘要的技术日志。

禁止把 SQL、表名、Schema、数据库 URL、凭证或内部对象路径暴露给公开 DTO、Agent 或日志。
导出必须再次执行权限、脱敏、行数/字节上限和必要的 Prepare → Preview → Confirm；不能
以“报表”绕过租户、保留和审计规则。

## 7. Dashboard、Report 和导出

Dashboard 是声明式定义：布局、组件、Metric Version、Dimension、Filter Policy、刷新
窗口和数据新鲜度提示。Report 在固定模板和版本化查询上生成，记录来源、口径、生成主体、
时间窗口、投影版本、checksum 和产物状态；产物可重建，二进制存储遵循对象存储元数据和
租户隔离规则。定义与执行元数据可写入 Analytics，但不写回业务事实。

UI、Open API 和 Report 不得绕过 Query Service。下钻只可进入有权限的更细粒度 Dataset
或业务拥有者查询，不得退化为任意跨表 JOIN。

## 8. Agent 分析边界

Agent 只可调用白名单、版本化的分析工具，例如“按已发布指标查询”“读取 Dashboard
快照”“请求受限下钻”“准备脱敏报表”。工具返回 Read DTO、口径版本、时间范围、数据
新鲜度和截断状态，不返回 SQL 或 Schema。Agent 不能定义正式指标、改变 Filter Policy、
绕过授权、直接导出未脱敏数据或执行数据库命令。任何写操作仍回到业务 Application
Service；Analytics 不成为 Agent 的写入口。

## 9. PostgreSQL 起步与 OLAP 演进条件

初期使用 PostgreSQL 专用投影表、物化视图和指标快照，沿用 ADR-0008 的 Query Object、
Read DTO、键集分页和多数据库适配边界；SQLite 仅用于本地单进程测试，不能作为生产分析
并发或容量证据。

评估 `analytics-worker` 或 ClickHouse/其他 OLAP 前，必须提供客观证据：在目标租户规模、
查询并发、扫描量、P95/P99 延迟、刷新延迟、保留窗口、重建时间、故障恢复和成本预算下，
PostgreSQL 已无法满足已接受的质量属性。演进必须保留事件/投影重建路径、双读/回填窗口、
权限一致性、迁移验证和可回滚开关；独立存储仍不拥有业务事实。

## 10. 质量属性与验收条件

后续实施计划必须把以下场景写成可测量门禁：

| 场景 | 最低证据 |
|---|---|
| 新鲜度 | 每个 Dataset 有 P95/P99 投影延迟和超窗告警 |
| 查询性能 | 按租户规模记录 P95/P99、超时率、扫描量和并发上限 |
| 恢复 | 重复/乱序/缺口、消费者重启、重放和全量重建测试 |
| 一致性 | 权威样本与投影对账、质量规则、血缘和版本可追踪 |
| 安全 | 租户、行列策略、脱敏、导出限制、Agent 禁止能力契约测试 |
| 可用性 | Query Service 依赖失败时的隔离、降级和稳定错误 |
| 可维护性 | Metric Version 兼容、废弃、回滚和产物重建演练 |

## 11. 迁移、回滚和运维

新增投影遵循兼容事件/表扩展、回填或重放、质量对账、灰度读、切换和旧投影清理顺序；
不得修改历史事件或业务表来迎合报表。指标口径变更以新版本并存，明确生效时间和回滚
版本。投影损坏时停用读、保留权威数据、从 checkpoint 或事件重建，再通过质量门禁恢复。
OLAP 迁移必须具备双写/回填期间的成本和权限审计，以及可回退到 PostgreSQL 的开关。

## 12. 实施门禁

任何后续实现必须引用本 Baseline、ADR-0017、ADR-0008、数据所有权、持久化查询、安全、
可观测性和质量属性文档，并通过 Domain/Application + Fake Port、投影/查询契约、真实
PostgreSQL、故障恢复、架构 Fitness Function、Secret/许可证扫描和文档链接检查。本 PR
只建立文档，不启动 PLAN-0006，不新增运行时代码、迁移、API、Worker、依赖或 ClickHouse。

## 13. Business Module Semantic Contract

业务模块是语义含义和正式业务事实的来源；Analytics 负责注册、校验、编译、授权、查询
计划、预算和可重建投影。模块通过 `BusinessModuleManifest` 声明 module/version、拥有的
Bounded Context、平台能力、公开契约、资源类型、数据分类、迁移命名空间和
`SemanticContribution`，而不是把数据库表或 SQL 暴露给 Agent。

Semantic Contract 继续使用本 Baseline 的 Dataset、Metric、Measure、Dimension、Time
Dimension、Metric Version、Filter Policy 和 Lineage。编译器把语义 ID 归一化为
`<module-id>.<semantic-id>`，解析公开语义引用，拒绝重复/冲突/版本不兼容/未知端点/循环
依赖/跨模块 private 引用，并生成可重建的 canonical manifest + digest。它不实现
Text-to-SQL、WrenAI runtime、任意 SQL、Schema browsing、Database Credentials 或
ClickHouse/通用 OLAP。

未来 Agent 链路仍为：

```text
User → Agent → Typed Semantic Query Request → Analytics Query Service
→ Semantic Resolver/Policy → Query Plan → Controlled Projection Execution
```

返回值只含 Read DTO、指标口径版本、时间范围、新鲜度和截断状态；模块之间通过公开
Application API、事件、ResourceRef、Public Projection 或 Reference + Snapshot 协作。
