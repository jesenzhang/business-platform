# PLAN-0014: Reference Conformance and R2 Module Readiness

> Status: Active / Review-first  
> Revision: 0  
> Date: 2026-09-23  
> Target close: 2026-10-23  
> Owner: Platform Architecture + Module Owners  
> Planning Base: main at `8365de0211390a6a3367b536c5ceaa5256bbb5cf` (2026-09-23 sync)
> Architecture inputs: SAAS_PLATFORM_ARCHITECTURE, SAAS_MODULE_STANDARD, BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE, ADR-0020/0021/0022/0024/0025  
> Architecture Rebaseline: 2026-09-23 ADR-0025 / SaaS Baseline alignment; execution branches must sync latest main before review  
> Constraint: this plan does not authorize a broad microservice split or replacement of current working modules

## 1. Goal

对已经完成或接近完成的 Platform/Business modules 做系统化 Reference Conformance Review，使用成熟开源实现、规范和真实生产型系统校准：

- Authority 与数据所有权；
- Tenant/Principal/AuthZ；
- Domain/Application contract；
- failure/retry/idempotency；
- security/isolation；
- version/compatibility/migration；
- operations/recovery；
- independent release model；
- contract/integration/E2E tests。

目标不是把 business-platform 改造成任何参考项目的 clone，而是证明当前实现：

1. 没有遗漏成熟系统已经证明必须处理的关键失败模式；
2. 没有把供应商/框架特性误当成平台 Domain；
3. 已完成模块达到可进入 R2 独立发布的质量；
4. 发现的真实缺口有优先级、测试和关闭条件；
5. 后续新模块可以复用同一套对齐与评审方法。

## 2. 与当前产品计划的关系

本计划是横向平台质量计划，与 Contract Business Vertical Slice 并行，但不能替代真实业务验证。

```text
PLAN-0014 Reference Conformance
  ├─ review/reference/test first
  ├─ only bounded C0/C1 repairs
  └─ R2 pilot
             │
             ├──────────────┐
             ▼              ▼
Contract Vertical Slice   future modules
             │
             ▼
PLAN-0006 Workspace/Agent
```

规则：

- Contract Vertical Slice 仍是下一项主要业务代码计划；
- PLAN-0014 的 reference review 可以并行执行；
- 只有 C0/C1 gap 才允许在本计划内直接修复；
- C2/C3 进入后续 backlog，不为“对齐”制造大范围重构；
- 若 reference evidence 要求改变 Accepted ADR/Authority，先停在 review，另立 ADR。

## 3. 对齐方法

每个模块必须执行同一流程。

### Step A — Current implementation reconstruction

从源码、测试、migration、API/Event、runbook 重建当前真实语义，不以架构文档代替代码事实。

至少记录：

- authoritative data；
- public ports/contracts；
- persistence ownership；
- tenant boundary；
- security boundary；
- failure model；
- retry/idempotency；
- version/migration；
- deployment/runtime assumptions；
- current test evidence；
- current release maturity R0-R4。

### Step B — Reference pinning

每个 reference 必须记录：

- repository/product；
- reviewed revision/release；
- review date；
- license；
- docs；
- core source path；
- contract/security/recovery tests；
- relevant benchmark/production evidence；
- important issue/PR where failure mode is material。

不能只读 README，也不能用 moving latest 代替 reviewed revision。

### Step C — Capability matrix

逐项形成：

| Dimension | business-platform | Reference | Gap | Decision |
|---|---|---|---|---|
| Authority | | | | KEEP/ADAPT |
| Domain model | | | | |
| Tenant | | | | |
| AuthZ | | | | |
| API/Events | | | | |
| Failure semantics | | | | |
| Retry/Idempotency | | | | |
| Recovery | | | | |
| Version/Migration | | | | |
| Security | | | | |
| Observability/Audit | | | | |
| Performance | | | | |
| Release model | | | | |

### Step D — Gap classification

统一等级：

- **C0 BLOCKER**：安全/租户越权、数据损坏、authority 混乱、无法恢复、已知高风险失败模式；
- **C1 R2 REQUIRED**：进入独立发布前必须完成；
- **C2 SHOULD**：有明确价值，但不阻止当前 R2；
- **C3 DEFER**：当前没有真实消费者或成本明显高于收益；
- **REJECT**：参考项目做法与本平台 Authority/约束不兼容。

