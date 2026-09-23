# 架构实施状态

> 文档类型：Living Document
> 最后更新：2026-09-23
> 当前阶段：Enterprise AI SaaS Platform Baseline 已对齐；PLAN-0014 Reference Conformance / R2 Readiness Active；Contract 真实垂直切片仍是下一主要业务实现
> 当前计划：PLAN-0014 Reference Conformance / R2 Readiness Active（review-first，2026-09-23→2026-10-23）；Contract Business Vertical Slice 仍是下一主要业务代码计划；PLAN-0006 Revision 1 Proposed / BLOCKED
> 集成方式：GitHub PR merge
> Analytics/Visualization：Baseline 已建立，运行时尚未实现
> SaaS Platform：2026-09-23 经 ADR-0025 Accepted，`SAAS_PLATFORM_ARCHITECTURE.md` 与 `SAAS_MODULE_STANDARD.md` 已晋升 Baseline；复用现有 BusinessModuleManifest/compiler 与 PLAN-0013 IAM，不激活新 runtime
> Reference Alignment：PLAN-0014 已排期，先 IAM/Policy 与 Module Packaging，再 Durable/Audit、Document/Storage、Observability/Notification，最后至少两个模块完成 R2 pilot

> 2026-08-03: PLAN-0001 and PLAN-0002 are Integrated and archived. PLAN-0002
> was fast-forwarded at `ad47544505b66d577ccdcb8f300812c294d3d7bf`; main CI
> run 30784568762 passed all six jobs, including PostgreSQL, MinIO, Document
> E2E, and architecture fitness. The repository is in Phase 2 preparation.
>
> 2026-08-03: PLAN-0003 Revision 1 is Integrated and archived at
> `f6dbc693da42d0f9a7566739f5a4169c6a86f880` via local solo fast-forward. Main
> CI run 30796583865 passed all six jobs, including PostgreSQL, MinIO, Document
> E2E, and architecture fitness. At that historical checkpoint Search remained
> Deferred and PLAN-0004 had not started.
>
> 2026-08-03: PLAN-0004 Revision 1 reached Accepted Candidate at
> implementation `a0bb0ad9374e87b1225e26ccbfbd44f4d616ebf2`. Feature CI run
> `30833527820` passed all six jobs, including PostgreSQL, MinIO, the
> multi-process crash/reclaim E2E, and Architecture Fitness. SQLite remains
> local single-process and PostgreSQL is the production multi-worker authority;
> no PR or main integration is used.
>
> 2026-08-04: PLAN-0004 Revision 1 was integrated and archived by local solo
> fast-forward at main `12454709a88fde16f7769af27a75e79c4bc0981a`. Feature CI
> `30833916455` and Main CI `30868701290` passed all six jobs, including the
> GitHub Linux PostgreSQL/MinIO E2E and Architecture Fitness. Local SQLite
> process E2E passed; Windows PostgreSQL/MinIO remains NOT RUN.
>
> 2026-08-04: PLAN-0005 started from main `6fd065c33471665e828a3da3a2cc3fae8d6d2afc`.
> Runtime Governance owns Audit, integrity scan/finding, repair run and ledger
> state; Document Intelligence remains the owner of processing state.
>
> 2026-08-04: PLAN-0005 reached Accepted Candidate at implementation
> `71a2a8495033aec7d8e40752fc80fd2ba30dc485`. Feature CI `30879843925` passed
> format, check, clippy, workspace tests, Architecture Fitness, and the real
> PostgreSQL/MinIO governance E2E. Windows PostgreSQL/MinIO remains NOT RUN;
> metadata-only text-artifact verification and the separate owner/Governance
> local transaction boundary are recorded accepted risks in the current plan;
> SQLite also proves expired-lease reclaim and stale-fence rejection.
>
> 2026-08-04: PLAN-0005 was reopened as Revision 1, `Active`, for Governance
> trust and correctness hardening. The previous implementation candidate is
> `71a2a8495033aec7d8e40752fc80fd2ba30dc485` and the previous evidence HEAD is
> `ab01288cd3477107c567e342ff45b7f624ec5396`. It is not an integration
> candidate until the Revision 1 evidence and Feature CI are green.
>
> 2026-08-04: Revision 1 local evidence is green for formatting, workspace
> check, Clippy, workspace tests, the SQLite Governance E2E, management
> security tests, and Architecture Fitness. Feature CI remains PENDING for
> this revision; PostgreSQL/MinIO integration cases remain NOT RUN locally.
> Implementation evidence is committed at `f1e10a0942c18715f5591b56619d5c6dae21f06e`;
> the push attempt could not reach GitHub HTTPS/443, so no new Feature CI run
> was created.
>
> 2026-08-06: PLAN-0005 Revision 2 was integrated and archived at main
> `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac` by solo fast-forward. Implementation
> SHA is `24e70f4182ca3315d94033178952113c4faba717`; Candidate SHA is
> `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac`; Main CI `31026047403` passed all
> six jobs, including GitHub Linux PostgreSQL/MinIO/E2E and Architecture Fitness.
> Evidence CI `31022731371` and Feature CI `31021778597` also passed. Windows
> PostgreSQL/MinIO remains NOT RUN.
>
> Runtime Audit, Integrity Finding, Controlled Repair, Repair Ledger, and
> Lease/Fence Recovery are integrated. The accepted boundaries remain:
> PROC-INT-008 is metadata/checkpoint based; the hash chain is tamper evidence,
> not WORM; and cross-bounded-context coordination uses local transactions,
> leases, idempotency, and reconciliation rather than distributed transactions.
>
> 2026-08-06: Cloudflare OS was reviewed as an Enterprise AI Workspace reference.
> ADR-0018 accepts an explicit Workspace product layer, task-scoped Capability
> Grants, Agent Observation lineage, non-authoritative Artifact/Blueprint state,
> and a future independent Generated App sandbox. Cloudflare OS remains a
> reference only and is not a runtime dependency. PLAN-0006 is Proposed on the
> documentation branch; no Agent implementation, activation, integration, or
> PLAN-0007 start has occurred.

