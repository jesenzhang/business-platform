# 服务端后端架构基线

> 文档 ID：ARCH-BACKEND-001  
> 版本：2.0  
> 状态：Baseline  
> 生效日期：2026-07-30  
> 适用范围：整个 Rust 服务端 Workspace

## 1. 架构结论

服务端后端采用：

```text
战略 DDD
+ 模块化单体优先
+ 领域/应用/适配器分层
+ 依赖倒置与端口适配
+ 数据所有权与显式一致性
+ 质量属性驱动
+ 自动化架构适配门禁
```

完整架构由 [`BACKEND_ARCHITECTURE_MANIFEST.md`](BACKEND_ARCHITECTURE_MANIFEST.md) 登记的文档集共同定义。本文是总体原则，不替代 Bounded Context、数据、安全、长时任务、部署和契约等专题架构。

这不是要求所有代码机械套用完整 DDD 战术模式，而是要求：

1. 业务能力按业务语义划分边界，而不是按数据库表、接口类型或技术组件划分；
2. 核心业务模型独立于交付协议、数据库、消息系统、对象存储和外部供应商；
3. 应用层通过用例组织业务，不让入口层和基础设施层承载业务决策；
4. 基础设施作为可替换适配器依赖核心，而不是核心依赖基础设施；
5. 每份可变权威业务数据只有一个 Bounded Context 拥有；
6. 复杂领域使用 Aggregate、Value Object、Domain Service 和 Domain Event，简单能力保持轻量；
7. 质量属性使用可测量场景表达并进入验收；
8. 架构规则尽可能通过 CI、契约测试和 Fitness Functions 自动证明。

## 2. 为什么采用 DDD

本项目不是单一 CRUD 服务，而是面向企业内部业务平台，包含：

- 用户、组织、客户、合同、项目、审批和财务；
- 文档、解析、字段建议和人工复核；
- 权限、多租户、审计和业务状态机；
- 长时任务、异步处理和跨系统协作；
- Web、开放 API、Worker 和 Agent 等多种入口。

主要复杂度来自业务语义、规则变化、状态转换和跨模块协作，而不是 HTTP 或数据库访问。因此需要以领域模型和业务边界组织系统。

DDD 在本项目中的价值：

- 建立稳定业务边界；
- 统一业务语言；
- 防止数据库模型成为业务模型；
- 防止 Agent、UI、Worker 各自实现业务规则；
- 明确数据所有权和跨上下文协作；
- 使基础设施替换不改变核心业务；
- 为未来按真实边界拆分服务提供基础。

## 3. DDD 的适用程度

### 3.1 战略 DDD：强制采用

所有业务模块必须明确：

- Bounded Context；
- 业务职责和数据所有权；
- 对外公开的 Application API；
- 与其他上下文的关系；
- 统一语言；
- 同步调用和领域/集成事件边界。

领域边界不能由一个数据库表、前端页面、Controller、消息 Topic、基础设施产品或 Agent Skill 直接决定。

### 3.2 战术 DDD：按复杂度采用

以下场景使用丰富领域模型：

- 存在多个业务不变量；
- 有明确生命周期或状态机；
- 多字段必须原子保持一致；
- 操作结果依赖当前业务状态；
- 规则持续演进；
- 需要产生领域事件。

以下场景保持轻量：

- 简单只读查询；
- 无业务不变量的配置数据；
- 字典和参考数据；
- 协议适配；
- 基础设施健康检查。

禁止为了形式完整而为所有表创建空洞的 Aggregate、Repository 和 Domain Service。

## 4. 顶层架构

```text
Delivery / Entry
Web API / OpenAPI / Worker / Scheduler / Agent Adapter
                         │
                         ▼
Application
Use Case / Command / Query / Process Coordination
                         │
                         ▼
Domain
Bounded Context / Aggregate / Entity / Value Object / Policy
                         ▲
                         │ Ports
                         │
Infrastructure Adapters
Persistence / Messaging / Storage / External Providers
                         ▲
                         │
Composition Root
配置、实例化、依赖注入、进程生命周期
```

权威依赖方向：

```text
外层依赖内层
实现依赖抽象
核心不依赖交付和基础设施
```

## 5. 分层职责

### 5.1 Domain

Domain 表达业务事实和规则：

