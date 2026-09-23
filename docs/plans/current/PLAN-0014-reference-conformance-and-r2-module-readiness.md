# PLAN-0014: Reference Conformance and R2 Module Readiness

> Status: Active / Review-first  
> Revision: 0  
> Date: 2026-09-23  
> Target close: 2026-10-23  
> Owner: Platform Architecture + Module Owners  
> Planning Base: main at `5bd9b88a7b83e811aeec7087215b4bc5c004f4cf`  
> Architecture inputs: SAAS_PLATFORM_ARCHITECTURE, SAAS_MODULE_STANDARD, BUSINESS_APPLICATION_PLATFORM_ARCHITECTURE, ADR-0020/0021/0022/0024  
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
  2026-09-xx-saas-align-iam-policy.md
  2026-09-xx-saas-align-module-packaging.md
  2026-10-xx-saas-align-durable-execution.md
  2026-10-xx-saas-align-audit-messaging.md
  2026-10-xx-saas-align-document-storage.md
  2026-10-xx-saas-align-observability-notification.md
```

使用：

`docs/templates/REFERENCE_CONFORMANCE_REVIEW_TEMPLATE.md`

每份 review 必须记录 fixed reference revisions，不允许只引用项目首页。

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