> 2026-08-07: PLAN-0007 is Active for the external Business API surface, independent React console, remote CLI, and read-only MCP adapter. PLAN-0006 remains Proposed / NOT ACTIVE; no Workspace or Capability runtime is introduced.

> 2026-08-10: PLAN-0008 Final Candidate HEAD
> `7eb5421e492a11c0ac20b17f8fd5c3a034f7a29b` was integrated by local solo
> fast-forward from Base `35d1d01fd49a70ee996fbb5fb72818a632989efe`. Feature CI
> `31353149398` and Main CI `31353409550` passed all jobs, including GitHub
> PostgreSQL/MinIO, migration, revision/evidence, concurrency, retry, crash
> recovery, multi-process E2E, and Architecture Fitness. Local PostgreSQL/MinIO
> remains NOT RUN. PLAN-0008 is Integrated / Archived; PLAN-0009 is Completed /
> Rehearsal Closed and archived, with production migration NOT GRANTED; PLAN-0006
> remains Proposed / NOT ACTIVE.

2026-08-11: Canner/WrenAI was registered as a pinned reference at commit
`ec85b1e1589ad2b6981d08df1f6b2ad29ae5b902`. ADR-0020 accepts Business Module
Isolation and Semantic Contract as two complementary platform seams. The first
PLAN-0010 candidate was invalidated during review because PR #7 is based on
GitHub `origin/main` `654fe83d82107d899079d20e5fef8aaf4d5431b8`, while its
declared local base `f09d2a5` contains the PLAN-0009 rehearsal history. A clean
candidate was reconstructed from the actual GitHub base at implementation SHA
`7997a501528bf12ae7846a9dc278fe4fce65a467`; no WrenAI runtime, Python,
database, migration, API, Worker, C ACL or business crate relocation is
authorized in this plan.

Revision 1 records the Audit history boundary explicitly: migration 013
backfills deterministic tenant-local sequence values but marks pre-existing
rows `chain_version=0`; only new sequence-based rows are chain-protected.
The processing state matrix is now the owner rule for PROC-INT-006, while
PROC-INT-008 remains PARTIAL (metadata/checkpoint only; object-store probing
is deferred). Resolved findings reopen as explicit recurrence episodes.

> 2026-08-13: ADR-0021 and ADR-0022 are Accepted, PLAN-0010 is Integrated / Archived,
> PLAN-0009 is Completed / Rehearsal Closed / Archived, and PLAN-0006 remains
> Proposed / NOT ACTIVE. PLAN-0011's activation gate is satisfied and PLAN-0011 is
> now Active. Stage 2 Luna activation is documentation-only at exact base
> `be7524974d70c4eef58111106c79f68e94f2cd3b`; no Rust, code, runtime, migration,
> API, Worker, or dependency implementation is included.

> 2026-08-19: PLAN-0011 reached Accepted Candidate at
> `ed870acfe165756632c0519bb181fd5dcf8a11cd` after bounded repair and independent
> REVIEW-C PASS over the exact implementation range. Feature CI `32210387950` and
> Main CI `32213985080` passed all configured jobs, including Architecture Fitness,
> frontend/CLI/MCP contracts, PostgreSQL/MinIO and E2E. The candidate was integrated
> into main by solo fast-forward. PLAN-0011 is now Integrated / Archived. The
> Business Application Platform document is promoted to Baseline; no runtime,
> dynamic plugin, Marketplace, database migration, PLAN-0006 implementation,
> PLAN-0009 production migration or concrete business module was introduced.

> 2026-08-25: PLAN-0012 M0 完成收支——PLAN-0007 completion audit（
> `docs/reports/PLAN-0007-COMPLETION-AUDIT.md`）逐项核对完成定义与六项交付切片，
> 全部门禁 PASS 或带原因 NOT RUN（本机 PostgreSQL/MinIO 与 Playwright 因沙箱/环境
> NOT RUN，GitHub CI 覆盖）。PLAN-0007 归档至 `docs/plans/archive/2026/`，状态
> Integrated / Archived（implementation `ec6cff141a89dcdf5de2f2ea2b8b001384f88755`）。
> 本里程碑仅文档收尾，未修改业务代码、Cargo.toml、迁移或 openapi.json。PLAN-0012
> 为当前唯一 Active 计划，M1（model-provider 集成决策与 ADR-0023）可从干净起点启动。

> 2026-08-30: PLAN-0012 M2 全部完成（T2.1–T2.5）。vendored jarvis-model-provider
> 快照升级至上游 `af9fbe7`（0.3.0-dev.1，无本地魔改）；新增 fail-closed 的
> `allow_private_http` 配置映射上游 `EndpointPolicy::TrustedPrivateHttp`，默认仍为
> HTTPS-or-loopback 拒绝语义。真实 provider smoke 通过（内网 vLLM qwen3_vl，
> OpenAI-compatible；提取 title/language/fields/warnings 正常，证据见 ADR-0023
> 第 7/8 节）。全仓 fmt/check/clippy/test --no-fail-fast 与
> check-architecture.ps1 PASS；agent-adapter 上游失败测试在本机沙箱存在经基线
> 复现确认的既有 flake（连接 `127.0.0.1:9` 被代答 -32003），单包隔离通过，由 CI
> 覆盖。PLAN-0012 剩余：M3（真实认证）、M4（预生产环境与可观测性）、M5（v0.1
> 发布审计）。