- Aggregate、Entity、Value Object；
- 领域策略和服务；
- 领域状态转换；
- 领域错误和事件；
- 业务不变量。

Domain 不负责 HTTP、数据库、消息发布、文件存储、供应商请求、配置和进程生命周期。

领域模型必须能在无数据库、无网络、无外部服务环境中测试。

### 5.2 Application

Application 表达系统用例：

- 接收业务意图；
- 校验调用上下文；
- 协调领域对象；
- 调用端口；
- 定义事务、版本和幂等边界；
- 协调审计、事件和 Process Manager；
- 返回协议无关结果。

Application 不解析 HTTP Header、编写 SQL、拼接供应商请求、选择基础设施产品或保存全局客户端。

### 5.3 Delivery / Entry

包括 Web API、OpenAPI、Worker 消费入口、Scheduler、Agent Adapter、管理命令和 CLI。

只负责：

```text
协议输入
→ 身份和上下文提取
→ 构造应用命令
→ 调用用例
→ 协议输出
```

不得实现业务状态机、业务权限规则或直接访问数据存储。

### 5.4 Infrastructure

提供持久化、消息、对象与产物存储、外部 AI/OCR、身份供应商、时钟、ID 和遥测适配。

基础设施类型、DTO 和错误不得向上泄漏。核心层只看到稳定的业务或能力接口。

### 5.5 Composition Root

`apps/*` 负责：

- 加载和校验配置；
- 创建具体适配器；
- 将实现注入 Application；
- 启动 API 或 Worker；
- 管理生命周期和优雅关闭；
- 初始化可观测性。

只有组合根可以知道完整技术装配关系。

## 6. 端口所有权

端口由需要能力的内层定义，不由外部产品定义。

核心表达：

- 保存和读取业务聚合；
- 持久化和恢复任务；
- 保存和读取大型产物；
- 调用文档识别能力；
- 发布业务通知；
- 获取时间。

核心不表达：

- 执行某种 SQL；
- 发送某个 Broker Topic；
- 写入某个 Bucket；
- 调用某厂商 Endpoint。

端口名称、输入和输出必须表达业务或通用能力语义。

## 7. 业务域与平台能力

### 核心业务上下文

Contract、Approval、Project、Finance、Document Intelligence 等，承载企业差异化规则。

### 支撑业务上下文

Identity、Organization、Customer、Document Management、Notification、Audit、Policy 等。

### 通用平台能力

Durable Task Execution、Artifact Storage、Messaging、Observability、AI Provider Integration 等。

通用平台能力可以拥有自己的任务、步骤、重试、取消和租约模型，但不得反向侵入具体业务领域。

具体边界和关系以 `BOUNDED_CONTEXT_MAP.md` 为准。

## 8. 数据所有权和一致性

- 每个 Bounded Context 拥有自己的可变权威数据、写入规则和生命周期；
- 其他上下文不得直接更新其私有数据；
- 聚合是核心强一致边界；
- 同一上下文内优先本地事务；
- 跨上下文使用 Application API、领域/集成事件、幂等、Process Manager 和补偿；
- Read Model 可重建，不是正式写入来源；
- Domain 不接触事务对象；
- 基础设施负责事务、锁、并发和可靠发布实现，但不能改变业务语义。

完整规则见 `DATA_OWNERSHIP_AND_CONSISTENCY.md`。

## 9. 工作流和长时任务

必须区分：

- Domain State Machine；
- Process Manager；
- Human Workflow；
- Durable Task Execution。

业务上下文拥有业务过程状态；通用任务能力拥有执行状态。任务成功不自动等于业务完成。

长时任务状态必须持久化，支持检查点、claim/lease、重试、取消、崩溃恢复和多 Worker；具体存储、消息和对象产品属于适配器实现。

完整规则见 `WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`。

## 10. API、事件和集成

跨上下文优先：

1. 对方公开的 Application API；
2. 明确上下文端口；
3. 领域/集成事件；
4. Anti-Corruption Layer。

命令表达意图，事件表达已发生事实。API、事件和回调必须版本化、幂等、兼容和可测试。

禁止跨上下文直接写表、共享可变领域实体、将数据库结构作为公开协议。

完整规则见 `../standards/API_AND_EVENT_CONTRACT_STANDARD.md`。

## 11. 模块化单体与微服务

当前采用模块化单体：