每个 gap 必须写出“为什么”，不能只列功能差异。

### Step E — Bounded repair

只实现 C0/C1。任何 repair 都必须：

- 有 failing test / contract fixture / architecture rule 先证明 gap；
- surgical change；
- 不顺带引入参考项目完整框架；
- 更新 manifest/contract/docs；
- 运行模块 gate + workspace gate。

### Step F — Independent review

实现者之外的 reviewer 对固定 SHA 范围审阅。

Reviewer 必须回答：

1. Authority 是否保持；
2. 是否存在 provider/framework leakage；
3. Tenant/AuthZ 是否 fail closed；
4. reference gap 是否被正确分类；
5. C0/C1 是否全部关闭；
6. tests 是否真的覆盖失败语义；
7. version/migration/release 是否 honest；
8. 是否存在为对齐而产生的 speculative abstraction；
9. R2 admission 是否成立。

Verdict 只有：PASS / PASS WITH C2-C3 / FAIL。

## 4. 优先级与参考项目

### P0-A — Identity + Organization + Policy

**当前基础：PLAN-0013 Integrated。**

Primary references：

- Logto — Organization / Organization RBAC / M2M；
- ZITADEL — B2B organization / OIDC / service identities / audit；
- OpenFGA — ReBAC / resource relationship；
- SpiceDB — Zanzibar / consistency / caveat；
- Cerbos — contextual RBAC/ABAC/PBAC；
- ABP — tenant-aware application permission/module design。

对齐重点：

- PlatformUser vs external subject；
- TenantMembership / OrganizationMembership lifecycle；
- suspended/revoked/expired behavior；
- service account / M2M identity boundary；
- RoleBinding/resource scope；
- group/org inheritance；
- explicit share；
- explain decision；
- revocation consistency/cache；
- cross-tenant fail closed；
- external AuthZ engine port feasibility；
- internal permission vocabulary remains authoritative。

Required tests：

- issuer+subject stable mapping；
- unknown/suspended membership；
- cross-tenant resource probe；
- stale role binding/revocation；
- resource-scope widening attack；
- organization inheritance matrix；
- service identity cannot inherit human privilege accidentally；
- authorization explain is consistent with decision；
- PostgreSQL/SQLite shared behavior suite；
- provider adapter contract fixture if an external engine adapter is introduced。

### P0-B — Business Module Packaging

Primary references：

- ABP；
- Frappe/ERPNext；
- Odoo；
- Twenty。

对齐重点：

- module identity/version；
- dependency/capability declaration；
- enable/disable/uninstall；
- uninstall != purge；
- migration ownership；
- package digest；
- compatibility window；
- extension/contribution；
- tenant applicability；
- dependency cycle/removal；
- upgrade/rollback；
- independent release artifact。

Required tests：

- deterministic compile；
- same input permutation -> same bytes/digest；
- incompatible capability/version fail closed；
- dependency cycle；
- live consumer removal blocked；
- module disable preserves retained data；
- upgrade/downgrade compatibility fixture；
- manifest/contract/release artifact consistency；
- independent package build dry-run。

### P1-A — Durable Task Execution

Primary references：

- Temporal；
- Restate；
- Trigger.dev；
- jarvis-rs Durable Runtime；
- AgentOS durable execution concepts where relevant。

对齐重点：

- accepted != completed；
- Job/Step/Attempt ownership；
- lease/heartbeat/fencing；
- retry classification；
- cancellation；
- checkpoint/resume；
- worker crash/redeploy；
- workflow/business-state vs execution-state；
- side-effect idempotency；
- outcome unknown/reconciliation；
- version change while work is in flight；
- HITL wait/resume；
- multi-worker ownership。

Required tests：

- stale worker completion rejected；
- lease expiry/reclaim；
- retryable vs terminal；
- crash at every commit boundary where practical；
- duplicate dispatch；
- cancel race；
- checkpoint recovery；
- no blind replay after non-idempotent side effect；
- rolling-version compatibility fixture；
- real PostgreSQL multi-worker evidence。

### P1-B — Audit + Messaging

Audit references：

- ZITADEL audit/event history；
- immudb tamper-evident/immutable log concepts；
- Infisical audit/access event design。

