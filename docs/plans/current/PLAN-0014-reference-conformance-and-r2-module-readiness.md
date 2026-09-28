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

### Wave 3 execution record — 2026-09-27

分支 `feat/PLAN-0014-wave3-durable-audit-messaging-conformance`，Base
`443a11cabdce1dd34006feaf08aa8be3382150c1`（Wave 2 closeout 提交，Wave 2 本身仍
NOT INTEGRATED）；`main` 未修改、未 push、未合并、未启动 Wave 4/5、未激活
PLAN-0015/PLAN-0006。固定参考取 tag→SHA 双记录并源码取证：Temporal v1.32.0、
Restate v1.7.12（BSL，仅读语义）、Trigger.dev `trigger.dev@3.3.17`+`v4.6.4`、
jarvis-rs vendored 提交（私有、无 license，仅作 floor 对照）、ZITADEL v4.18.0、
immudb v1.11.2（BSL，仅读语义）、Infisical 0.165.16（audit 服务属 EE）、
Debezium v3.6.3.Final、NATS v2.15.0、Kafka 4.3.1；"AgentOS"（agent0ai/agent-os）
与 Quarkus outbox 序号列均 NOT FOUND，如实记录且不推断。

有证据支持的 C0/C1 以 failing-first 修复（提交 `217e950`、`4b4ee71`）：D-G01
取消事实性——意图（`processing_cancel_requested` 审计）与事实
（`document.processing.cancelled.v1` 事件 + `processing_cancelled` 审计）分离，
事实在全部转换路径（立即取消、step 边界、失败处置、release/reclaim 取消优先、
AI 取消终态）恰好一次；SQLite `cancel_factuality` 5 测试先红后绿，PG 镜像 5 测试
`#[ignore]` 并接入 CI。A-G01 document-sqlite 审计链写入静默断裂——17 列 legacy
INSERT（省略 chain 列）改为完整 20 列 v1 链写入（MAX+1 序号、genesis 回退、
`with_chain_metadata`），读路径对 legacy NULL 行归一化（对齐 PG 013/015 边界）；
`audit_chain` 3 测试先红后绿，PG `chain_pg` 3 测试镜像 ignored+接入 CI。D-G02
适配层 stale-fence/reclaim 负向直测补齐（`lease_fence` 4 测试绿）。architecture
fitness 对 dev-kind 测试夹具 Cargo 边给出原则性豁免（运行时组合边仍受约束，含
单测与标准文档 §23 同步）。

Gap registry：C0=0；C1 关闭×3（D-G01、A-G01、D-G02）；D-G06/A-G07 事件 envelope
缺 correlation/causation/producer 属 C1-by-standard，因改变公开契约面，注册为
独立设计提案（显式 blocker，不在本 Wave 静默改契约）；C2×9（D-G03 pipeline
version 列、D-G04 Attempt/provider 关联、D-G05 legacy claim-port seam、A-G02..
A-G06、A-G08）注册并附决定，参考模式 REJECT 清单（Temporal matching/sticky、
Restate journal 架构、Kafka transactional EOS、通用 DAG/调度器/registry/broker）
记录理由。失败矩阵 D01-D11 / A01-A08 逐行绑定测试路径+名称、命令、环境与结论，
缺口写 GAP/NOT RUN，不以 mock 充数。

门禁（本地 Windows，原始日志+退出码存 `docs/reviews/2026-09-27-wave3-gates/`）：
fmt/check/clippy(-D warnings)/test/architecture/`git diff --check` 全部 exit 0；
735 passed、50 ignored、0 failed；tamper-evidence 措辞经六参考校准无 overclaim，
尾部截断不可检测由测试断言固化。全部 PostgreSQL lane 本地 BLOCKED（本机 :5432
认证不可用，服务未动；专用测试库复跑命令见 gates/07），8 个新增 PG 测试在本会话
从未执行，历史 CI / 当前本地 / 当前集成三类证据分开记录。当前状态：`Wave 3
implementation reviewed / self-verified / READY FOR INDEPENDENT REVIEW /
NOT INTEGRATED`；R1 foundation，非 R2 admission，独立 reviewer verdict 待回填。
审阅记录 `docs/reviews/2026-09-27-saas-align-durable-execution.md`、
`docs/reviews/2026-09-27-saas-align-audit-messaging.md`。

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

## 2026-09-27 Wave 3 Round 2 correction

