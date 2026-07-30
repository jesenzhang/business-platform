# 架构实施状态

> 文档类型：Living Document  
> 最后更新：2026-07-30  
> 当前阶段：Phase 0 → 基础服务加固  
> 当前计划：`PLAN-0001-foundation-hardening`

## 1. 当前权威结论

- Rust 业务平台是系统主体，Agent 是可选入口；
- 服务端采用战略 DDD 主导的模块化单体；
- 服务端内部采用 Domain、Application、Delivery、Infrastructure 和 Composition Root 分层；
- 核心层只表达业务和通用能力语义；
- 基础设施产品通过适配器、配置和 ADR 接入；
- 战术 DDD 按复杂度采用，不对简单 CRUD 过度建模；
- 长时任务区分业务过程状态与执行机制状态；
- Web、Worker、OpenAPI 和 Agent 复用 Application 用例。

权威文档：

- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/CODE_ARCHITECTURE.md`
- `docs/adr/ADR-0003-domain-driven-layered-backend.md`

## 2. 当前实现状态

当前仓库仍是 Phase 0 工程骨架，已经具备 Workspace 和初始模块，但尚未完全符合目标架构。

已具备：

- `apps/*` 和领域/能力 crate 初始划分；
- Domain/Application/Infrastructure/API 的设计方向；
- 统一配置、错误、对象存储和消息骨架；
- Outbox 概念；
- 总体架构、代码架构和编码规范。

仍需收敛：

- `shared-kernel` 仍存在框架和数据库依赖；
- API 组合状态仍直接公开数据库连接；
- 部分端口、适配器和核心模型尚未物理分离；
- `workflow` 仍未实现通用任务领域和应用能力；
- Worker、Migration 和 Agent Adapter 仍处于骨架阶段；
- 架构依赖规则尚未形成自动化适配测试。

## 3. PLAN-0001 实施约束

`PLAN-0001` 已在实施中，其实现必须遵循以下抽象约束：

1. 业务边界由业务语义和数据所有权定义，不由基础设施产品定义；
2. 核心层只能依赖稳定的业务或能力抽象；
3. 具体技术实现只能位于适配器和组合根；
4. Application 表达事务、幂等、可靠执行和外部能力需求，但不接触实现对象；
5. 通用任务能力可以建模任务、步骤、重试、恢复和取消，但不得包含特定业务规则；
6. 业务上下文拥有业务过程状态，通用任务系统拥有执行机制状态；
7. 任何新增框架或 SDK 依赖必须检查是否向核心层泄漏；
8. 首个垂直切片必须证明核心用例可通过 Fake 端口测试，并通过真实适配器契约测试验证基础设施。

## 4. 架构适配门禁

PLAN-0001 完成审查时至少验证：

- Domain 不依赖交付框架、数据库、消息、存储和供应商 SDK；
- Application 不依赖具体适配器；
- Handler 和 Worker 入口不直接承载业务规则；
- 基础设施错误和 DTO 不向核心泄漏；
- 业务用例能以 Fake/In-Memory 端口运行；
- 适配器有真实依赖契约测试；
- 跨上下文没有直接写入对方私有数据；
- 新增长时任务实现区分业务状态和执行状态。

## 5. 下一次更新条件

出现以下事件时更新本文：

- PLAN-0001 形成 PR；
- DDD 分层在首个垂直切片中完成验证；
- 新增或调整 Bounded Context；
- 引入新的通用平台能力；
- 改变依赖方向；
- 拆分部署单元或微服务；
- 架构适配测试进入 CI；
- PLAN-0001 完成并归档。

## 6. 当前判定

```text
架构基线：已明确
代码骨架：已存在
DDD 战略边界：待逐步细化
分层依赖：部分符合
基础设施隔离：部分符合
长时任务核心：尚未实现
自动化架构门禁：尚未实现
PLAN-0001：实施中
```