Messaging references：

- Debezium Outbox pattern；
- NATS JetStream / Kafka delivery semantics as broker references；
- mature transactional outbox/inbox implementations in reference systems.

对齐重点：

- AuditFact != Telemetry；
- actor/tenant/action/resource/outcome/correlation；
- tamper evidence guarantee is stated honestly；
- retention/export/SIEM；
- duplicate/out-of-order/replay；
- poison message/dead-letter；
- consumer version evolution；
- backpressure；
- outbox finality and publisher recovery。

Required tests：

- audit row committed with authoritative business write where contract requires；
- broken chain detection；
- replay/duplicate inbox convergence；
- out-of-order event behavior；
- publisher crash after claim；
- consumer retry exhaustion；
- schema evolution fixture；
- sensitive-field redaction。

### P1-C — Document + Object Storage

Document references：

- Mayan EDMS；
- Paperless-ngx；
- OpenContracts；
- Documenso。

Storage references：

- AWS S3 behavioral contract；
- RustFS；
- MinIO。

对齐重点：

- Document/File/Revision separation；
- immutable/version binding；
- business resource link；
- ACL inheritance；
- retention/deletion；
- evidence/artifact linkage；
- multipart；
- checksum；
- presigned URL；
- conditional request；
- object versioning；
- provider capability matrix；
- DB/object compensation。

Required tests：

- wrong revision cannot receive evidence/update；
- cross-tenant file access；
- object missing after metadata commit；
- DB rollback after object upload；
- multipart abort/retry；
- checksum mismatch；
- presign scope/expiry；
- provider capability negative tests；
- real S3-compatible contract lane.

### P2-A — Observability

Primary reference：

- OpenTelemetry specification/Collector；
- Prometheus/Grafana only as backends；
- Langfuse only for AI/Agent telemetry.

对齐重点：

- correlation/trace propagation；
- semantic naming；
- bounded label cardinality；
- sampling；
- redaction；
- telemetry vs audit separation；
- exporter failure isolation。

Required tests：

- request -> job -> provider correlation；
- no tenant/user/document ID as unbounded metric label；
- exporter unavailable does not corrupt domain transaction；
- sensitive fields absent；
- trace continuity across async job.

### P2-B — Notification

Primary reference：

- Novu；
- provider-specific email/SMS only as adapters.

对齐重点：

- NotificationIntent；
- recipient resolution；
- channel preference；
- template version；
- retry/idempotency；
- digest/delay；
- delivery status；
- business state independence。

Required tests：

- duplicate intent convergence；
- provider timeout/retry；
- opt-out/preferences；
- tenant template isolation；
- delivery failure does not roll back committed business state。

## 5. Review artifacts

每个 module group 产出一份固定 review：

```text
docs/reviews/
  2026-09-23-plan-0014-wave0-iam-policy-baseline.md
  2026-09-23-saas-align-identity-organization-policy.md
  2026-09-xx-saas-align-module-packaging.md
  2026-10-xx-saas-align-durable-execution.md
  2026-10-xx-saas-align-audit-messaging.md
  2026-10-xx-saas-align-document-storage.md
  2026-10-xx-saas-align-observability-notification.md
```

使用：

`docs/templates/REFERENCE_CONFORMANCE_REVIEW_TEMPLATE.md`

每份 review 必须记录 fixed reference revisions，不允许只引用项目首页。

### Wave 0/1 execution record — 2026-09-23

本轮执行分支为 `feat/PLAN-0014-wave1-iam-policy-reference-conformance`，固定
Base 为 `8365de0211390a6a3367b536c5ceaa5256bbb5cf`；`main` 未修改、未 push、未
启动 Wave 2。已从源码、测试、migration、API/UI 与 architecture check 重建
Identity / Organization / Policy 的真实语义，并固定 Logto、ZITADEL、ABP Framework、
OpenFGA、SpiceDB、Cerbos 的 release/tag、commit、license、source/test/doc 路径。

交付物：

- `docs/reviews/2026-09-23-plan-0014-wave0-iam-policy-baseline.md`；
- `docs/reviews/2026-09-23-saas-align-identity-organization-policy.md`；
- 本计划与 `docs/architecture/ARCHITECTURE_STATUS.md` 的执行状态同步。