> 2026-08-30: PLAN-0012 M3 核心完成（T3.1/T3.2/T3.4）+ T4.4。Business API 生产
> 认证落地：`OidcValidator`（`apps/business-api/src/oidc.rs`）经 issuer JWKS 验证
> Bearer JWT（OIDC discovery 默认、`auth.jwks_url` 覆盖、TTL 缓存、未知 kid 即时
> 刷新），强制 exp/iss/aud（可配 audience），仅 ES256/RS256；dev auth 关闭时
> `auth.issuer_url` 由配置校验强制；JWKS 故障 fail-closed；声明 `tenant_id`/
> `user_id`/`management_permissions`/`roles` 映射到既有 TenantContext/
> ManagementPermission，未识别权限不授予。13 例契约测试覆盖有效/过期/错
> audience/错 issuer/篡改/未知 kid/缺失或 nil tenant/JWKS 故障/alg=none 与声明
> 映射。CLI/MCP bearer token 已参数化，无需改动。CI 新增 `security` job
> （cargo-audit/gitleaks/trivy），本地 NOT RUN 由 GitHub CI 首跑验证。T3.3
> （IdP demo compose + console 登录）按计划风险缓解后置。全仓门禁：fmt/check/
> clippy/test --no-fail-fast（495 pass / 0 fail）与 check-architecture.ps1 PASS。
> PLAN-0012 剩余：T3.3、T4.1/T4.2/T4.3/T4.5、M5 发布审计。

> 2026-08-30 (2): PLAN-0012 T4.1/T4.3/T4.5 完成。可观测性：`observability::LogFormat`
> （text/json fail-fast），四个进程统一 `log_format` 配置，JSON 单行日志可供
> 预生产采集。备份/恢复：`deploy/operations/drill-backup-restore.sh` 一体化演练
> 脚本（seed→pg_dump/mc mirror→恢复到 `*_restore`→行数/表数/对象 roundtrip 校验），
> 本机执行 NOT RUN（无本地 PostgreSQL/MinIO）。Runbook v0.1：`docs/operations/
> RUNBOOK.md`（部署/就绪/升级/回滚/备份/故障处置/安全基线/已知缺口）。

> 2026-08-30 (3): PLAN-0012 T4.2 v0.1 完成。`business-api` 新增公共 `/metrics`
> Prometheus 文本端点（metrics-exporter-prometheus，无全局标签泄漏；标签有界：
> method + 数值 status、认证失败 reason 类别），`http_requests_total`、
> `http_request_duration_seconds`、`auth_failures_total` 已接入；契约测试验证
> 端点公开性与计数器出现。worker 侧吞吐/租约/AI 时延指标与 dashboard 配置为
> 后续批次。PLAN-0012 剩余：T3.3（IdP compose + console 登录）、M5 发布审计
> （T4.2 worker 指标与 T4.3 演练首跑在 M5 一并收口）。

> 2026-09-02: PLAN-0012 发布加固批次（v0.1 候选）完成代码面收口。
> ① 备份演练脚本重写：seed 标记先于备份并校验 checksum/size、唯一安全
> drill 子目录、恢复到唯一 restore bucket、从恢复目标验证、trap 清理、
> 禁止对未验证 BACKUP_DIR 执行删除（本机执行 NOT RUN：无 docker/psql/
> pg_dump/mc，由 CI service containers 补跑）。② OIDC 生产加固：
> audience 必填、issuer/jwks HTTPS-only、JWKS/discovery 不跟随重定向、
> 生产 transport fail-closed，真实 OIDC principal 跨租户契约测试。
> ③ AI Provider 重试语义：429 保留并钳制 Retry-After、Timeout/5xx 平台
> backoff、Authentication/InvalidRequest 不重试，端到端 ProviderError→
> disposition 测试，核心层无 provider 类型。④ 可观测性：生产强制 JSON
> 日志（三进程 fail-closed + 回归测试）、`x-request-id`→correlation_id
> 贯穿 Job/AI Task/审计/worker 日志（migration 018/SQLite 008）、HTTP
> method 归一到固定集合、worker `/metrics`（`observability.metrics_addr`
> 生产必填）新增吞吐/排队/lease 丢失与回收/重试 disposition/AI 时延/
> 429/5xx 指标（标签全部代码枚举，无 tenant/文档/路径/模型输出），
> `deploy/observability/` 提供 Prometheus 抓取配置与最小 Grafana
> dashboard。⑤ 稳定性：agent-adapter 固定端口 flake 测试替换为进程内
> stub server（连接拒绝/上游 5xx/协议错误/正常返回四态）。
> PLAN-0012 剩余：T3.3 后置、M5 发布审计（性能 smoke、预生产全链路
> 演练、v0.1 tag）。

> 2026-09-02 (2): PLAN-0012 Release Closure（`codex/plan-0012-release-closure`）
> 完成审阅修复与全仓验证。① CI 供应链：trivy 0.74.0 与演练用 MinIO mc
> RELEASE.2025-08-13T08-35-41Z 固定为不可变 GitHub release 资产，SHA-256
> 显式维护、下载后执行前校验，不匹配立即中止（维护策略注释在 ci.yml）。
> ② ai-worker：`TaskOutcome::Succeeded` 移至 fenced completion 持久成功之后；
> completion 被 fence（`LeaseLost`）改记 `lease_unproven` + lease lost，
> Unavailable/Failed 等其他持久化错误改记 `failed` 且不增加 lease lost
> （最终归属由 2026-09-03 条目 `0809f83` 收口），保证每 attempt 恰好一个最终
> outcome（进程内 CountingRecorder 回归测试，red→green 验证）。③ business-api-client 代理测试保存/恢复代理环境变量并以进程级
> 互斥串行化，消除全局环境污染与并行竞争。④ 生产配置 fail-closed 拒绝
> 空白/纯空白 `auth.jwks_url`（新增配置测试）。⑤ RUNBOOK/本状态/计划/完成
> 审计同步。Slice B 门禁全 PASS：fmt/check/clippy `-D warnings`/test
> `--workspace --all-features`（本机）、check-architecture.ps1、
> check-openapi.ps1、`DRILL_SELFTEST=1` 演练自检。Slice C（预生产验收：真实
> IdP/model-provider、20 并发性能 smoke、全链路演练、Prometheus/Grafana
> 抓取与标签基数验证）因本工作区无 staging 与真实凭据，按门禁要求标记
> BLOCKED/NOT RUN，不以 fake/stub 替代证据。变更已合入 main（PR #9，
> merge `eb62451`，Main CI `33637882962` 全绿），但 v0.1 tag 与 PLAN-0012
> 归档继续推迟，唯一未决条件为 Slice C 在预生产环境以真实证据 PASS。