Round 1 HEAD `6b324ea76986640a3f1baef7ecebdb2c53fbd2c6` independent verdict: FAIL / NEEDS REPAIR. Document-only SQLite migration and audit legacy downgrade regressions are repaired with failing-first tests; acceptance matrices no longer substitute audit ordering/replay for messaging or crash recovery. D-G03 and D-G06/A-G07 remain open C1 with ADR-0027/0028 Proposed, NOT ACCEPTED. Historical C2-only/closed summaries above are superseded on these points. R1 / R2 BLOCKED; Wave 4/5 not started. Details: `docs/reviews/2026-09-27-wave3-round2-repair.md`; current gates are separate from Round 1 evidence.

## 2026-09-27 Wave 3 Round 2b 固化与真实 PostgreSQL 验证

- Round 2 修复已固化为本地 commit `81640e79147128925d10efb0f6ff30a0c568ec33`。入库前 `code-snapshot.json`
  十个哈希逐一与工作区字节一致；因仓库 `core.autocrlf=true` 且无 `.gitattributes`，其中 6 个 blob 以 CRLF 形式入库，
  换行归一化后与工作区文本逐字符等价（字节差恰为 CR 个数），无语义差异。`crates/document-sqlite/tests/operational.rs`
  为既有 EOL/stat 脏标记，工作区字节与 HEAD blob 相同，未 staged、未 reset/checkout/clean，也不计入本轮修复。
- 数据库授权：仅按操作者明确授权在共享 :5432 服务上**新建专用库**；未使用 `TEST_POSTGRESQL_URL` /
  `TEST_POSTGRESQL_RESTORED_URL` 指向的 `contract_management_*` 库（属另一项目 contract harness），
  未修改共享服务或认证配置，未猜测密码，日志全量脱敏。
- 环境：PostgreSQL 18.4，专用库 `plan0014_wave3_r2`，`cargo run -p migration -- --backend postgres up` 退出码 0，
  迁移 1–19，43 张 public 表。五条 lane 均 `--all-features -- --include-ignored --test-threads=1`：
  contracts 3 / cancel_factuality_pg 5 / chain_pg 3 / outbox_reliability 7 / inbox_idempotency 4，
  合计 **22 passed / 0 failed / 0 ignored**。
- 首次真实执行暴露两个只有 PostgreSQL 才能发现的缺陷，已在最终候选 `2f9976e1faa8a1262cf1aab6cd6554a38c7fc03a` 修复：
  1. **产品缺陷（新 C1，A-G09）**：`record_hash` 覆盖纳秒精度的 `occurred_at`/`recorded_at`，而 `TIMESTAMPTZ` 只存微秒，
     导致 PostgreSQL 上刚写入的审计行即无法自验，生产后端防篡改能力失效，而 741 个 SQLite/单元测试全绿。
     修复为 `audit::canonical_timestamp` 在签名时对齐微秒；`rehydrate` 不做归一化，历史行不重签名。
     修复前已写入 PostgreSQL 的行仍不可验证，属需另行评审的运维事项，不做自动重签名。
  2. **仅测试 fixture**：`cancel_factuality_pg` 向 `outbox_events.tenant_id`（migration 001 遗留 `VARCHAR(100)`）
     绑定 `Uuid`，PostgreSQL 以 SQLSTATE 42883 拒绝，SQLite 静默接受；生产写入方本就绑定文本。
- 重跑硬性前提：lane 需**初始为空的专用库**。`claim_next`/`claim_batch` 是全局队列，复用被中断残留数据污染的库会使
  `contracts`、`outbox_reliability` 失败；此为 fixture 隔离限制，已记录不掩盖。
- 最终候选重跑六项门禁全部退出码 0：`cargo test --workspace --all-features` 为 **742 passed / 0 failed / 50 ignored**，
  Architecture Fitness PASS。50 个 ignored 仍不计为已执行；CI 接线不等于本候选已通过 CI。
- 仍未执行/仍为 BLOCKED：进程级 kill 崩溃恢复（D04/D07 保持 PARTIAL，文件重开不等于进程杀死）、真实 provider 副作用与外部
  source pin、消息乱序与事件 Schema 演进（A04 仍 NOT PROVEN）、多进程多节点与滚动升级（D10 仅 PARTIAL）。
  D-G03 与 D-G06/A-G07 仍为未关闭 C1，ADR-0027/ADR-0028 仍 **Proposed / NOT ACCEPTED**。
- 状态：**READY FOR INDEPENDENT RE-REVIEW / NOT INTEGRATED**（PG BLOCKED 已解除）。实现者不自填独立 PASS；
  两份正式评审的独立记录仍为 `<pending>`。R1 仅在独立复审通过后成立，**R2 BLOCKED**；Wave 4/5 未启动，
  PLAN-0015 / PLAN-0006 未激活。

## 2026-09-27 Wave 3 Round 2c 审计时间精度回归修复