当前结论：Identity / Organization / Policy 保持 R1 foundation；本轮未发现 C0，C1
明确为 R2 阻断或待独立发布证据关闭；过期/撤销生命周期、service identity、通用
ReBAC 与独立模块发布材料保留为 C2/C3 或后续 Wave。没有为“参考对齐”引入 Rust、
migration、OpenAPI 或 provider runtime 变更。共享 identity/organization/policy
contract 的 fake/domain 与 SQLite 适配器证据已记录；Business API、PostgreSQL、
workspace 全门禁及独立 reviewer 必须按 formal review 的逐项状态解释，未运行项不得
解释为通过。独立 reviewer 已给出 `PASS WITH C2/C3`（Base
`8365de0211390a6a3367b536c5ceaa5256bbb5cf` → reviewed Candidate
`c7dc3d142a526b47e9ba8e472b82346ffdaed221`）；Wave 1 review exit 满足，Identity /
Organization / Policy 仍保持 `R1 / R2 BLOCKED / NOT CLAIMED`。

本轮收集的当前分支证据：`cargo fmt --all -- --check` PASS；
`scripts/check-architecture.ps1` 的 Architecture Fitness 与 OpenAPI contract PASS；
`iam_adversarial_sqlite` 6/6、`e2e_admin_bootstrap_sqlite` 3/3、`iam_admin_api` 22/22、
`security` 16/16 PASS；identity/organization/policy domain/shared fake tests 及三个
SQLite adapter contract suites PASS。PostgreSQL、workspace-wide check/clippy/test 与
本机 security/license/secret scan 仍分别为 `NOT RUN`（无本机 PostgreSQL；本轮无生产 Rust
变更）。详细限定见 formal review 的 Validation 表。
Reviewer 另将 OrganizationMembership roster removal 与独立 Policy RoleBinding 撤销
未被联动/验证列为 G-09 C2；据此补齐 gap registry 与 scenario 9 的证据边界，没有
改变运行时语义。

### Wave 2 execution record — 2026-09-24

本轮执行分支为 `feat/PLAN-0014-wave2-business-module-packaging-conformance`，固定
Base 为 `5d3d2eb5cfe93aad6f8c5fe4d023d386d0a08b04`（Wave 1 链未合入 main，因此直接从
本地 Wave 1 链顶创建；`git range-diff` 已确认本地链与远端同名链 patch 等价，两条链不得
同时合入；本地 `main` 落后，不作为基线）。`main` 未修改、未 push、未启动 Wave 3、
未宣布任何模块 R2。

固定参考（review date 2026-09-24）：ABP `10.6.1` = `9ca1d3ee62c8c557af34b50fc49c146405d752d4`、
Frappe `v16.35.0` = `012667b9c4e7f66d5e1ff5858d2e922331d4300a`、ERPNext `v16.36.0` =
`b30aa5334bcea94dba74f5b866af13c43a861948`、Odoo 分支 `19.0` tip =
`2e2acfd6d2725d1a4a95b1a605634e1ac34163f`（无版本 tag，如实记录）、Twenty `v2.9.0` =
`b53f1832d80f87be2cfd88990b855c0e5f3cc926`；均按源码/测试取证。（Round 2 校准：
`TwentyCoreModule`/`serverVersionRange`/`ApplicationActivation` 三个标识从未出现在
ADR-0021 原文，其 NOT FOUND 只是对本记录自身参考取证的标识校准；ADR-0021 依赖的
"required server version range" 兼容窗口语义经 v2.9.0 源码证据（`engines.twenty` +
`CANNOT_DOWNGRADE_APPLICATION`）证实仍成立，Accepted ADR 未被推翻、未修改，无需修订
候选。见 review §3/§16。）

交付物：

- canonical module release identity：`module-release.identity.v1`（8 字段）与
  `ModuleReleaseBundle`（module_version → crate_version 审计映射）落为
  `business-module-contracts/src/release.rs` 纯值契约；compiler 新增 canonical
  `contract_digest` seam（公开契约面 SHA-256，与输入顺序及发布元数据无关）；
  `check-architecture.ps1` 增加成员与 seam fitness。复用 `BusinessModuleManifest`
  为唯一声明，未建第二 manifest。
