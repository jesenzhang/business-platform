# PLAN-0014 Wave 4 — Notification reference conformance review

> Wave: PLAN-0014 Wave 4 · Review date: 2026-09-28 (actual date)
> Branch `codex/plan-0014-wave4-reference-conformance` · Base `a4c2f74b…` · Code commit `6fb9487d…`
> Maturity claimed: **未实现 (not implemented)**. No maturity is claimed for a capability that does not exist.
> Verdict left to the independent reviewer. Baseline / fixed references: `2026-09-28-wave4-baseline.md` §2.3, §3.

## 1. Scope

Review question: what does the Notification area actually provide, is the
documentation honest about it, and what is the minimum honest contract needed so
that no caller can mistake "submit succeeded" for "delivered"?

The Wave 4 brief forbids building a notification platform for reference
alignment. This review therefore measures, registers and stops; the only code
change is the crate's own documentation, which asserted capabilities that do not
exist.

## 3. Current implementation facts

`crates/notification/src/lib.rs` was, in full, a 7-line module doc that claimed:
「负责站内信、邮件、短信、Webhook 等多渠道通知的发送与管理」plus
「本模块遵循 DDD 分层：domain / application / infrastructure / api」 and
`// TODO: 阶段二实现`.

Measured against the tree at the base commit:

* no `NotificationIntent`, no recipient resolution, no preference or opt-out
  model, no template or template version, no idempotency key, no retry policy,
  no failure classification, no delivery-status model;
* no provider adapter and no provider dependency — `crates/notification/Cargo.toml`
  lists `shared-kernel`, `serde`, `chrono`, `uuid`, `thiserror`, none of which is
  referenced because the crate contains no items;
* no migrations, no tests, and **no dependent**: no crate and no composition root
  in `apps/` takes a dependency on `notification`;
* therefore none of the five delivery states exists, and nothing can be claimed
  about 请求已接受 / 进入队列 / provider 已接受 / 确认送达 / 失败.

The delivery substrate that a notification feature would sit on also exists only
partly: the outbox table is written inside document and processing transactions,
and the `messaging` crate implements claim/publish and inbox idempotency with
real PostgreSQL lanes — but **no production binary constructs a relay/publisher**.
`grep outbox apps/**/*.rs` matches only test lanes and cleanup statements, and
`apps/business-api/src/config.rs:529` asserts the API needs no messaging
configuration at all. `config/production.toml:26-27` declares
`[messaging] enabled = true`, and that key has no reader.

Documentation change made in this wave (the only one): the crate doc now states
the placeholder status, enumerates what is missing, forbids treating a submit as
a delivery, points at gaps `N-G01`…`N-G05`, and — per AGENTS §4 — associates the
remaining TODO with an explicit plan (PLAN-0014 Wave 4) instead of a bare
「阶段二实现」. Diff blob `34f1921a9d04bad9804dfa6d4b2c63165204d5e0f33f6d4aca3dd065c161d353`.

## 4. Capability matrix

| Capability | 核心声明? | 适配器实现? | 真实环境验证? | Unsupported behaviour |
|---|---|---|---|---|
| NotificationIntent vs business fact | no | no | no | n/a — nothing exists to call |
| tenant-scoped recipient resolution | no | no | no | n/a |
| preference / opt-out | no | no | no | n/a |
| template + version | no | no | no | n/a |
| idempotency / retry / failure classification | no | no | no | n/a |
| delivery status as a source of truth | no | no | no | n/a |
| provider adapter (e-mail / SMS / webhook / in-app) | no | no | no | n/a |
| outbox → broker relay (substrate) | yes (`messaging` ports) | adapters exist, **no caller in any binary** | PG lanes exist for the crate itself, not for a notification flow | an event stays in `outbox_events` forever |

## 5. Findings

### C0 BLOCKER
None: an inert crate cannot mislead a caller at runtime. The misleading artefact
was the documentation, corrected in this wave.

### C1 R2 REQUIRED

**N-G01 · C1 · 文档与实现不一致（本波已修复文档面）**
* 代码位置：`crates/notification/src/lib.rs`（base 版本 7 行）
* 违反的契约：AGENTS §1「当前实现与权威文档冲突时，不应把现状自动解释为正确设计」；§4「新增 TODO 必须关联明确计划」
* 复现方式：读文件即得；`cargo tree -i notification` 显示零依赖方
* 影响：任何按文档行事的人会认为多渠道通知已可用并直接接入
* 修复范围：仅文档（已在代码提交中）；能力本身不实现
* 测试：不适用（无行为）
* 证据：本文件 §3；blob `34f1921a…`
* 剩余限制：若未来实现，必须同时删除该占位说明并补 ADR/计划

