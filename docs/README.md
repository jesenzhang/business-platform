# 项目文档中心

本目录是项目文档的统一入口。文档位置、状态和权威关系由 [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) 管理。

## 1. 权威顺序

```text
总体产品与系统架构
→ 服务端架构清单
→ 服务端总体与专题架构
→ 代码架构和标准
→ 当前计划
→ 当前实现
```

发生冲突时，当前代码不能自动覆盖 Baseline；应修正实现或通过 ADR 修改架构。

## 2. 总体与服务端架构

| 类别 | 文档 | 状态 | 作用 |
|---|---|---|---|
| 总体架构 | [`../企业AI业务平台与智能助手总体架构方案_v2.md`](../企业AI业务平台与智能助手总体架构方案_v2.md) | Baseline | 产品、系统主体、Agent 与总体部署边界 |
| 完整服务端架构清单 | [`architecture/BACKEND_ARCHITECTURE_MANIFEST.md`](architecture/BACKEND_ARCHITECTURE_MANIFEST.md) | Baseline | 定义完整架构文档集、权威关系和任务准入 |
| 服务端总体架构 | [`architecture/SERVER_BACKEND_ARCHITECTURE.md`](architecture/SERVER_BACKEND_ARCHITECTURE.md) | Baseline | 战略 DDD、模块化单体、分层和端口适配 |
| Bounded Context Map | [`architecture/BOUNDED_CONTEXT_MAP.md`](architecture/BOUNDED_CONTEXT_MAP.md) | Baseline | 业务能力、上下文职责与协作关系 |
| 数据与一致性 | [`architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`](architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md) | Baseline | 数据所有权、事务、事件、幂等和补偿 |
| 长时任务 | [`architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`](architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md) | Baseline | 业务状态、流程协调和可靠执行边界 |
| 质量属性 | [`architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`](architecture/QUALITY_ATTRIBUTE_SCENARIOS.md) | Baseline | 性能、可用性、安全、恢复和容量验收 |
| 安全架构 | [`architecture/SECURITY_ARCHITECTURE.md`](architecture/SECURITY_ARCHITECTURE.md) | Baseline | 身份、租户、授权、文件、AI 和 Agent 安全 |
| 部署架构 | [`architecture/DEPLOYMENT_ARCHITECTURE.md`](architecture/DEPLOYMENT_ARCHITECTURE.md) | Baseline | 进程、网络、环境、发布和扩缩容 |
| 可观测性 | [`architecture/OBSERVABILITY_ARCHITECTURE.md`](architecture/OBSERVABILITY_ARCHITECTURE.md) | Baseline | 日志、指标、追踪、审计和告警 |
| 遗留迁移 | [`architecture/LEGACY_MIGRATION_ARCHITECTURE.md`](architecture/LEGACY_MIGRATION_ARCHITECTURE.md) | Baseline | 从现有系统渐进迁移与退出策略 |
| 代码架构 | [`architecture/CODE_ARCHITECTURE.md`](architecture/CODE_ARCHITECTURE.md) | Baseline | crate、层次、依赖和运行边界 |
| 架构状态 | [`architecture/ARCHITECTURE_STATUS.md`](architecture/ARCHITECTURE_STATUS.md) | Living | 当前实现符合程度和计划门禁 |

以上文档共同构成完整服务端架构，不能只选择其中一份解释系统设计。

## 3. 标准与基础设施

| 类别 | 文档 | 状态 | 作用 |
|---|---|---|---|
| API 与事件 | [`standards/API_AND_EVENT_CONTRACT_STANDARD.md`](standards/API_AND_EVENT_CONTRACT_STANDARD.md) | Baseline | 命令、查询、事件、版本、幂等和兼容 |
| 架构门禁 | [`standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`](standards/ARCHITECTURE_FITNESS_FUNCTIONS.md) | Baseline | CI 依赖检查、契约测试和发布证据 |
| Rust 编码 | [`standards/RUST_CODING_STANDARD.md`](standards/RUST_CODING_STANDARD.md) | Baseline | Rust 代码、错误、异步、测试和安全规则 |
| 基础设施验证 | [`../企业AI业务平台基础设施开发验证与预生产方案_v1.md`](../企业AI业务平台基础设施开发验证与预生产方案_v1.md) | Baseline | 本地、测试、CI、预生产与恢复 |
| 文档治理 | [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) | Baseline | 文档目录、生命周期、变更和归档 |

## 4. 已接受架构决策

- [`adr/ADR-0003-domain-driven-layered-backend.md`](adr/ADR-0003-domain-driven-layered-backend.md)：服务端采用战略 DDD、模块化单体、领域/应用/适配器分层和依赖倒置。

`ADR-0001` 和 `ADR-0002` 已由正在实施的 PLAN-0001 预留给对象存储 SDK 和可靠 Outbox 决策。

## 5. 文档目录

```text
docs/
├── README.md
├── governance/       治理、流程和文档制度
├── architecture/     业务、应用、数据、技术和部署架构
├── standards/        编码、测试、安全和契约规范
├── adr/              长期架构决策
├── plans/            当前执行计划与归档
├── reviews/          审查和验收记录
├── runbooks/         部署、恢复、值守和故障手册
├── reference/        外部参考与调研结论
└── templates/        标准模板
```

现有两份中文长文档暂时保留在仓库根目录，属于已登记迁移例外。后续新增长文档不得继续放在根目录。

## 6. 文档状态

- `Draft`：讨论中；
- `Proposed`：等待接受；
- `Baseline`：当前权威规则，代码必须遵循；
- `Accepted`：ADR 已接受；
- `Living`：持续反映当前状态，不替代 Baseline；
- `Superseded`：已被替代；
- `Archived`：不再参与当前决策。

## 7. 新任务使用方式

开始任何服务端任务前：

1. 阅读 `BACKEND_ARCHITECTURE_MANIFEST.md`；
2. 确认 Bounded Context 和数据所有者；
3. 明确业务不变量、用例、API/Event 和一致性；
4. 检查安全和质量属性；
5. 在计划中增加架构符合性章节；
6. 确定需要运行的 Fitness Functions；
7. 改变长期边界时新增 ADR；
8. 代码、测试和文档在同一变更中更新。

禁止从数据库表、Handler、消息 Topic 或 SDK 直接推导业务边界。

## 8. 当前实施

- 当前计划：[`plans/current/PLAN-0001-foundation-hardening.md`](plans/current/PLAN-0001-foundation-hardening.md)
- 实时架构状态：[`architecture/ARCHITECTURE_STATUS.md`](architecture/ARCHITECTURE_STATUS.md)
- 初始审查：[`reviews/2026-07-30-initial-implementation-review.md`](reviews/2026-07-30-initial-implementation-review.md)

PLAN-0001 正在实施。其实现分支在合并前必须同步完整服务端架构 Baseline，并提供架构门禁证据。

## 9. 后续仍需落地的运行文档

架构设计已经形成，后续按实施阶段补充具体 Runbook：

- 本地开发和故障处理；
- 预生产部署；
- 正式生产部署；
- 数据库和对象存储恢复；
- 安全事件响应；
- Provider 故障和任务恢复；
- 遗留系统切流与回滚。

Runbook 是架构的操作实现，不得修改 Baseline 语义。

## 10. 模板

- [`templates/ADR_TEMPLATE.md`](templates/ADR_TEMPLATE.md)
