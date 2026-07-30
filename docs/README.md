# 项目文档中心

本目录是项目文档的统一入口。文档位置、状态和权威关系由 [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) 管理。

## 1. 权威文档

| 类别 | 文档 | 状态 | 作用 |
|---|---|---|---|
| 总体架构 | [`../企业AI业务平台与智能助手总体架构方案_v2.md`](../企业AI业务平台与智能助手总体架构方案_v2.md) | Baseline | 产品、系统和部署总体边界 |
| 服务端架构 | [`architecture/SERVER_BACKEND_ARCHITECTURE.md`](architecture/SERVER_BACKEND_ARCHITECTURE.md) | Baseline | 战略 DDD、分层、端口适配和基础设施独立性 |
| 代码架构 | [`architecture/CODE_ARCHITECTURE.md`](architecture/CODE_ARCHITECTURE.md) | Baseline | crate、层次、依赖和运行边界 |
| 基础设施 | [`../企业AI业务平台基础设施开发验证与预生产方案_v1.md`](../企业AI业务平台基础设施开发验证与预生产方案_v1.md) | Baseline | 本地、测试、CI、预生产与恢复 |
| 编码规范 | [`standards/RUST_CODING_STANDARD.md`](standards/RUST_CODING_STANDARD.md) | Baseline | Rust 代码、错误、异步、测试和安全规则 |
| 文档治理 | [`governance/DOCUMENT_MANAGEMENT.md`](governance/DOCUMENT_MANAGEMENT.md) | Baseline | 文档目录、生命周期、变更和归档 |
| 架构状态 | [`architecture/ARCHITECTURE_STATUS.md`](architecture/ARCHITECTURE_STATUS.md) | Living | 当前架构落实程度和实施门禁 |

服务端架构权威关系：

```text
总体架构
→ SERVER_BACKEND_ARCHITECTURE
→ CODE_ARCHITECTURE
→ RUST_CODING_STANDARD
→ 当前实现
```

现有两份中文架构文件暂时保留在仓库根目录，属于已登记的迁移例外。后续内容更新不得再新增根目录长文档。

## 2. 已接受架构决策

- [`adr/ADR-0003-domain-driven-layered-backend.md`](adr/ADR-0003-domain-driven-layered-backend.md)：服务端采用战略 DDD 主导的模块化单体和领域/应用/适配器分层。

`ADR-0001` 和 `ADR-0002` 已由正在实施的 PLAN-0001 预留给对象存储 SDK 和可靠 Outbox 决策，合并时必须补齐登记。

## 3. 文档目录

```text
docs/
├── README.md                 文档入口
├── governance/               治理、流程和文档制度
├── architecture/             系统、代码、数据和集成架构
├── standards/                编码、测试、安全和接口规范
├── adr/                      已接受或提议的架构决策
├── plans/                    当前执行计划与归档规则
├── reviews/                  审查和验收记录
├── runbooks/                 部署、恢复、值守和故障手册
├── reference/                外部参考与调研结论
└── templates/                文档模板
```

## 4. 文档状态

- `Draft`：讨论中，不具有约束力。
- `Proposed`：等待接受，可用于评审。
- `Baseline`：当前权威基线，代码必须遵循。
- `Accepted`：ADR 已接受。
- `Living`：持续反映当前项目状态，不替代 Baseline 和 ADR。
- `Superseded`：已被新文档替代，仅供追溯。
- `Archived`：不再使用，仅保留历史。

## 5. 使用方式

### 设计新功能

1. 确认所属 Bounded Context、统一语言和数据所有权；
2. 阅读总体架构、服务端架构、代码架构和编码规范；
3. 明确 Domain、Application、Delivery、Infrastructure 和 Composition Root 的职责；
4. 若改变长期架构边界，新增 ADR；
5. 若是阶段性实施，写入 `plans/current/`；
6. 实现完成后提交审查记录；
7. 计划完成后移入归档。

### 修改已有文档

- 修改 `Baseline` 文档必须说明影响范围；
- 新文档必须从本入口登记；
- 被替代文档必须标记替代关系，不能静默删除；
- 与代码、配置或部署不一致的文档必须在同一变更中修复；
- `ARCHITECTURE_STATUS.md` 在计划、PR、架构门禁或实现状态变化时更新。

## 6. 当前实施

- 当前计划：[`plans/current/PLAN-0001-foundation-hardening.md`](plans/current/PLAN-0001-foundation-hardening.md)
- 实时架构状态：[`architecture/ARCHITECTURE_STATUS.md`](architecture/ARCHITECTURE_STATUS.md)
- 初始审查：[`reviews/2026-07-30-initial-implementation-review.md`](reviews/2026-07-30-initial-implementation-review.md)

PLAN-0001 正在实施，其代码必须符合 `SERVER_BACKEND_ARCHITECTURE.md` 和 ADR-0003；若实施分支已创建，应在合并前同步这些基线或完成 rebase。

## 7. 模板

- [`templates/ADR_TEMPLATE.md`](templates/ADR_TEMPLATE.md)

## 8. 后续待补充

- Bounded Context Map 和统一语言表
- API 设计规范
- 数据库迁移规范
- 事件与消息 Schema 规范
- 测试策略
- 安全基线
- 预生产和生产 Runbook