**N-G05 · C1**：出站底座未接线，任何“已通知”说法都不成立。
位置：`config/production.toml:26-27`（`[messaging] enabled = true` 无读取方）、
`messaging` crate（claim/publish 已实现但无组合根使用）、
`grep outbox apps/**/*.rs`（仅测试与清理）。
违反：AGENTS §7（跨上下文默认使用领域/集成事件 + 幂等 + 补偿）、§9（事件必须被消费并处理重复/乱序）。
复现方式：写入一条 outbox 事件（任一 document 创建用例即产生），观察其永不被中继、无 `published_at` 变化（SQLite 侧甚至没有中继列，见 D-W4-10）。
影响：Wave 3 已验证的 relay 逻辑在生产装配中不可达；通知、外部集成、跨上下文投影都建立在一条不存在的投递路径上。
修复范围：属部署/接线决策（哪个进程承载 relay、投递保证、DLQ、退避），需要设计提案 + 部署文档；本波不引入新部署单元。
测试：接线后的端到端（写事件 → relay 发布 → 消费幂等）。
证据：上列 grep/config 位置；Wave 3 `messaging` PG lanes 证明适配器本身可用。
剩余限制：与 ADR-0028（事件信封）未接受直接相关，属 R2 阻塞项。

### C2 / C3（登记为设计前置，不实现）

* **N-G02**：收件人解析必须租户作用域，且默认不得跨租户解析个人联系方式。
* **N-G03**：模板必须有版本，历史通知必须能说明当时使用的是哪个版本。
* **N-G04**：状态模型必须区分 请求已接受 / 进入队列 / provider 已接受 / 确认送达 /
  失败，且每态必须注明真值来源（本仓事实 vs provider 回执 vs 用户可见性回执）。
  参考方向：Documenso 将“业务记录状态”与“外部动作状态”分表建模
  （`packages/prisma/schema.prisma` @ `389390c8…`）。
* **C3**：不引入通知平台、不做多租户模板工作台、不做渠道编排——本轮明确 REJECT。

### REJECTED reference patterns

* “provider 返回 2xx 即视为送达”：provider 接受只到 *provider 已接受*，确认送达需
  provider 回执或用户可见性证据。本仓尚无 provider，故先写下规则，不写实现。
* “把通知内容当业务事实存储”：业务事实由业务上下文拥有，通知只持有意图与状态。
* “用 outbox 表当查询用的通知中心”：Read Model 可重建，不得成为写入权威（AGENTS §7）。

## 6. Test matrix

| # | 测试 | 命令 | 环境 | Verdict |
|---|---|---|---|---|
| N1 | 无（crate 无行为、无测试） | `cargo test -p notification --all-features` | — | 0 tests；`cargo test --workspace --all-features` 759/0/59 中该 crate 贡献 0 |
| N2 | 文档一致性：占位说明与实际内容一致 | 人工读取 + 本评审 §3 | — | pass（本波修正后） |
| N3 | relay 端到端 | — | 需要 relay 接线 | **NOT RUN（能力不存在）** |

## 7. R2 readiness

Notification 不能进入 R2：没有任何能力可实现验收（N-G01…N-G05 中 N-G05 还阻塞
其他跨上下文能力）。诚实的当前状态就是 baseline §2.3 里的「未实现」。
R2 前置条件：接线 relay（N-G05，依赖 ADR-0028）→ 定义意图与状态契约（N-G04）→
再谈渠道。

## 8. Reviewer questions

1. relay 接线应作为 Wave 5 前置，还是并入 ADR-0028 的事件实施波？
2. 是否同意本波「只更正文档、不实现能力」的处理，而不是为参考对齐建平台？
3. 投递状态词汇表（N-G04）应由 Notification 定义，还是由 API/事件标准统一定义？

## 9. Verdict

**Self-verification only: READY FOR INDEPENDENT REVIEW / NOT INTEGRATED.**
Notification maturity: **未实现**；本轮只交付真实性（文档纠正）与可执行的前置条件。

### Blocking actions
N-G05（relay 接线，依赖 ADR-0028）、N-G04（状态契约设计）。

### Deferred actions
N-G02、N-G03、渠道适配、模板工作台。

### Accepted differences from references
* 不采纳 Novu 的 provider/workflow 引擎形态：其参考版本在本环境无法固定
  （`api.github.com/repos/novu/novu` → HTTP 404，`git ls-remote` 0 refs；见 baseline §3），
  因此本波不引用其任何具体行为，也不据此设计契约。
* 参考行为不得覆盖 Accepted ADR 与数据所有权：即便未来 Notification 拥有投递状态，
  业务事实仍由业务上下文拥有，投递状态不得反向改写业务事实。