> 2026-09-03: PLAN-0012 最终审阅修复（`codex/plan-0012-slice-c-staging`，
> `0809f83`）：细化 ai-worker completion 边界的持久化错误归属——
> `complete_ai_and_resume` 成功记 `succeeded`；`ProcessingRepositoryError::LeaseLost`
> 记 `lease_unproven` + `ai_lease_lost_total`；Unavailable/Failed 及其他持久化
> 错误记 `failed` 且不增加 `ai_lease_lost_total`。进程内 CountingRecorder 回归
> 测试对 fenced / persistence failure / durable success 三场景逐项断言计数器，
> 并断言每 attempt 最终 outcome 总数恒为 1。页头集成方式据 PR #9 事实修正为
> GitHub PR merge。Slice B 门禁结果与 Slice C 状态记录于完成审计；Slice C
> （预生产验收）因本工作区无 staging 与真实凭据仍 BLOCKED/NOT RUN，不以
> fake/stub 替代证据。

> 2026-09-03: PLAN-0012 Slice C 真实预生产验收通过并发布 v0.1。staging 真实栈
> （PostgreSQL 18 + MinIO + 内网 vLLM 真实推理 + Prometheus/Grafana）全链路
> 上传→真实提取→Review→崩溃恢复→备份恢复、20 并发 smoke×2、抓取/仪表盘/
> 标签基数逐项 PASS（含直方图 buckets 修复 `ac58991` 与契约套件隔离确定性
> 修复 `d0076e0`/`3ab22dd`）；真实 Auth0 租户 production 模式 E2E 12/12 PASS、
> 8 项 fail-closed 启动负例全部拒绝。逐项证据与 caveat 见完成审计两次
> amendment（`docs/reports/PLAN-0012-COMPLETION-AUDIT.md`）。PR #10 合入 main
> （merge `2383651`），Main CI `33705531597` 全绿后执行发布：`v0.1` annotated
> tag 指向 `2383651`，PLAN-0012 归档至 `docs/plans/archive/2026/`，T3.3
> （Keycloak demo compose）按计划后置。ADR-0024（认证外置、授权内置）经
> PR #12 Accepted。

> 2026-09-18: 发布后文档同步修正（纯文档，无代码/迁移/契约变更）：
> ① `docs/plans/README.md` 归档清单补记 PLAN-0012 条目（归档提交当时仅更新了
> current 列表）；② 本文头部此前停留在"PLAN-0012 Active"，现更正为 v0.1 已
> 发布、PLAN-0012 Integrated / Archived，当前计划仅 PLAN-0006
> （Proposed / NOT ACTIVE）；③ §7 补 PLAN-0012 判定行并修正外部扫描门禁表述
> （cargo-audit/gitleaks/trivy 已入 CI security job 并 PASS，run
> `33617370509`；cargo-deny/syft/grype/osv-scanner 仍未接入）；④ ADR 索引补记
> ADR-0006/0007 为历史跳号（仓库历史从未创建，不得复用）。

> 2026-09-18: CI 供应链/可用性修复两项（PR #14，merge `732ee1d`，
> Main CI `35304693307` 全绿）。① rustls 0.23.43 → 0.23.45 清除
> RUSTSEC-2026-0285（TLS 1.3 握手消息跨加密级别接受，CVSS 5.3，
> 2026-09-14 发布；上次 Main 全绿为 09-03，属新增命中），同线补丁升级，
> 本机全仓 `cargo check --workspace --all-targets` 与
> `cargo test --workspace --all-features`（131 套件 ok / 540 tests）PASS；
> ② Docker Hub `minio/minio`/`minio/mc` 镜像仓库已整体下线（Hub API 404，
> 非匿名限流；E2E job 连续两轮在 Initialize containers 阶段复现），CI
> service、`scripts/test-postgres-minio-multiprocess.sh` 与
> `deploy/{docker-compose.yml,dev,demo}` 全部迁移至 quay.io，所用每个 tag
> 均经 Quay v2 API 匿名 token 校验 manifest 存在（200）；compose 的
> `mc:2024-06-13T21-20-15Z` 未在 Quay 发布（404），改用 CI 已验证的
> `2024-06-12T14-34-03Z`。`mc` 二进制下载（GitHub release + SHA-256 固定）
> 不受影响。PR 合并需修改 `.github/workflows` 时须经具备 `workflow` scope
> 的通道推送（当前 gh OAuth token 无该 scope，本次经 SSH 推送）。

> 2026-09-18: post-v0.1 全仓审阅后调整实施路线。审阅确认 production OIDC/JWT、
> tenant/user principal 和少量 ManagementPermission 已运行，但 `crates/identity`
> 与 `crates/organization` 仍为 TODO 骨架，缺少 PlatformUser/TenantMembership、
> Role/Permission/RoleBinding/ResourceScope 和统一 Policy Decision；Contract 等真实
> 业务 crate 也尚未形成可验证垂直切片。因此新增
> `IDENTITY_AND_AUTHORIZATION_ARCHITECTURE.md` 与 PLAN-0013（2026-09-19 经 PR #17
> 合入 main，Integrated / Archived），并将 PLAN-0006 重写为 Revision 1 / Proposed / BLOCKED：
> 只有 PLAN-0013 与首个 Contract Business Vertical Slice 集成后才能激活。
> 路线变为 Identity/Authorization → Contract →（按需 Knowledge/Evidence）→
> Workspace/Agent → Controlled Write ActionPlan → Analytics/跨部门业务。
> 同步吸收 Ever Gauzy 作为 P0 Business Platform reference：采用 Provider
> definition/binding 分离、compiled tool catalog + per-turn resolution 等概念，
> 拒绝其内部 OAuth credential issuer 与 AGPL runtime/code 复用。本轮仅文档/
> 架构/计划收口，不修改 runtime code、migration、OpenAPI 或数据库。

