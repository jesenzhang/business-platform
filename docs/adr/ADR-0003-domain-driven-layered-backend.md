# ADR-0003：服务端采用领域驱动的分层架构

> 状态：Accepted  
> 日期：2026-07-30  
> 决策范围：整个 Rust 服务端 Workspace

## 背景

平台包含多个企业业务上下文、复杂状态、权限、多租户、长时任务、AI 候选结果和多种入口。若以数据库、HTTP 或基础设施产品组织代码，业务规则将分散在 Handler、Worker、Agent 和适配器中，基础设施替换也会迫使业务重写。

现有总体架构已经提出模块化单体、Domain/Application/Infrastructure/API 等概念，但需要正式确认服务端采用何种领域建模方法，并把业务边界、数据所有权、质量属性、安全、长时任务和架构治理统一到同一决策体系。

## 决策

服务端采用：

```text
战略 DDD
+ 模块化单体优先
+ 领域/应用/适配器分层
+ 依赖倒置与端口适配
+ 数据所有权与显式一致性
+ 质量属性驱动与自动架构门禁
```

具体要求：

1. 战略 DDD 强制采用，业务能力按 Bounded Context、统一语言、数据所有权和公开 Application API 划分；
2. 战术 DDD 按业务复杂度选择，不要求简单 CRUD 机械使用完整模式；
3. Domain 表达业务规则和状态，不依赖交付协议和基础设施；
4. Application 表达用例、事务意图、权限、幂等和跨端口协调；
5. Delivery 负责协议适配，不实现业务规则；
6. Infrastructure 实现核心定义的端口，并依赖核心；
7. Composition Root 负责选择和注入具体实现；
8. 每份可变权威业务数据必须只有一个 Bounded Context 拥有；
9. 单一上下文内优先本地事务，跨上下文使用事件、幂等、Process Manager 和补偿；
10. 通用平台能力与业务上下文分离，业务过程状态和执行机制状态分别归属其拥有者；
11. 具体数据库、消息、存储和供应商选型只能作为适配器、部署和 ADR 决策，不能成为核心业务规则；
12. 跨上下文通过 Application API、领域/集成事件或 Anti-Corruption Layer 协作，不直接写对方私有数据；
13. API、事件、安全、部署、可观测性和遗留迁移属于正式架构资产；
14. 性能、可靠性、安全和恢复目标使用可测量质量属性场景表达；
15. 后续任务必须通过自动化 Fitness Functions 和审查证据证明架构符合性。

## 架构文档集

本决策由以下 Baseline 共同落实：

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
- `docs/architecture/CODE_ARCHITECTURE.md`
- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`
- `docs/standards/RUST_CODING_STANDARD.md`

## 结果

正面影响：

- 业务规则集中且可独立测试；
- Web、Worker、OpenAPI 和 Agent 复用同一用例；
- 数据所有权和跨上下文协作明确；
- 基础设施可替换；
- 模块化单体具备未来拆分条件；
- 长时任务、AI 和 Agent 不会反向主导业务设计；
- 质量属性、安全和运维能力可验收；
- 后续实现拥有持续自动化架构约束。

代价：

- 需要明确上下文、端口、数据所有权和依赖方向；
- 适配器和核心模型之间需要映射；
- 团队必须避免过度使用战术 DDD；
- 跨上下文流程需要显式事件、幂等和补偿；
- 需要维护契约测试、架构门禁和专题文档；
- 重大变化需要 ADR 和迁移计划。

## 被否决方案

### 以数据库表为中心的分层 CRUD

无法稳定承载复杂业务规则，容易使数据库模型成为公开业务模型。

### 所有模块完整套用战术 DDD

会在简单能力中产生不必要的抽象和样板代码。

### 以微服务和基础设施产品作为顶层边界

部署边界不等于业务边界，会把当前技术选择固化到核心模型。

### 只写架构文档、不建立自动门禁

架构会随着实现便利逐步失效，无法持续证明符合性。

## 合规要求

- 所有当前和后续计划读取 `BACKEND_ARCHITECTURE_MANIFEST.md`；
- 计划包含 Bounded Context、数据所有权、质量属性和 Fitness Functions；
- PR 说明架构影响和验证证据；
- 功能测试通过但架构门禁失败时，不得合并；
- 改变本决策核心内容必须使用新的 ADR 替代。
