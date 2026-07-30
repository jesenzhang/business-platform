# ADR-0003：服务端采用领域驱动的分层架构

> 状态：Accepted  
> 日期：2026-07-30  
> 决策范围：整个 Rust 服务端 Workspace

## 背景

平台包含多个企业业务上下文、复杂状态、权限、多租户、长时任务、AI 候选结果和多种入口。若以数据库、HTTP 或基础设施产品组织代码，业务规则将分散在 Handler、Worker、Agent 和适配器中，基础设施替换也会迫使业务重写。

现有总体架构已经提出模块化单体、Domain/Application/Infrastructure/API 等概念，但没有正式确认服务端采用何种领域建模方法，也没有区分战略 DDD、战术 DDD和端口适配的职责。

## 决策

服务端采用：

```text
战略 DDD
+ 模块化单体优先
+ 领域/应用/适配器分层
+ 依赖倒置与端口适配
```

具体要求：

1. 战略 DDD 强制采用，业务能力按 Bounded Context、统一语言、数据所有权和公开 Application API 划分；
2. 战术 DDD 按业务复杂度选择，不要求简单 CRUD 机械使用完整模式；
3. Domain 表达业务规则和状态，不依赖交付协议和基础设施；
4. Application 表达用例、事务意图、权限、幂等和跨端口协调；
5. Delivery 负责协议适配，不实现业务规则；
6. Infrastructure 实现核心定义的端口，并依赖核心；
7. Composition Root 负责选择和注入具体实现；
8. 通用平台能力与业务上下文分离，业务过程状态和执行机制状态分别归属其拥有者；
9. 具体数据库、消息、存储和供应商选型只能作为适配器、部署和 ADR 决策，不能成为核心业务规则；
10. 跨上下文通过 Application API、领域事件或 Anti-Corruption Layer 协作，不直接写对方私有数据。

## 结果

正面影响：

- 业务规则集中且可独立测试；
- Web、Worker、OpenAPI 和 Agent 复用同一用例；
- 基础设施可替换；
- 模块化单体具备未来拆分条件；
- 长时任务、AI 和 Agent 不会反向主导业务设计。

代价：

- 需要明确上下文、端口和依赖方向；
- 适配器和核心模型之间需要映射；
- 团队必须避免过度使用战术 DDD；
- 需要 CI 架构适配测试持续约束。

## 被否决方案

### 以数据库表为中心的分层 CRUD

无法稳定承载复杂业务规则，容易使数据库模型成为公开业务模型。

### 所有模块完整套用战术 DDD

会在简单能力中产生不必要的抽象和样板代码。

### 以微服务和基础设施产品作为顶层边界

部署边界不等于业务边界，会把当前技术选择固化到核心模型。

## 关联文档

- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/CODE_ARCHITECTURE.md`
- `AGENTS.md`
- `docs/architecture/ARCHITECTURE_STATUS.md`