## 1. 当前权威结论

- Rust 业务平台是系统主体，Agent 和 Enterprise AI Workspace 是可选产品层；
- 关闭 Workspace/Agent Runtime 后，Web、OpenAPI、Worker 和业务流程必须继续运行；
- 服务端采用战略 DDD 主导的模块化单体；
- 服务端内部采用 Domain、Application、Delivery、Infrastructure 和 Composition Root 分层；
- 业务能力、统一语言和数据所有权决定 Bounded Context；
- 核心层只表达业务和通用能力语义；
- 基础设施产品通过适配器、配置、部署和 ADR 接入；
- 战术 DDD 按复杂度采用，不对简单 CRUD 过度建模；
- 每份可变权威业务数据只有一个拥有上下文；
- 同一上下文内优先本地事务，跨上下文通过事件、幂等、Process Manager 和补偿协作；
- 长时任务区分业务状态、业务流程状态、人工工作流和执行机制状态；
- API、事件、安全、质量属性、部署和可观测性属于正式架构资产；
- Web、Worker、OpenAPI 和 Agent 复用 Application 用例；
- Agent 使用原用户委托身份，但每个任务还必须获得更窄的 Capability Grant；
- Workspace、Skill、Context、Observation、Artifact 和 Generated App 不拥有正式业务事实；
- Agent 生成代码不得进入核心业务进程；
- 后续任务必须通过架构 Fitness Functions 提供持续符合证据。

## 2. 完整服务端架构文档集

入口：

- `docs/architecture/BACKEND_ARCHITECTURE_MANIFEST.md`

总体与专题 Baseline：

- `docs/architecture/SERVER_BACKEND_ARCHITECTURE.md`
- `docs/architecture/BOUNDED_CONTEXT_MAP.md`
- `docs/architecture/DATA_OWNERSHIP_AND_CONSISTENCY.md`
- `docs/architecture/WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md`
- `docs/architecture/ENTERPRISE_AI_WORKSPACE_ARCHITECTURE.md`
- `docs/architecture/QUALITY_ATTRIBUTE_SCENARIOS.md`
- `docs/architecture/SECURITY_ARCHITECTURE.md`
- `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`
- `docs/architecture/LEGACY_MIGRATION_ARCHITECTURE.md`
- `docs/architecture/CODE_ARCHITECTURE.md`
- `docs/architecture/PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md`
- `docs/architecture/DATA_GOVERNANCE_ANALYTICS_AND_VISUALIZATION_ARCHITECTURE.md`
- `docs/architecture/BUSINESS_MODULE_ISOLATION_AND_SEMANTIC_CONTRACT_ARCHITECTURE.md`
- `docs/architecture/DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`
- `docs/architecture/RUNTIME_AUDIT_ARCHITECTURE.md`
- `docs/architecture/DATA_INTEGRITY_AND_REPAIR_ARCHITECTURE.md`
- `docs/architecture/AUDIT_RETENTION_AND_TAMPER_EVIDENCE.md`

标准：

- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`
- `docs/standards/RUST_CODING_STANDARD.md`
- `docs/standards/QUERY_MODEL_AND_DATABASE_ADAPTER_STANDARD.md`

关键决策：

- `docs/adr/ADR-0003-domain-driven-layered-backend.md`
- `docs/adr/ADR-0010-durable-processing-job-and-fixed-pipeline.md`
- `docs/adr/ADR-0011-worker-leases-fencing-and-crash-recovery.md`
- `docs/adr/ADR-0013-unified-runtime-audit-model.md`
- `docs/adr/ADR-0017-platform-native-analytics-and-visualization.md`
- `docs/adr/ADR-0018-enterprise-ai-workspace-and-capability-security.md`
- `docs/adr/ADR-0020-business-module-isolation-and-semantic-contract.md`

外部参考和审查：

- `docs/reference/CLOUDFLARE_OS_REFERENCE_ANALYSIS.md`
- `docs/reference/WRENAI_REFERENCE_ANALYSIS.md`
- `docs/reviews/2026-08-06-cloudflare-os-and-enterprise-ai-workspace-gap-review.md`

## 3. 当前实现状态

当前仓库已完成 Phase 1 Foundation Integrity、Phase 2 Persistence and Query
Hardening、Phase 3 First Durable Business Flow 与 Phase 4 Runtime Governance
Foundation；PLAN-0001 至 PLAN-0005 均已集成并归档。

已具备：

- `apps/*` 和领域/能力 crate 初始划分；
- Domain/Application/Infrastructure/API 的设计方向；
- 统一配置、错误、对象存储和消息基础；
- PostgreSQL 生产权威与 SQLite 本地适配；
- Document Management 最小垂直切片；
- 固定 Durable Document Processing Pipeline；
- Worker Lease/Fence/Heartbeat/Crash Recovery；
- AI Task、Candidate 和 Review 基础；
- Runtime Audit、Integrity Finding、Controlled Repair 和 Repair Ledger；
- 真实 PostgreSQL/MinIO E2E 和 Architecture Fitness CI；
- 完整服务端架构 Baseline 和文档治理；
- production OIDC/JWT authentication、可信 TenantContext/AuthenticatedPrincipal 与少量 Governance ManagementPermission。
- Business Module Isolation 与 Semantic Contract Baseline、ADR-0020、纯 Rust contract/compiler
  和确定性编译测试；现有业务 crate 尚未迁入 `modules/` 目录。

Architecture Foundation Convergence 已集成文档基础和 PLAN-0011 foundation：Business Application Platform
边界、Inter-Module Communication、Process Manager/Saga、Published Extension Point、
Package/Dry Plan、UI/Agent/Semantic contribution 三层边界和 module-a/b/extension
synthetic validation 已形成；ADR-0021/0022 已 Accepted，PLAN-0011 已 Integrated / Archived。
纯 Rust contract/compiler/dry-plan/fitness foundation 已实现；不得把这些文档或代码
描述为已实现 Registry、安装器、卸载器、Saga runtime 或动态插件。

Identity / Authorization 现状：

- ADR-0024 已接受，production OIDC authentication 已实现；
- PLAN-0013（Identity and Authorization Foundation）已 Integrated / Archived：实施分支
  `feat/PLAN-0013-identity-and-authorization-foundation` 经独立审阅修订（C1 + M1-M5）后，
  以代码 head `1990a33` 经 PR #17 合入 main（merge `d3a9196`，Main CI `35426688873` 全绿）：
  `crates/identity`、`crates/organization`、`crates/policy` 承载
  PlatformUser/TenantMembership/OrganizationUnit+Member/Role/Permission/
  RoleBinding/ResourceScope 与统一 default-DENY Policy（Authorize/
  ExplainDecision，15 个有界 DecisionReason），各带 PostgreSQL authority +
  SQLite 本地单进程 adapter 与跨 dialect 契约套件；server-configured
  bootstrap admin（无 first-user-is-admin）；Governance API 已迁移到统一
  Authorize + 兼容桥（仅 7 个 governance 键，flag 默认开启）；Stage 8 admin
  REST 23 操作 + public contracts/OpenAPI/client + Stage 9 Console 页面已实现；
- 全部门禁通过（含真实 PostgreSQL + MinIO + Playwright 的 CI run `35418654378` 十个作业全绿），
  Completion Audit 见 `docs/reports/PLAN-0013-COMPLETION-AUDIT.md`；计划已归档至
  `docs/plans/archive/2026/PLAN-0013-identity-and-authorization-foundation.md`。

Enterprise AI Workspace 现状：

- 架构 Baseline、ADR-0018、Ever Gauzy/Cloudflare OS 参考和 PLAN-0006 Revision 1 已形成；
- `crates/agent-integration` 仍只有 TODO 骨架；PLAN-0006 Revision 1 在 Identity/Authorization + Contract slice 前 BLOCKED；
- apps/agent-adapter 在 PLAN-0007 中提供窄的 HTTP MCP read-only adapter；PLAN-0006 的 Workspace/Agent Runtime 仍未实现；
- Workspace/Conversation/Thread/Turn 尚未实现；
- Skill/Context/Tool Registry 尚未实现；
- Delegated Agent Capability Grant 尚未实现；
- ToolInvocation/Observation/Artifact 尚未实现；
- Assistant UI/SSE/业务卡片尚未实现；
- Model Gateway 和 Generated App Sandbox 尚未实现。

仍需收敛：

- 当前领域 crate 尚未按 Bounded Context Map 完成全部统一语言和数据所有权落实；
- `workflow` 通用能力仍未实现通用 Durable Task Execution；PLAN-0004 的固定
  Document Processing 执行边界已落在独立 processing adapters 中；
- Worker 与 Migration 已具备该固定切片的运行实现，其他 Agent Adapter 能力
  仍处于骨架阶段；Runtime Governance 的统一 Audit、Integrity 与 Repair
  能力已由 PLAN-0005 收敛并集成；平台原生 Analytics/Visualization 架构 Baseline 已建立，运行时实现尚未开始；
- API/Event 契约尚未全部形成可生成 Schema；
- 质量属性尚未形成完整性能和容量证据（staging 20 并发 smoke 已实测并记录，
  生产形态容量证据尚无）；
- Runbook v0.1 已交付（`docs/operations/RUNBOOK.md`，PLAN-0012 M4）；生产数据
  演练与生产 observability 在线验证仍待真实生产访问；
- Enterprise AI Workspace 仅有文档设计，没有运行证据。

## 3.1 当前后续路线

当前不再按“先补齐所有平台抽象”推进，而以真实业务垂直切片驱动：

~~~text
P0  PLAN-0013 Identity / Authorization
      PlatformUser / TenantMembership / Organization
      Role / Permission / RoleBinding / ResourceScope / Policy
       ↓
P1  Contract Business Vertical Slice
      List/Detail → Document Revision → AI Extract/Evidence
      → Review → Apply Candidate → Contract Version Update
       ↓
P2  Knowledge/Evidence Projection（真实问答需求需要时）
       ↓
P3  PLAN-0006 Revision 1 Workspace / Agent
      Workspace/Turn/AgentRun
      Compiled Agent Tool Catalog + per-turn resolution
      task-scoped Capability + Observation + durable SSE
       ↓
P4  Controlled Write ActionPlan
       ↓
P5  Analytics / Approval / Finance / Legal 跨上下文业务
~~~

近期明确不优先：generic Workflow Designer、dynamic native/WASM/Node/Python plugin
runtime、Marketplace、Generated App Sandbox、自建 OAuth/OIDC server、自建通用 RAG
engine。只有真实业务需求证明抽象必要时再独立立项。

## 4. 已完成计划的持续约束

PLAN-0001 至 PLAN-0005 的约束继续有效：

1. 业务上下文拥有自己的正式状态和规则；
2. Domain/Application 不依赖具体 Delivery/Infrastructure；
3. Application 用例定义权限、租户、版本、幂等、事务和审计意图；
4. Infrastructure 类型、错误和 DTO 不向核心泄漏；
5. API 按契约规范实现认证、错误、版本、幂等和乐观锁；
6. 数据写入、Audit 和 Outbox 的一致性符合数据架构；
7. Local/S3 Adapter 符合安全、流式和契约测试要求；
8. Processing 和 Governance 使用明确 Owner Port，不直接写其他上下文私有表；
9. 多 Worker 写入使用 Lease/Fence/Optimistic Version；
10. Repair 只允许明确类型和 Allow-list，不提供任意 SQL；
11. CI 必须继续提供真实 PostgreSQL/MinIO 和架构证据。

PLAN-0006 还必须保持：

12. Agent Adapter 不直接访问业务数据库；
13. Skill 不复制业务状态机；
14. Capability Grant 不超过原用户权限且绑定任务、资源、动作、字段和期限；
15. Tool 无通用 SQL、Shell、文件系统和任意 HTTP；
16. Observation 不保存无界敏感正文；
17. Workspace/Artifact 不成为第二业务内核；
18. Agent Runtime 可替换；
19. Generated App 明确排除在 PLAN-0006 外。

## 5. 后续任务强制规则

所有计划必须包含：

- 目标 Bounded Context/平台能力边界；
- 数据所有者；
- 业务不变量；
- Commands、Queries、API 和 Events；
- 事务、一致性、幂等和补偿；
- 安全与数据分类；
- 质量属性；
- 部署和可观测性影响；
- Fitness Functions；
- 文档和 ADR 更新。

Agent/Workspace 计划还必须包含 Delegation、Capability、Tool allow-list、
Observation、Artifact lineage、Runtime replaceability 和 Prompt Injection 威胁模型。

缺少这些内容的计划不能直接进入实现。

## 6. 架构适配门禁

当前最低门禁：

- Domain 不依赖 Delivery、Infrastructure 和供应商实现；
- Application 不依赖具体 Adapter；
- Handler 和 Worker 入口不承载业务规则；
- 基础设施错误和 DTO 不向核心泄漏；
- 业务用例能以 Fake/In-Memory Ports 运行；
- 适配器有真实依赖契约测试；
- 跨上下文没有直接写入对方私有数据；
- 新增长时任务区分业务状态和执行状态；
- API/Event 兼容性有测试；
- 安全和租户边界 fail-closed；
- 架构相关文档与代码同步；
- Agent Adapter 不依赖业务持久化适配器；
- Agent Integration core 不依赖 Axum/SQLx/Reqwest/Provider SDK；
- Capability 使用和 Tool 参数范围可自动验证；
- Generated App 运行时依赖不会提前进入 PLAN-0006。

PLAN-0010 还必须保持：

20. Module Manifest 不拥有业务事实，Semantic Contract 不成为第二语义权威；
21. 编译器拒绝重复/冲突/未知引用/循环/私有跨模块引用并生成可重建摘要；
22. WrenAI、Python、任意 SQL、Schema、凭证、C-specific Platform Core 依赖均保持隔离。

## 7. 当前判定

```text
完整服务端架构 Baseline：已形成并生效
业务能力和 Context Map：已形成初始 Baseline
数据所有权和一致性：已形成 Baseline
长时任务架构：已形成 Baseline，固定 Document Processing 已实现
Runtime Audit / Integrity / Repair：已形成 Baseline 且实现基础已集成
Enterprise AI Workspace：Baseline/ADR/Proposed Plan 已形成，代码未开始
API/Event 契约：已形成 Baseline，Schema 尚待全面落地
安全架构：已形成 Baseline；production OIDC authn 已实现；PlatformUser/TenantMembership/Role/Permission/ResourceScope/Policy 已 Integrated（PLAN-0013，PR #17）；Agent Capability 待 PLAN-0006 Revision 1
质量属性：已形成初始目标，Workspace 性能/恢复证据尚无
部署和可观测性：已形成 Baseline，Workspace/Sandbox 部署未实现
遗留迁移：已形成 Baseline，具体切片尚待计划
代码骨架：已存在
分层依赖：部分符合
基础设施隔离：部分符合
自动化架构门禁：已实现基础，本地与 GitHub Actions 均 PASS；PLAN-0010 module/semantic gates PASS；PLAN-0011 business application/compiler/dry-plan/fitness gates PASS；cargo-audit/gitleaks/trivy 已入 CI security job 并 PASS（run 33617370509；本机 NOT RUN）；cargo-deny/syft/grype/osv-scanner 未接入；Agent 门禁待 PLAN-0006
PLAN-0001：Integrated / Archived
PLAN-0002：Integrated / Archived
PLAN-0003：Integrated / Archived
PLAN-0004：Integrated / Archived（main `12454709a88fde16f7769af27a75e79c4bc0981a`；Feature CI `30833916455`；Main CI `30868701290` 全绿；Windows PostgreSQL/MinIO NOT RUN）
PLAN-0005：Integrated / Archived（main `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac`；Implementation `24e70f4182ca3315d94033178952113c4faba717`；Candidate `9056db7a1ff780ecbaaa7afb81e070e7f77c45ac`；Main CI `31026047403`；Windows PostgreSQL/MinIO NOT RUN）
PLAN-0008：Integrated / Archived（Base `35d1d01fd49a70ee996fbb5fb72818a632989efe`；Implementation/runtime `70469be26cb009c23f1a77c1553947522ba82aed`；Final Candidate/Integration `7eb5421e492a11c0ac20b17f8fd5c3a034f7a29b`；Feature CI `31353149398`；Main CI `31353409550`；本机 PostgreSQL/MinIO NOT RUN）
Analytics/Visualization：Baseline（ADR-0017）；运行时实现尚未开始
PLAN-0006：Revision 1 / Proposed / BLOCKED（ADR-0018/0021/0022/0024；planning base `b9eadf8f1b2b46c88f40f2defc88ffcb8bdb0b34`；activation requires Contract vertical slice Integrated；PLAN-0013 已 Integrated）
PLAN-0013：Integrated / Archived（Identity and Authorization Foundation；PlatformUser/TenantMembership/Organization/Role/Permission/RoleBinding/ResourceScope/Policy；分支 `feat/PLAN-0013-identity-and-authorization-foundation`，代码 head `1990a33`；独立审阅修订 C1+M1-M5 完成；Feature CI `35418654378` 十作业全绿；merge `d3a9196`（PR #17），Main CI `35426688873` 全绿；完成审计 `docs/reports/PLAN-0013-COMPLETION-AUDIT.md`）

PLAN-0007：Integrated / Archived（Business Console、Public REST Contract、CLI、read-only MCP；implementation `ec6cff141a89dcdf5de2f2ea2b8b001384f88755`；completion audit `docs/reports/PLAN-0007-COMPLETION-AUDIT.md`，由 PLAN-0012 M0 完成；全部门禁 PASS 或带原因 NOT RUN，Windows PostgreSQL/MinIO 与本地 Playwright NOT RUN）
PLAN-0009：Completed / Rehearsal Closed / Archived（C Legacy Contract & Document Migration Rehearsal；原始 Base `654fe83d82107d899079d20e5fef8aaf4d5431b8`；原始完成 HEAD `f09d2a5012627ab2219f309a2d9c1c4eacfe11f4`；readiness `REHEARSAL_PASS_WITH_MANUAL_REVIEW_REQUIRED`；production migration `NOT GRANTED`）
PLAN-0010：Integrated / Archived（Business Module Isolation + Semantic Contract；candidate `7997a501528bf12ae7846a9dc278fe4fce65a467`；已集成基线 `ad35c3c172cf19c97366c38ae8340852f3b6365c`）
PLAN-0011：Integrated / Archived（Business Application Packaging and Contribution；Candidate/Main `ed870acfe165756632c0519bb181fd5dcf8a11cd`；Feature CI `32210387950`；Main CI `32213985080`；ADR-0021/0022 Accepted）
PLAN-0012：Integrated / Archived（v0.1 预生产发布：外部 OIDC IdP 生产认证（ADR-0024）、vendored model-provider（ADR-0023）、可观测性与备份恢复演练；Slice C 真实 staging 栈 + 真实 Auth0 IdP 验收 E2E 12/12、fail-closed 负例 8/8；merge `2383651`（PR #10），Main CI `33705531597`；`v0.1` tag → `2383651`；T3.3 Keycloak demo compose 后置；完成审计 `docs/reports/PLAN-0012-COMPLETION-AUDIT.md`）
v0.1 release：已发布（annotated tag `v0.1` → `2383651`；生产数据演练与 observability 在线验证仍需真实生产访问，见完成审计 NOT RUN 表）
Business Application Platform：Baseline（由 PLAN-0011 建立 packaging/contribution/compiler/dry-plan foundation；runtime/具体业务模块未实现）
```

## 8. 下一实施顺序与门禁

### PLAN-0013

Integrated / Archived：独立审阅修订（C1 + M1-M5）完成后经 PR #17 合入 main；计划归档于
`docs/plans/archive/2026/PLAN-0013-identity-and-authorization-foundation.md`，完成审计见
`docs/reports/PLAN-0013-COMPLETION-AUDIT.md`。

### Contract Business Vertical Slice

创建独立计划（前置 PLAN-0013 已 Integrated）。该切片必须成为 Identity/Authorization、Document
Revision/Evidence、Business Application Contract 和 Business Console 的真实消费者，
而不是继续新增 generic platform abstractions。

### PLAN-0006 Revision 1

仅在 Contract slice Integrated 后激活（PLAN-0013 已 Integrated）。首批 Agent Tool 只读，并且
只能调用已发布的 Application Query；Capability 只能缩小当前用户权限。

## 9. 下一次更新条件

出现以下事件时更新本文：

- PLAN-0013 状态变化、Accepted Candidate 或集成；
- 首个 Contract Business Vertical Slice 建立/集成；
- PLAN-0006 Revision 1 activation gate 满足或调整；
- Identity/Authorization、Bounded Context 或数据所有权改变；
- 新增生产部署单元、供应链安全事件或重大基础设施变化；
- 质量属性目标被实测或调整；
- Knowledge/Analytics/Controlled Write 等后续计划被正式提出。


> 2026-09-23: ADR-0025 Accepted，完成 Enterprise AI SaaS Platform 文档对齐。business-platform
> 被明确为 SaaS Platform 主承载；SaaS Core、Business Application Platform 与 AI/Agent Platform
> 使用同一 Tenant/Principal/Policy/Audit/Usage contract；正式模块目标最低 R2，独立发布与独立部署
> 分离；外部 SaaS 基础设施只通过 Port/Contract 作为 provider。Backend Manifest、Server Architecture、
> Bounded Context Map、Business Application Platform、Code Architecture、Deployment Architecture 与
> Architecture Fitness 已同步。本轮 docs-only，不修改 Rust、migration、OpenAPI、数据库或运行拓扑。

> 2026-09-23: PLAN-0014 Wave 0/1 在分支
> `feat/PLAN-0014-wave1-iam-policy-reference-conformance` 基于同步后的
> `8365de0211390a6a3367b536c5ceaa5256bbb5cf` 完成 Identity / Organization / Policy
> 的源码、测试、migration、API 与架构基线重建，以及固定版本的外部参考对齐。审阅记录见
> `docs/reviews/2026-09-23-plan-0014-wave0-iam-policy-baseline.md` 与
> `docs/reviews/2026-09-23-saas-align-identity-organization-policy.md`。当前实现判定为
> R1 foundation；本轮未发现 C0，C1 以 R2 独立发布证据与真实 PostgreSQL/发布闭环不足为阻断，
> 过期/撤销生命周期、service identity 与通用 ReBAC 保留为 C2/C3。SQLite shared contract
> 验证已通过；完整 Business API、PostgreSQL、workspace 与独立 reviewer 状态以 formal review
> 的逐项证据为准。本轮仍为 docs-only，不修改 Rust、migration、OpenAPI、provider runtime，
> 不启动 Wave 2。