- 新测试：release identity 9 unit + `release_surface_digest.rs` 5 项行为测试；
- `docs/reviews/2026-09-24-saas-align-module-packaging.md`（能力矩阵、决策登记、
  gap registry G-201..G-206、T0–T7 验证矩阵、R2 packaging acceptance A-01..A-12、
  Wave 2 exit 判定）；
- 本计划与 `docs/architecture/ARCHITECTURE_STATUS.md` 的执行状态同步。

当前结论：Wave 2 三个 exit（canonical release identity defined / R2 packaging
acceptance executable / no forced microservice split）判定 MET（Round 1 初判；其中
第一项在 Round 2 独立审阅中因 A-09 高估与摘要漏面被判 NOT MET，修复后以新测试证据
恢复 MET——见下方 Round 2 记录与 review §12/§16）；C0=0；C1×3——G-201
release identity 契约层已关闭（真实模块工件组装留 Wave 5），G-202 独立发布工件与
workspace 外消费者证据缺失（R2 BLOCKER，Wave 5 pilot 关闭）、G-203 迁移命名空间未落
物理隔离（声明字段已有）；C2×3 含继承的 G-09 原样保留。`business-module-contracts` /
`business-application-compiler` 与 Identity / Organization / Policy 一律维持
`R1 / R2 BLOCKED / NOT CLAIMED`。

本轮证据（本地 Windows，`cargo` 链顺序执行）：`cargo fmt --all -- --check`、
`cargo check --workspace --all-targets --all-features`、
`cargo clippy --workspace --all-targets --all-features -- -D warnings`、
`cargo test --workspace --all-features` 全部退出码 0；focused 79 tests（含 14 项新增）
PASS（Round 2 更正：该 "79" 为誊写错误，Round 1 实际为 99 = contracts 22u+22i、
compiler 18+26+5+6；分项以 review §4.2 为准）；`scripts/check-architecture.ps1`
Architecture Fitness 与 OpenAPI contract PASS；`git diff --check` clean。PostgreSQL
迁移 lane、release 工件构建与 workspace 外独立消费者构建为 `NOT RUN`（无本机
PostgreSQL / release tool 未实现 / 属 Wave 5 fixture，见 review §9/§10），不以 focused
或 SQLite 证据替代。独立 reviewer verdict 未返回；该轮状态为
`READY FOR REVIEW / NOT INTEGRATED`。

Wave 2 Round 2 修复（同分支，2026-09-24）：Round 1 候选 `50b6ae9` 的独立审阅发现
(1) `contract_digest` 遗漏已登记为 public target 的公开 agent capability 面（先写
失败测试证实再修复纳入）；(2) A-09 只有 shape 校验与手工同值装配，非一致性验证——
新增 compiler 边界 `verify_release_identity`（对 `compile()` 输出重算 contract digest
与 compiled declaration fingerprint 并比较，核对 module_id/version/manifest schema/
migration namespace，单字段错配各有负向测试）并明确 `validate` 仅字段形状校验；
(3) release 层 `module_version`/bundle `crate_version` 强制严格 SemVer；(4) `package_digest`
语义修正为 compiled declaration fingerprint——声明摘要，不是发布工件字节，工件完整性
digest 继续 Wave 5 NOT RUN；(5) 计数与 ADR-0021 表述校准。focused 现为 113 tests
（Round 1 新增 14 + Round 2 新增 14）。Wave 2 exit 第一项经 Round 2 NOT MET 降级后以
新证据恢复 MET（review §12/§16）。本轮仍零部署单元、零运行时变更、未改 Accepted ADR、
未改 manifest/compile 兼容行为。修复后状态为 `READY FOR INDEPENDENT RE-REVIEW /
NOT INTEGRATED`；PostgreSQL、真实发布工件、workspace 外消费者证据维持 NOT RUN。