- Bounded Context 首先表现为业务、代码和数据所有权边界；
- crate 不自动等于独立服务；
- 同一部署进程内仍遵守上下文边界；
- 使用消息或 Worker 不自动意味着微服务。

只有独立扩缩容、安全隔离、故障隔离、发布周期、数据所有权或特殊资源形成客观需求后，才通过 ADR 拆分部署单元。

## 12. 基础设施独立性

总体架构规定能力语义和非功能要求，不将具体产品写入核心规则。

具体技术角色记录在：

- Infrastructure Adapter；
- Composition Root；
- 部署配置；
- ADR；
- Runbook；
- 契约测试。

替换基础设施允许修改适配器、配置、部署和运维手册，不应修改业务不变量、业务用例、领域状态和公开业务语义。

## 13. 安全架构

- 默认拒绝和生产 fail-closed；
- 身份、租户和授权在进入 Application 前建立；
- 业务上下文拥有业务授权规则；
- 外部文件、AI、消息和回调均不可信；
- 高风险写入使用 Prepare → Preview → Confirm → Execute；
- Agent 权限不超过原用户；
- Secret 和敏感数据最小暴露；
- 安全控制必须经过自动测试和威胁建模。

完整规则见 `SECURITY_ARCHITECTURE.md`。

## 14. 质量属性

架构以可测量场景驱动：

- 性能和容量；
- 可用性和故障隔离；
- 恢复和 RPO/RTO；
- 安全与租户；
- 可维护性和可替换性；
- 可观测性；
- API/Event 兼容性。

目标和验收见 `QUALITY_ATTRIBUTE_SCENARIOS.md`。

## 15. 部署和可观测性

初始部署单元：

- business-api；
- business-worker；
- ai-worker；
- migration；
- agent-adapter，可选。

部署必须支持环境隔离、健康检查、优雅关闭、水平扩展、资源治理、备份恢复和故障隔离。

所有入口、用例、任务、消息和外部依赖必须通过 request/trace/correlation/job/event 等标识关联；Audit 与普通技术日志分离。

完整规则见：

- `DEPLOYMENT_ARCHITECTURE.md`；
- `OBSERVABILITY_ARCHITECTURE.md`。

## 16. AI 与 Agent 边界

- AI 输出是候选结果，不是业务事实；
- AI Provider 属于外部能力适配；
- 业务上下文负责结果校验、冲突判断和正式应用；
- Agent 是入口适配器，不拥有业务规则；
- Web、Worker、Agent 和其他入口复用同一 Application 用例；
- Agent 和 AI 的替换不得改变业务核心。

## 17. 遗留迁移

采用 Strangler Fig 和 Anti-Corruption Layer 渐进迁移。迁移以业务能力/Bounded Context 垂直切片为单位，任一阶段同一权威数据只有一个写入者。

完整规则见 `LEGACY_MIGRATION_ARCHITECTURE.md`。

## 18. 架构适配门禁

必须持续验证：

- Domain 不依赖交付和基础设施；
- Application 不依赖具体适配器；
- 入口不直接访问数据或承载业务规则；
- 基础设施类型不进入核心接口；
- 跨上下文不直接写对方数据；
- 核心用例可通过 Fake/In-Memory Ports 测试；
- 适配器通过真实依赖契约测试；
- API/Event、安全、任务和迁移兼容性通过测试；
- 文档与代码同步。

具体门禁见 `../standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`。

## 19. 新任务实施准则

实现新能力依次回答：

1. 属于哪个 Bounded Context？
2. 数据所有者是谁？
3. 统一语言和业务不变量是什么？
4. 是业务状态、流程状态还是执行状态？
5. Commands、Queries、API 和 Events 是什么？
6. 需要哪些端口和 ACL？
7. 事务、版本、幂等、重试和补偿是什么？
8. 安全和数据分类是什么？
9. 质量属性和部署影响是什么？
10. 需要哪些 Fitness Functions、文档和 ADR？

## 20. 最终原则

> 业务复杂度由领域模型承载，流程协调由应用用例承载，技术复杂度由适配器承载，运行装配由组合根承载。

> 核心系统描述“系统必须做什么”，基础设施描述“当前如何实现”。

> 后续任务必须证明符合完整架构 Baseline，而不是仅在代码目录上看起来分层。