- 起点 HEAD `0ac031af1344af4a8fa4d91c314cf3563065e1bf`（Round 2b closeout）。本轮只处理审计时间规范化、历史恢复与相应回归证据。
- **R1（产品缺陷，Round 2b 引入的回归）**：`AuditEvent::rehydrate` 通过 `Self::new` 做校验，而 `new` 会规范化 `occurred_at`；
  `rehydrate` 只回写 `recorded_at`，未回写原始 `occurred_at`。SQLite 以 RFC 3339 文本存储时间，历史行可带纳秒尾数，
  读取时被截断导致 `hash_record` 不再等于库中 `record_hash`，`verify_chain` 把完好的历史链判为损坏。
  Round 2b 的记录与持久化架构文档当时写的是“`rehydrate` 不做归一化”，**代码与文档不一致**——文档描述的是应有行为，代码未实现。
  修复：`rehydrate` 在 `new` 完成全部领域校验后，按存储值原样回写 `occurred_at` 与 `recorded_at`。
- **R2（产品缺陷）**：`canonical_timestamp` 用 `Utc.timestamp_nanos(timestamp_micros() * 1_000)` 重建时刻；
  epoch 微秒数 ×1000 在公元 2262-04-11 之后超出 `i64` 纳秒范围，debug 下 panic、release 下回绕后进入签名。
  修复：只原位截断亚秒部分（`with_nanosecond(timestamp_subsec_nanos() / 1_000 * 1_000)`），不做 epoch 级乘法，
  任意可表示时刻的日期与整秒均不变（含 1970 年以前）。
- **顺带加固**：`with_chain_metadata` 原先只规范化 `recorded_at`。它是“即将写入并签名”的转换点，
  若事件经 `rehydrate` 携带历史纳秒精度再签名，会在 PostgreSQL 上重现不可验证行缺陷；现同时规范化 `occurred_at`。
- 未改变哈希算法、canonical payload、`chain_version` 语义、genesis、排序或历史签名；未重签名任何历史记录；无迁移触碰历史时间戳。
- 失败先行证据：`docs/reviews/2026-09-27-wave3-round2c-gates/`。`00` 为两项修复同时回退时的失败复现
  （溢出 panic + `occurred_at` 未按存储值恢复），`01` 为仅回退 R1 时 SQLite 真实 repository 层失败，
  `superseded-intermediate-run/` 保留被加固改动超越的中间一次 lane 运行及其原因。
  历史 `record_hash` 期望值 `12b8823d…f815` 为按修复前历史算法冻结、并在本 crate 之外独立重算校验的常量，不由被测构造路径生成。
- 测试：audit 单测新增历史恢复、边界范围、重签名三类；`crates/audit-sqlite/tests/historical_chain.rs`
  经真实 `SqliteAuditStore` + 真实迁移目录验证纳秒历史行读回后摘要复现、链校验通过，且新的微秒精度写入可链入其后并含篡改负例；
  `chain_pg.rs` 新增真实 PostgreSQL 精度往返测试。
- 验证：`cargo test -p audit -p audit-sqlite -p document-sqlite --all-features` 25 passed / 0 failed；
  任务专用空库 `plan0014_wave3_r2final`（PostgreSQL 18.4，`occurred_at`/`recorded_at` 实测 `datetime_precision = 6`）
  上迁移 CLI 退出码 0，五条 lane **23 passed / 0 failed / 0 ignored**（contracts 3、cancel_factuality_pg 5、chain_pg 4、
  outbox_reliability 7、inbox_idempotency 4）。六项完整门禁结果见同一目录 `10`–`15`。
- **Round 2b 的 22 项 PostgreSQL 通过不覆盖本次历史恢复缺陷**：PostgreSQL 存储即微秒，无纳秒尾数可被读回破坏；
  只有 SQLite 文本存储能保存会被错误归一化的历史，而当时没有测试读回这类行。
- 提交链：base `0ac031af1344af4a8fa4d91c314cf3563065e1bf` → 代码候选 `8946e46b03ac5516e7bf6e09f4f58382aafd4bd3`
  → 文档 closeout `016b9ed070bfc64787ac4c7f58248db0cb7f698a` → 本提交（仅记录提交链的指针提交，其自身 SHA 见交付报告与 `git log`）。
- 状态：**READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**。实现者不自填独立 PASS；两份 Wave 3 评审的独立记录仍为 `<pending>`。
  R1 声明仍需独立复审；**R2 BLOCKED**；Wave 4/5 未启动；ADR-0027/ADR-0028 仍 Proposed / NOT ACCEPTED。
  修复前写入 PostgreSQL 的不可验证行仍属需另行评审的运维事项，本轮不枚举、不重签名。