Wave 2 closeout 与外部复审（同分支，2026-09-27，docs-only）：实现者之外的外部只读复审
已记录于 review §14——Codex 独立只读复审，Reviewed HEAD
`93b18ab67c0426e6e5efc4f065260d7068995f38`，审阅修复范围
`50b6ae95a3e6f8db8f51586cf1e5bce5d0662f7f` → Reviewed HEAD：代码修复通过；文档兼容性
表述不确——review §5 规则 3 原"contract_digest 不变 ⇒ 兼容面不变"高估摘要语义，已改为
"contract_digest 相同仅表示纳入摘要的公开声明面相同；compatibility、dependencies、
module_version 等仍须通过 compile/dry_plan 的独立兼容性校验，不能据摘要相同推断可互换"。
复审独立执行 focused 113 passed / 12 suites、Architecture Fitness PASS、
`git diff --check` PASS（closeout 在 Reviewed HEAD 复测同值）；全 workspace 四项门禁
与外部参考取证本轮未独立重跑；PostgreSQL / 真实发布工件 / workspace 外消费者维持
NOT RUN。验收分项校准：review §10 十二条 = 10 PASS（A-01..A-09、A-11；A-09 验证范围为
声明一致性，非真实工件/外部消费者）+ 1 PARTIAL（A-10 workspace 内部分）+ 1 NOT RUN
（A-12）；此前"可运行 9 条 PASS"为 A-09 恢复后未跟随更新的誊写计数。Round 1/2 历史与
失败记录未改写。Wave 2 当前状态：`Wave 2 implementation reviewed / documentation
corrected / NOT INTEGRATED`；外部复审不构成 R2 admission PASS，Step F formal verdict
未回填；R1 / R2 BLOCKED / NOT CLAIMED 维持；G-201 真实工件组装残项、G-202、G-203、
G-09 保留。

## 6. 日程

### Wave 0 — Baseline & tooling
**2026-09-23 → 2026-09-24**

- inventory 当前 module/API/test/migration；
- 固定 reference review revision；
- 建立 review template；
- 建立 gap registry；
- 确认 R1 maturity baseline。

Exit：

- module inventory 完整；
- reference revisions pinned；
- review template 可执行；
- 无代码行为变化。

### Wave 1 — IAM / Organization / Policy
**2026-09-24 → 2026-09-29**

- P0-A reference review；
- adversarial test gap；
- C0/C1 bounded repair；
- independent review。

Exit：

- C0=0；
- C1=0 or explicit R2-blocked；
- review verdict available。

### Wave 2 — Business Module Packaging
**2026-09-30 → 2026-10-03**

- P0-B reference review；
- module version/release model gap；
- independent package build/dry-plan design；
- bounded repair；
- independent review。

Exit：

- canonical module release identity defined；
- R2 packaging acceptance executable；
- no forced microservice split。

### Wave 3 — Durable / Audit / Messaging
**2026-10-04 → 2026-10-10**

- P1-A/P1-B review；
- crash/recovery/replay/adversarial gaps；
- bounded repairs；
- real PostgreSQL lanes where required；
- independent review。

Exit：

- known crash boundary semantics documented/tested；
- Audit guarantees stated without overclaim；
- C0/C1 closed or module remains R1.

### Wave 4 — Document / Storage / Observability / Notification
**2026-10-11 → 2026-10-17**

- P1-C + P2 reviews；
- S3 provider capability matrix；
- document revision/ACL negative tests；
- OTel/notification provider seams；
- independent review。

Exit：

- provider-neutral contracts proven；
- no provider-specific type leakage；
- C0/C1 closed or explicit defer.

### Wave 5 — R2 Pilot Release
**2026-10-18 → 2026-10-23**

Pilot candidates：

1. `identity`；
2. `policy`；
3. `business-module-contracts` / compiler release bundle。

At least two modules must prove R2 end to end:

- independent module version；
- manifest；
- contract artifact；
- package digest；
- compatibility；
- migration mapping if persistent；
- changelog；
- contract tests；
- release artifact；
- consumer builds without concrete adapter dependency。

Final independent review decides whether the standard is practical before rolling R2 conversion across all modules.

## 7. 与 Contract Vertical Slice 的并行规则

为避免平台对齐阻塞业务交付：

- Wave 0/1/2 可与 Contract Vertical Slice 设计和实现并行；
- reference review 不持有 Contract 业务分支；
- 如果 Contract 暴露 Tenant/Policy/Document 的真实 C0/C1 gap，优先纳入对应 Wave；
- Contract slice 需要的 module contract 改动必须同时满足本计划的 review/test 规则；
- PLAN-0006 仍等待 Contract slice gate，不因 PLAN-0014 自动解锁。

## 8. 测试层级

每个模块按适用性执行：

### T0 — Static architecture

- dependency graph；
- no private persistence import；
- no provider SDK in core；
- manifest/contract consistency；
- tenant-required API context check。

### T1 — Domain/Application

- invariants；
- authorization decisions；
- idempotency；
- optimistic conflict；
- failure classification。

### T2 — Shared Contract Suite

同一 suite 对：

- Fake/In-memory；
- PostgreSQL；
- SQLite（若声明支持）；
- external provider adapter（若存在）执行。

### T3 — Integration

- real PostgreSQL；
- real S3-compatible；
- real broker/provider where activated；
- migration from previous version。

### T4 — Adversarial/Security

- cross-tenant；
- forged principal/resource；
- stale/revoked permission；
- path/object scope；
- duplicate/replay；
- failure injection。

### T5 — Recovery/Concurrency

- process crash；
- lease reclaim；
- fencing；
- multi-worker；
- restart；
- rolling version where applicable。

### T6 — Performance evidence

只针对关键路径：

- authorize P95/P99；
- identity resolve；
- document query；
- worker claim/throughput；
- object streaming memory；
- outbox backlog。

性能数据是 evidence，不虚构 SLO。

### T7 — Release compatibility

- package build；
- manifest/digest；
- old/new contract fixture；
- migration upgrade；
- rollback/forward-only statement；
- consumer compile/test against released contract artifact。

## 9. Independent Review gate

每个 Wave 结束必须有独立 reviewer 对固定 SHA 做 read-only review。

Fail conditions：

- unresolved C0；
- unresolved C1 却宣称 R2；
- Authority 被 reference/provider 改写；
- cross-tenant gap；
- private persistence dependency；
- provider-specific type leaked into public contract；
- failure/recovery semantics 未测试；
- reference revision 未固定；
- 文档写 PASS 但无执行证据。

C2/C3 可以接受，但必须进入明确 backlog。

## 10. R2 admission

从本计划开始，模块进入 R2 前必须同时满足：

1. SAAS_MODULE_STANDARD R1 requirements；
2. Reference Conformance Review PASS；
3. 至少两个同领域成熟 reference，其中至少一个真实生产型 OSS/平台；
4. C0=0、C1=0；
5. independent SemVer/release identity；
6. manifest + public contracts + digest；
7. migration/retention/rollback semantics；
8. shared contract tests；
9. independent review PASS；
10. release artifact reproducible。

## 11. Non-goals

本计划不：

- 一次部署 OpenFGA/OpenMeter/Infisical/Svix/Novu 等全部组件；
- 把每个 module 拆成微服务；
- 用 reference feature count 决定 architecture；
- 为没有真实消费者的 C2/C3 功能扩写通用框架；
- 重写 PLAN-0013 已验证的 IAM；
- 用 Temporal/Restate 替换现有 durable processing；
- 用 OpenFGA 替换 Policy authority；
- 用 OTel/immudb 替换 Audit authority。

## 12. Risks

### Reference overfitting

Mitigation：Authority-first；所有差异必须写 Decision，允许 KEEP/REJECT。

### Endless research

Mitigation：每个 Wave 固定日期、固定 reference、固定输出；不能为了“再找一个项目”延期。

### Parallel work conflict

Mitigation：review/doc/test changes优先；代码 repair 保持 bounded，与 Contract slice 避免同一热点文件。

### R2 packaging explosion

Mitigation：Wave 5 只 pilot 两到三个模块；验证标准后再推广。

## 13. Completion definition

PLAN-0014 完成要求：

- 六组 review 全部有固定 reference revision；
- 每组有 capability matrix 和 gap classification；
- 所有 C0 关闭；
- 所有进入 R2 的模块 C1 关闭；
- 至少两个 module 完成 R2 pilot；
- R2 artifact 可重复构建；
- module release version 与 workspace product version 明确分离；
- architecture fitness 增加必要门禁；
- workspace fmt/check/clippy/test 与相关 real integration lanes 绿色；
- 最终 independent review PASS；
- 未完成 C2/C3 进入明确后续计划/backlog；
- completion audit 记录真实 PASS/NOT RUN，不补造证据。

完成后本计划归档；后续每个新模块独立执行 Reference Conformance Review，而不再重复建立方法论。
