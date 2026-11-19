# 架构实施状态

> 文档类型：Living Document
> 最后更新：2026-09-27
> 当前阶段：Enterprise AI SaaS Platform Baseline 已对齐；PLAN-0014 Reference Conformance / R2 Readiness Active；Contract 真实垂直切片仍是下一主要业务实现
> 当前计划：PLAN-0014 Reference Conformance / R2 Readiness Active（review-first，2026-09-23→2026-10-23）；Contract Business Vertical Slice 仍是下一主要业务代码计划；PLAN-0006 Revision 1 Proposed / BLOCKED
> 集成方式：GitHub PR merge
> Analytics/Visualization：Baseline 已建立，运行时尚未实现
> SaaS Platform：2026-09-23 经 ADR-0025 Accepted，`SAAS_PLATFORM_ARCHITECTURE.md` 与 `SAAS_MODULE_STANDARD.md` 已晋升 Baseline；复用现有 BusinessModuleManifest/compiler 与 PLAN-0013 IAM，不激活新 runtime
> Reference Alignment：PLAN-0014 已排期，先 IAM/Policy 与 Module Packaging，再 Durable/Audit、Document/Storage、Observability/Notification，最后至少两个模块完成 R2 pilot
> Execution/Sandbox Infrastructure：2026-09-24 新增 ADR-0026 + EXECUTION_SANDBOX_SERVICE_ARCHITECTURE.md + PLAN-0015（均 Proposed / NOT ACTIVE）；先确定共享基础设施边界，Business Platform 保留 Durable Job/业务权威，远程执行服务独立部署且不与 Jarvis 共享状态；当前不改变 PLAN-0014 与 Contract 垂直切片优先级。

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

> 2026-09-24 closeout: PLAN-0014 Wave 1 独立只读审阅对 Base
> `8365de0211390a6a3367b536c5ceaa5256bbb5cf` → Candidate
> `c7dc3d142a526b47e9ba8e472b82346ffdaed221` 给出 `PASS WITH C2/C3`，未发现证据支持的
> C0/C1 correctness 或 tenant-isolation blocker。审阅确认的 C2 已记录为 G-09：组织成员
> roster 移除不能单独证明另一条 Policy RoleBinding 已撤销。Identity / Organization /
> Policy 继续为 R1 foundation；R2 因独立发布契约及当前 PostgreSQL 证据不足保持 BLOCKED。
> 详见 formal review；本次只更新审阅状态文档。

> 2026-09-24: PLAN-0014 Wave 2 Business Module Packaging reference conformance 在分支
> `feat/PLAN-0014-wave2-business-module-packaging-conformance`（Base
> `5d3d2eb5cfe93aad6f8c5fe4d023d386d0a08b04`，未 push、未合并）完成：固定 ABP 10.6.1、
> Frappe v16.35.0、ERPNext v16.36.0、Odoo 19.0、Twenty v2.9.0 的 commit 级参考并按源码
> 取证（Round 2 校准：`TwentyCoreModule`/`serverVersionRange`/`ApplicationActivation`
> 从未出现在 ADR-0021 原文，NOT FOUND 仅为本记录自身取证的标识校准；ADR-0021 的
> "required server version range" 兼容窗口结论经 v2.9.0 证据证实仍成立，Accepted ADR
> 未修改）。canonical
> module release identity（`module-release.identity.v1` 八字段 + `ModuleReleaseBundle`）
> 与 compiler canonical `contract_digest` seam 落地为纯值契约并有测试与 fitness 守卫；
> 复用 `BusinessModuleManifest` 为唯一声明，未建第二 manifest、未新增运行时或部署单元。
> R2 packaging acceptance 12 条均有可执行入口（本轮可运行 9 条 PASS；PostgreSQL lane、
> release 工件构建、workspace 外独立消费者构建记 NOT RUN，归 Wave 5 fixture）。Wave 2
> 三个 exit 判定 MET。gap registry：C0=0；C1×3（G-201 契约层已关闭；G-202 独立发布工件
> 与外部消费者证据缺失 = R2 BLOCKER；G-203 迁移命名空间未落物理隔离）；C2×3（含继承
> Wave 1 的 G-09 原样保留）。`business-module-contracts`/`business-application-compiler`
> 与 Identity/Organization/Policy 一律维持 R1 / R2 BLOCKED / NOT CLAIMED。审阅记录
> `docs/reviews/2026-09-24-saas-align-module-packaging.md`。
> Round 2 修复（同分支，2026-09-24）：Round 1 候选独立审阅发现 A-09 实为 shape 校验
> 非一致性验证、`contract_digest` 遗漏已登记 public target 的 agent capability、
> `package_digest` 被误述为工件内容摘要、focused 计数 "79" 誊写错误。修复：compiler
> 边界新增 `verify_release_identity`（重算双 digest + 单字段错配负向）并明确
> `ModuleReleaseIdentity::validate` 仅字段形状校验；agent 公开契约面纳入摘要；release 层
> `module_version`/bundle `crate_version` 强制 SemVer；`package_digest` 语义修正为
> compiled declaration fingerprint（声明摘要 ≠ 发布工件字节，工件完整性 digest 维持
> Wave 5 NOT RUN）。focused 更正为 113；Wave 2 exit 第一项经 NOT MET 降级后以新证据
> 恢复 MET（review §12/§16）。G-202/G-203 关闭条件不变；两个 crate 仍维持
> R1 / R2 BLOCKED / NOT CLAIMED。当前状态 READY FOR INDEPENDENT RE-REVIEW /
> NOT INTEGRATED，独立 reviewer 复审 verdict 待回填。

> 2026-09-27 closeout（docs-only）: PLAN-0014 Wave 2 外部复审已记录（review §14）——
> Codex 独立只读复审 Reviewed HEAD `93b18ab67c0426e6e5efc4f065260d7068995f38`（审阅修复
> 范围 `50b6ae95a3e6f8db8f51586cf1e5bce5d0662f7f` → Reviewed HEAD）：代码修复通过；文档
> 兼容性表述不确——review §5 规则 3 原"contract_digest 不变 ⇒ 兼容面不变"高估摘要语义，
> 已改为"contract_digest 相同仅表示纳入摘要的公开声明面相同；compatibility、dependencies、
> module_version 等仍须通过 compile/dry_plan 的独立兼容性校验，不能据摘要相同推断可互换"。
> 复审独立执行 focused 113 passed / 12 suites、Architecture Fitness PASS、
> `git diff --check` PASS，closeout 在 Reviewed HEAD 复测同值；全 workspace 四项门禁与
> 外部参考取证未独立重跑。验收分项校准为 12 条 = 10 PASS（A-01..A-09、A-11；A-09 验证
> 范围为声明一致性，非真实工件/外部消费者）+ 1 PARTIAL（A-10 workspace 内部分）+
> 1 NOT RUN（A-12）。Round 1/2 历史未改写。Wave 2 当前状态 `Wave 2 implementation
> reviewed / documentation corrected / NOT INTEGRATED`；外部复审不构成 R2 admission PASS，
> Step F formal verdict 未回填；R1 / R2 BLOCKED / NOT CLAIMED 维持；G-201 真实工件组装
> 残项、G-202、G-203、G-09 保留。

> 2026-09-27: PLAN-0014 Wave 3 Durable Task Execution / Audit / Messaging
> reference conformance 在分支
> `feat/PLAN-0014-wave3-durable-audit-messaging-conformance`（Base `443a11c`＝Wave 2
> closeout，未 push、未合并）完成：固定 Temporal v1.32.0、Restate v1.7.12、
> Trigger.dev 3.3.17+v4.6.4、jarvis-rs vendored、ZITADEL v4.18.0、immudb v1.11.2、
> Infisical 0.165.16、Debezium v3.6.3.Final、NATS v2.15.0、Kafka 4.3.1 的 tag→SHA
> 参考并源码取证（BSL/AGPL/EE 仅读语义；AgentOS NOT FOUND、Quarkus outbox seq 列
> NOT FOUND 均如实记录）。修复（failing-first）：D-G01 取消事实事件/审计在全部转换
> 路径恰好一次（意图 `processing_cancel_requested` 与事实 `cancelled.v1`+`processing_cancelled`
> 分离，SQLite 5 测试绿 + PG 镜像 ignored 接入 CI）；A-G01 document-sqlite 审计链写入
> 静默断裂（17 列 legacy INSERT 改为 20 列 v1 链，3 测试绿 + PG `chain_pg` 镜像）；
> D-G02 适配层 stale-fence/reclaim 负向直测补齐（4 测试）。注册未修：D-G03 pipeline
> version 列、D-G04 Attempt/provider 关联、D-G05 legacy claim-port seam、A-G03 无外部
> 反截断 checkpoint、A-G08 黑名单式 redaction 等 C2；D-G06/A-G07 事件 envelope
> correlation/causation 为 C1-by-standard，因属公开契约变更走独立设计提案（显式
> blocker）。门禁本地全绿（fmt/check/clippy/test/architecture/diff-check exit 0，
> 735 passed / 50 ignored，原始日志存 `docs/reviews/2026-09-27-wave3-gates/`）；全部
> PostgreSQL lane 本地 BLOCKED（原因与专用库复跑命令见 gates/07），8 个新增 PG 测试
> 从未在本会话执行，不得记为通过。tamper-evidence 措辞经六个参考校准无 overclaim，
> 尾部截断不可检测以测试断言固化。当前状态 `Wave 3 implementation reviewed /
> self-verified / READY FOR INDEPENDENT REVIEW / NOT INTEGRATED`；R1 foundation，
> 非 R2 admission，独立 reviewer verdict 待回填；未启动 Wave 4/5，未激活
> PLAN-0015/PLAN-0006。审阅记录
> `docs/reviews/2026-09-27-saas-align-durable-execution.md`、
> `docs/reviews/2026-09-27-saas-align-audit-messaging.md`。

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
- 状态：**READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**。实现者不自填独立 PASS；两份 Wave 3 评审的独立记录仍为 `<pending>`。
  R1 声明仍需独立复审；**R2 BLOCKED**；Wave 4/5 未启动；ADR-0027/ADR-0028 仍 Proposed / NOT ACCEPTED。
  修复前写入 PostgreSQL 的不可验证行仍属需另行评审的运维事项，本轮不枚举、不重签名。

## 2026-09-28 PLAN-0014 Wave 4 — Document / Object Storage / Observability / Notification reference conformance

> Wave 4 在分支 `codex/plan-0014-wave4-reference-conformance`（Base `a4c2f74b`，未 push、未合并，`main`
> 未改动）完成，代码候选 `6fb9487d045108c70efd41c88ccc95f0861ba603`。上一轮结论原文照录且未扩大：
> 「Round 2c 修复通过；Wave 3 保持 R1 / NOT INTEGRATED；R2 阻塞项继续有效」。
> 评审：`docs/reviews/2026-09-28-wave4-baseline.md`（Step A 能力基线 + Step B 固定参考）、
> `docs/reviews/2026-09-28-saas-align-document-storage.md`、
> `docs/reviews/2026-09-28-saas-align-observability.md`、
> `docs/reviews/2026-09-28-saas-align-notification.md`；
> 证据：`docs/reviews/2026-09-28-wave4-gates/`、`docs/reviews/2026-09-28-wave4-storage-lane/`。

- 固定参考到 tag→SHA（Paperless-ngx v3.2.1、Documenso v2.18.0、Open Contracts v3.1.1、OTel spec
  v1.61.0、Collector v0.161.0、RustFS 1.0.1-preview.11）；Novu 与 Mayan EDMS 在本环境无法固定，
  按 BLOCKED 记录且不引用其行为结论。
- 修复（failing-first）：幂等重放租户围栏 D-W4-01（SQLite+PG）；对象键/桶名进入日志与驱动错误文本
  ST-W4-02；本地适配器伪 presign ST-W4-03；presign 过期无界 ST-W4-04；governance-worker 无 subscriber
  OBS-W4-01；business-api 自建指标与客户端摘要 OBS-W4-03；provider 噪声 OBS-W4-02；`count()` 零覆盖
  D-W4-05；notification 文档虚报。
- 注册未修 C1：D-W4-02 复合外键、D-W4-06 文档资源无授权路径、D-W4-08/ST-W4-05 无元数据↔对象一致性扫描、
  D-W4-09 PostgreSQL 缺 evidence↔revision/artifact 绑定触发器、D-W4-11 `document_links` 无 revision 绑定、
  OBS-W4-05 无服务端关联身份（依赖 ADR-0028）、OBS-W4-06 exporter BLOCKED、OBS-W4-07 日志无脱敏、
  N-G05 outbox relay 未接线。多数属 schema/公开契约变更 → 需设计提案，本轮未实现。
- 真实通道：PostgreSQL 18.4 专用库 `plan0014_wave4`（3 target 全绿，结束后仅 DROP `plan0014_wave4%`）；
  真实 S3-compatible 改用 RustFS（MinIO 开源二进制 `dl.min.io` → HTTP 410 已停发），专用桶+前缀，
  lane 7/7、前缀 0 残留、桶已删、进程已停；`minio_contract.rs` 仍 BLOCKED/NOT RUN。
- 门禁全绿：fmt / check / clippy `-D warnings` / workspace 测试 **759 passed / 0 failed / 59 ignored** /
  architecture fitness PASS / `git diff --check`。59 ignored 不计为已执行。
- 状态：**READY FOR INDEPENDENT REVIEW / NOT INTEGRATED**；R1 foundation，独立 verdict 待回填；
  C1 未清零 ⇒ **R2 BLOCKED**；Wave 5 / PLAN-0006 未启动；ADR-0027 / ADR-0028 仍 Proposed / NOT ACCEPTED。

> 2026-09-29 Wave 5A（本地发布候选，未发布）在分支 `codex/plan-0014-wave5a-local-release-pilot`
> （Base `9b05ccddb4964b363b0c9ae001c4303c1b7393db`，未 push、未 merge、未打正式发布 tag、未部署）
> 完成。上一轮结论原文照录且未扩大：「Round 3 修复通过；Wave 4 有界收口；既有 C1、R2 阻塞与
> 未执行项继续有效」。

- 试点：`identity` 与 `policy` **两个模块**（PLAN-0014 §6 候选前两项）。`business-module-contracts`
  与 `business-application-compiler` 同属一个 foundation，若日后登记只是**一个**模块
  （`ModuleReleaseBundle.crate_versions` 映射两个 crate），本轮不用 bundle 凑模块数。
- 新增 workspace 成员 `tools/module-release-tool`（`generate` / `verify` / `compare`）：复用
  `contract_digest` / `package_fingerprint` / `verify_release_identity` seam，未修改两个纯 crate；
  工具侧新增的物理工件摘要 `artifact_integrity_digest` **不并入** `ModuleReleaseIdentity`。
- 工件：`identity-0.1.0-rc.1` / `policy-0.1.0-rc.1`，含 module id + 独立版本、manifest、公开契约
  工件（`contract/contract-surface.json` 逐字节等于编译器哈希前镜像）、迁移映射、changelog、
  L2 identity/bundle、L3 验证索引；四类摘要分列见 `docs/reviews/2026-09-29-wave5a-implementation-record.md`
  §4.3 与 §13.2；`source_revision` 钉在代码候选提交 `8036bc65ba7c77666e48afaa6439911ba7c083d7`。
- 确定性：两目录生成 + `compare` 零差异；宿主绝对路径 / 机器名 / 用户名 / 时间戳 / `target/`
  不出现在 payload；release identity 不含随机 UUID、当前时间或机器名。
- 真实构建：packed crate 在 payload 之外以自身为 workspace root 构建成功且构建后工件复验仍 PASS；
  两个 workspace 外消费者（无 path 依赖回本仓库、无 DB adapter、无 `apps/*`）实际编译并调用公开契约
  （identity 1 例、policy 2 例，均默认拒绝方向），缓存预热后 `--locked --offline` 复建通过。
- 门禁全绿：fmt / check / clippy `-D warnings` / workspace 测试 **833 passed / 0 failed / 70 ignored**
  （187 suites）/ architecture fitness PASS（新增 Wave 5A 边界，做过注入式反向验证）/
  `git diff --check`；定向 `module-release-tool` 30 passed。70 ignored 未执行、不计为已执行
  （当时写的是「全部 PostgreSQL lane」，同日 Repair 1 已按测试清单更正分类，见下方 Repair 1 记录）。
- 未执行 / BLOCKED：PostgreSQL 迁移执行与单模块独立安装/升级（无数据库授权；identity 与 policy 与
  organization 共享 `019_identity_authorization_foundation.sql`、同 `public` schema、被
  `runtime-migration` 全量 embed）；publish / 注册 / install / upgrade / rollback；push / merge /
  tag / 部署；漏洞、许可证、镜像扫描；CI。SQLite 与离线证据不替代 PostgreSQL 结论。
- gap registry 不变：G-01 / G-02（Wave 1）、G-201 / G-202 / G-203（Wave 2）与 Wave 4 的
  D-W4-02 / 06 / 08 / 09 / 11、OBS-W4-05 / 06 / 07、N-G05 继续有效。本轮证据只覆盖
  「可打包、可校验、可脱离 workspace 被真实调用」，**不**关闭 G-202（缺授权数据库上的独立安装/升级
  与运维回滚证据），亦不改写 `2026-09-24-saas-align-module-packaging.md` 的历史结论（该文档只追加带日期指针）。
- 状态：**LOCAL RELEASE CANDIDATES READY FOR REVIEW / NOT PUBLISHED**；`identity` / `policy` /
  两个纯 crate 维持 **R1 / R2 BLOCKED / NOT CLAIMED**；打包成功**不等于** R2；Wave 5 剩余项与
  PLAN-0006 未启动；ADR-0027 / ADR-0028 仍 Proposed / NOT ACCEPTED。
  评审与证据：`docs/reviews/2026-09-29-wave5a-implementation-record.md`、
  `docs/reviews/2026-09-29-wave5a-gates/`（独立审阅栏留空）。

> 2026-09-29 Wave 5A Repair 1（修复验证链，未发布）在同一分支完成，代码候选
> `69d9a6b01184db7d9f3d4aa3b688fb569387b2fc`。上一轮结论原文照录且未扩大：「Round 3 修复通过；
> Wave 4 有界收口；既有 C1、R2 阻塞与未执行项继续有效」；Wave 5A 独立审阅结论为
> **NEEDS REPAIR / NOT PUBLISHED**，本轮是其 §4 指令的实现。

- 修的是**验证链**，不是工件：四类摘要与成员数与上一轮逐字相同（`identity` `3fed976b…` /
  `b9dfa42c…` / `68deba81…` / `ef43cfa9…`，21 成员；`policy` `52e788ff…` / `37e95a19…` /
  `f8add9e0…` / `1a5a43f8…`，23 成员），唯一随提交变化的字段是 `source_revision`。
- **来源绑定成为真检查**：`source_revision` 现在必须是真提交对象，且工件的每个输入
  （changelog、各 crate 源文件、workspace manifest / lock、`rust-toolchain.toml`、spec、映射
  点名的迁移）都要能在该提交里以相同字节取到；40 个 0 的 revision 与不含所声称输入的更早提交
  都被拒。工具内只有 `src/git.rs` 可以起子进程，且只跑 `git`（参数数组，无 shell）。
- **隔离由完整解析图判定**：`module-release guard` 读 `cargo metadata --format-version 1` 的
  全部包，要求每个本地包规范化后落在消费者目录或声明的工件目录内；正/反斜杠、`..`、verbatim
  前缀、传递性 path 泄漏与不可解析路径都有回归。Wave 5A 编排脚本不再含自有路径规则——
  架构门禁禁止。
- 验证索引由 5 项增至 10 项（新增记录格式、记录一致性、packed 绑定、来源两项、声明检查改名），
  `compare` 现在两侧各自重算 payload 后才比较。工具链一项在明细里自述**只验声明**，构建来源
  由运行日志外部佐证，不互相冒充。
- 更正上一条记录的两处表述：70 个 `#[ignore]` 不是「全部 PostgreSQL lane」（实为 PostgreSQL
  38 + PostgreSQL 且对象存储 3 + 仅对象存储 28 + 内网模型 provider 1，均未执行）；旧「错误
  module_id 被具名检查拒绝」不成立，当时拒在 `[bundle-identity]` 且是上一例残留损坏。
- 门禁（本候选）：fmt / check / clippy `-D warnings` / workspace 测试 **855 passed / 0 failed /
  70 ignored**（190 suites）/ 架构 fitness PASS / `git diff --check` 0；定向
  `module-release-tool` **52 passed**、两个纯 crate **113 passed**（源码零改动）；新增
  `scripts/test-plan0014-wave5a.ps1` 脚本回归入口 9 条 PASS。
- 证据：`docs/reviews/2026-09-29-wave5a-repair1-gates/`（`F1-F5-closure.md` 逐项闭环表、
  `red-run/` 修复前反例复现、`negatives/` 逐例注入说明与原始日志、`gates/` 逐条门禁）。
  旧 `2026-09-29-wave5a-gates/` 原样保留，被替代的结论在其 README §6 逐项列出。
- 状态：**WAVE 5A REPAIR 1 READY FOR INDEPENDENT REVIEW / NOT PUBLISHED**；`identity` /
  `policy` / 两个纯 crate 维持 **R1 / R2 BLOCKED / NOT CLAIMED**；未 push / merge / tag / 部署，
  未启动 Wave 5B 或 PLAN-0006；G-01 / G-02 / G-201 / G-202 / G-203 与 Wave 4 观察项继续有效；
  ADR-0027 / ADR-0028 仍 Proposed / NOT ACCEPTED；独立审阅栏留空。

> 2026-09-30 Wave 5A Repair 2（候选的来源绑定补实，未发布）在同一分支完成，代码候选
> `c22939f0a51e3c7fa2ec424f3819c6e7113a9b66`。承前结论原文照录且未扩大：「Round 3 修复通过；Wave 4
> 有界收口；既有 C1、R2 阻塞与未执行项继续有效」；Repair 1 的独立审阅结论为 **NEEDS REPAIR**
> （F1-A / F1-B），本轮是其 §5 指令的实现。

- 上一轮把 `source_revision` 变成「真提交」，但**候选的字节仍来自磁盘工作树**：packed `Cargo.toml`
  从未按该提交重算（改写依赖版本 pin 并重算完整性摘要后 `verify` 仍通过），成员集合来自磁盘目录扫描
  （checkout 少一个已提交源文件时仍能生成 20 成员工件，解包后 `cargo check` 报 E0583）。
- 本轮把候选的**唯一来源**改成固定提交：成员列举用 `git ls-tree -r -z --full-name`，逐字成员用
  `git cat-file blob`，派生成员（packed `Cargo.toml`、`module.manifest.json`、契约面、迁移映射）由
  该提交重算后**整份**比较；`generate` 在创建输出目录之前先要求 checkout 与提交一致，符号链接
  （`120000`）与 submodule gitlink（`160000`）一律拒绝。工具内唯一子进程出口仍是 `src/git.rs`，
  架构门禁新增「成员列举必须是 `ls-tree -r -z`、禁止 `git show`、`src/commit.rs` 含 120000/160000」。
- 验证侧把一句话拆成三类可核对事实（成员集合 ↔ Git 树 / 逐字成员 ↔ Git blob / 派生成员 ↔ 重算），
  各自独立检查 ID：`source-member-set-matches-revision`、`source-inputs-match-revision`、
  `derived-members-match-revision`；物理完整性摘要保持独立标识，二者不得互相顶替（共 12 项检查，
  顺序固定并由测试断言）。
- 工件与四类摘要**未变**（`identity` `3fed976b…` / `b9dfa42c…` / `68deba81…` / `ef43cfa9…`，21 成员；
  `policy` `52e788ff…` / `37e95a19…` / `f8add9e0…` / `1a5a43f8…`，23 成员）；`digest_form` 的匹配顺序
  改为先试规范 LF 折叠摘要，使其不再取决于检查者如何 checkout 文件；协议本身
  （`plan-0014.artifact-integrity.v1`、`plan-0014.migration-set.v1`、`normalize_text_bytes`）一字未改。
- 门禁（本候选）：fmt / check / clippy `-D warnings` / 架构 fitness PASS / `git diff --check` 0；
  定向 `module-release-tool` **61 passed**；脚本回归 **14 条 PASS**（新增自实现摘要口径对齐 `R7a`、
  重算后仍被拒 `R7b`、隔离 checkout 三例 `R8a-c`）；pilot **exit 0**、19 负例全部按被断言的检查被拒。
  `cargo test --workspace --all-features` 在默认构建目录因两个 bin 被会话外进程占用而**环境阻塞**，
  同一代码在独立 `--target-dir` 下重跑，两份日志均归档。
- 未执行 / NOT RUN（逐项，见证据 §7）：仓库外消费者 lane（本轮按审阅指令跳过）、PostgreSQL 迁移执行
  与单模块安装 / 升级 / 回滚、70 个 `#[ignore]`（PostgreSQL 38 / PostgreSQL 且对象存储 3 / 仅对象存储
  28 / 内网模型 provider 1）、publish / 注册 / install / deploy、漏洞 / 许可证 / Secret / 镜像扫描、CI。
- 证据：`docs/reviews/2026-09-30-wave5a-repair2-gates/`（`F1-closure.md` 逐项闭环表、`red/` 与
  `green/` 同命令对照、`gates/` 逐条门禁、`pilot/negatives/` 逐例注入说明）。Repair 1 归档原样保留。
- 状态：**WAVE 5A REPAIR 2 READY FOR REVIEW / NOT INTEGRATED**；`identity` / `policy` / 两个纯 crate
  维持 **R1 / R2 BLOCKED / NOT CLAIMED**；未 push / merge / tag / 部署 / 改 main，未启动 Wave 5B 或
  PLAN-0006；G-01 / G-02 / G-201 / G-202 / G-203 继续有效；ADR-0027 / ADR-0028 仍 Proposed /
  NOT ACCEPTED；独立审阅栏留空。

> 2026-09-30 Wave 5A Repair 2 独立审阅结论落账 + Wave 5B-0 迁移隔离设计（**纯文档轮**，未发布、
> 未连库）在同一分支完成，文档基线 `c73955ec30f46d839ff33d44776cef142b7d6d77`。承前结论原文照录且未扩大：
> 「Round 3 修复通过；Wave 4 有界收口；既有 C1、R2 阻塞与未执行项继续有效」。

- 独立审阅结论：**PASS（Wave 5A Repair 2 有界修复） / NOT INTEGRATED**
  （`docs/reviews/2026-09-30-wave5a-repair2-independent-review.md`，审阅范围 `7180eec7..c73955e`）。
  该 PASS 仅覆盖这次有界修复，**不改写** G-01 / G-02 / G-201 / G-202 / G-203 的原边界，
  也不是 PLAN-0014 总验收或 R2 admission；证据目录的「审阅结论」栏改为指针，NOT RUN / BLOCKED 行与
  全部数字未动。
- **准入台账建立**：`docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md` 以原编号逐行登记
  G-01（→ G-201 承接，记一条 lineage 不记两个成功模块）、G-02、G-202、G-203、A-01..A-12、
  两模块 C0/C1 与 Wave 4 开放项（D-W4-02/06/08/09/11、OBS-W4-05/06/07、N-G05）：原关闭条件逐字、
  固定 SHA、证据路径、证据年代、实际执行范围、状态、剩余动作、owner。计数沿用既有校准
  **10 PASS + 1 PARTIAL（A-10 外部消费者）+ 1 NOT RUN（A-12），本轮关闭数 0/12**；
  Wave 2 的 formal Step F verdict 仍 `<pending>`（从未填写），如实登记不外推。
- **迁移隔离设计交付**：`docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md`
  （Proposed / DESIGN READY FOR REVIEW）。核心决策：把 `migrations/001..019` 冻结为 host baseline
  （历史、`MANIFEST.sha256` 与 `_sqlx_migrations` 一字不动），模块新迁移落
  `migrations/modules/<namespace>/` + per-namespace `MANIFEST.sha256`，执行事实写一张平台 ledger
  `platform_module_migrations(namespace, revision, …)`（owner 显式为 Platform Runtime，append-only，
  checksum 存 LF 折叠 SHA-256 并运行时回读），由 `apps/migration` 单一 writer 按 namespace 拓扑序应用，
  namespace 级 `pg_advisory_lock` 在 ledger DDL 之前获取，checksum 漂移 / 孤儿 revision / revision 缺口
  三类一律 fail-closed，回滚按 forward-fix 且**不默认删除任何业务数据**。区分三类 namespace：
  目录 namespace、ledger namespace、真实 PostgreSQL schema（本轮**不做** schema 隔离）。
- **G-203 判据核对（判定权在独立审阅）**：上述最小方案满足
  `docs/reviews/2026-09-24-saas-align-module-packaging.md:316-320` 的五个字面子句，但**不**满足
  「identity 可在 PostgreSQL 上单独冷安装」的强读法——identity / organization / policy 的既有 13 张表
  由同一个 `019` 创建，`019:21-24` 还重命名了 `001` 建的 v0 demo 表，故任一段落都无法独立执行。
  据此提出拆分提案 **G-203a**（namespace catalog / manifest / ledger / runner 隔离 + 模块未来 revision
  独立安装升级，Wave 5B 可关闭）与 **G-203b**（既有共享表物理隔离与单模块冷安装，不可逆搬迁，
  需独立 ADR 与运维授权）。**未**把较小方案写成闭环；G-203 维持 OPEN，R2 维持 BLOCKED。
- **盘点暴露的 Baseline 偏差（登记不修）**：`apps/business-api/src/composition.rs:126` 与
  `apps/governance-worker/src/main.rs:50` 在启动时执行 `MIGRATOR.run`，与
  `PERSISTENCE_QUERY_AND_MULTI_DATABASE_ARCHITECTURE.md:96-98`「正式 API 进程不自动迁移」及
  `SECURITY_ARCHITECTURE.md:251-260`「禁止运行时应用拥有随意 DDL 权限」冲突；全仓 repository SQL
  均为裸表名、**零** `search_path` 设置（grep 口径 280 处命中，IAM 三适配器 73 处），故最小方案的 SQL
  影响面为零，物理 schema 隔离的影响面则全部落在这里。
- 新登记：`ADR-0029`（**Proposed**，未自填 Accepted）+ `docs/adr/README.md` 索引行；范围仅到
  catalog 布局 / 平台 ledger / 单一 writer，不授权 schema 搬迁或任何 `DROP`。
- 本轮门禁：Rust workspace 门禁 **本轮未运行，产品代码无改动**（沿用 `c73955e` 记录，不冒充本轮执行）；
  设计包 §9.2 的真实 PostgreSQL 验收矩阵（V1..V14）除需构建的 V13/V14 外全部
  **BLOCKED（无数据库授权）**，V5 / V10 / V13 / V14 记 **NOT RUN**。
- 状态：**WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；WAVE 5B-0 DESIGN READY FOR REVIEW；R2 BLOCKED**；
  `identity` / `policy` 维持 **R1 / R2 BLOCKED / NOT CLAIMED**；未 push / merge / tag / 部署 / 改 main，
  未启动 PLAN-0006；Wave 5B-1 实施需独立审阅放行 G-203a 判据并取得数据库授权后才能开始。

> 2026-09-30 Wave 5B-0 设计包 **Revision 1**（按 `docs/reviews/2026-09-30-wave5b-design-independent-review.md`
> §3 执行；**纯文档轮**，未连库、未改代码、未发布）在同一分支完成，起点
> `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`。该审阅对 Revision 0 设计包的判定是
> **NEEDS REPAIR（设计包）**（B1 共享 ledger 首次创建的锁保护、B2 搬迁时间线不可执行且混淆两类判据、
> B3 验收命令与 fixture 范围）；上一段的历史结论与数字照录未改。

- **锁模型更正（B1）**：设计包新增 §5.9。共享 ledger 表 `platform_module_migrations` 的**首次创建**改由
  **所有 namespace 共用的固定 bootstrap 锁**保护，与 namespace 锁职责分离；两把锁都是**事务级**
  `pg_advisory_xact_lock`（与仓内既有 8 处实现同形），公共锁用单 bigint 固定键、namespace 锁用
  `(classid, objid)` 二元组键，两种锁标签在 PostgreSQL 中键域不相交。统一锁序：host baseline →
  bootstrap 单元（锁内"存在性检查 + 创建 + 结构校验 + 提交后回读"，COMMIT 即释放）→ 按 `depends_on` 拓扑序
  **一次一把** namespace 锁；**禁止**持有 namespace 锁时再取公共锁，锁序倒置型死锁被结构排除。
  `CREATE TABLE IF NOT EXISTS` 被去掉（`crates/messaging/tests/inbox_idempotency.rs:18-22` 已记录该写法在
  PG 下并发不安全，且它会静默接受形状不同的既存对象）。持锁连接即事务连接本身，锁随 COMMIT / ROLLBACK /
  断开自动释放；应用级取消不等于断开（须显式 ROLLBACK 并弃用该连接）；等锁用 `lock_timeout` fail closed，
  不设 `statement_timeout`。`verify` / `status` **不执行 DDL**，表缺失只返回 `Empty`/`Pending`，
  权限 / 形状 / checksum 漂移一律 fail closed。并发验收由单条 V8 拆为 **V8a / V8b / V8c**
  （V8b 专门覆盖"不同 namespace 首次并发 bootstrap"，状态仍 NOT RUN）。
- **判据更正（B2）**：**撤销** Revision 0 "方案 B 单模块冷安装 = 是"的断言。方案 B 改变的是**既有表的位置与
  授权边界**；空库上没有既有表可搬，搬迁后冷安装仍须先跑 host baseline `1..19`。因此设计包 §6.6 把
  **既有库物理隔离**与**空库单模块冷安装**列为两个独立验收目标，**当前证据均为零**。方案 B 因原 expand 时间线
  不可执行（视图指向尚未创建的表、旧名仍被旧表占用）而**降为未完成的研究方案并从 Wave 5B 可执行步骤中移除**；
  §6.4 只保留修正后的候选事务顺序（同一事务内先移动表、后建旧名视图）与中间态对象存在性表，§6.5 记录 7 项
  OPEN（兼容视图上的 `ON CONFLICT` 写语义未证明、`search_path` 基线缺失、角色矩阵未设计、锁窗口未测量等）。
  G-203 的四个**记账子项**（目录隔离 / ledger 隔离 / 既有库升级 / 独立冷安装）全部 Proposed / OPEN，
  且**不改变 G-203 / G-202 / A-12 的原分母**。
- **验收命令与 fixture（B3）**：`apps/migration/src/config.rs:17-21` 在两个 URL 环境变量同时存在时直接
  `ConfigLoadError`（值相同也拒绝），故 Revision 0 的同时设置写法在连接前即失败；设计包 §9.2 改为两条**互斥**
  环境作用域（CLI lane 只设 `MIGRATION__DATABASE__URL`、测试 lane 只设 `DATABASE_URL`），try/finally 恢复
  变量原有的"存在/不存在及值"，日志脱敏。§9.1.2 逐 suite 盘点实际数据库范围：`migration_test.rs:11-19`
  丢弃给定库名并新建 `plan2_<uuid>` sibling 库（清理仅在成功路径），另有套件直接改动给定库不清理，
  以及多条套件在变量缺失时静默回落到互不一致的硬编码库；因此"same target"不作为范围证明。
  V14 改为**增量对账**（`864 + N` / `191 + M` / `70 + K` 逐条对到测试名），禁止机械固定 70 或 864/191。
  `--namespace` / `verify` 明确标注尚未实现，V13 不再声称能验证新 mapping。
- **S1 规格与 S4 历史兼容**：设计包 §11 收窄 S1 为"离线 catalog 验证 + 静态门禁"（显式 `schema_version` 且
  未知字段/版本拒绝、路径逃逸与符号链接拒绝、manifest ↔ SQL 双向集合、重复/缺口/未知依赖/环/checksum、
  空目录只表示 `RESERVED`、历史保护同时覆盖文件集合与**钉定 blob 内容摘要**并写明 Windows EOL 以 git blob
  字节为准；owner 白名单只是静态 fitness 且给出同 owner 新表的授权扩展机制）；S1 不建平台表、不接 CLI、
  不改启动迁移、不升 release spec。§5.7 HC-1..HC-5 要求 schema 版本**分派**而非替换常量
  （`tools/module-release-tool/src/spec.rs:215`、`src/packaging.rs:1583` 现为单一常量比较），否则 `c22939f`
  固定候选会失去 verify 能力；未知版本拒绝 + 固定候选重验回归。两项均只设计不实现。
- **同步与门禁**：`ADR-0029` 保持 **Proposed / NOT ACCEPTED**（同步锁模型、范围边界与两条新增非目标）；
  准入矩阵新增"记账子项"与"真实执行范围边界"两节；`PLAN-0014` 与 `docs/README.md` 追加状态与指针；
  新增 `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md`。Rust workspace 门禁 **本轮未运行，
  产品代码无改动**（沿用 `c22939f0` / `c73955e` 记录，不冒充本轮执行，不借用 Wave 5A 数字）；
  设计包 §9 全部数据库项 **NOT RUN / BLOCKED（无数据库授权）**。
- 状态：**WAVE 5B-0 REVISION 1 READY FOR DESIGN RE-REVIEW；WAVE 5A REPAIR 2 REVIEWED / NOT INTEGRATED；
  R2 BLOCKED**；`identity` / `policy` / 两个纯 crate 维持 **R1 / R2 BLOCKED / NOT CLAIMED**；
  G-01 / G-02 / G-201 / G-202 / G-203 与 Wave 4 的 C1 一律未关闭；未自判 ADR-0029 Accepted；未实施 S1/S2/S3；
  未连接数据库；未 push / merge / tag / 部署 / 改 main；未重开 Wave 5A Repair 2。

## 2026-09-30 Wave 5B-S1 离线 catalog 门禁（实施轮）

- 起点 `6decf1ad9ea05233d66dd3539594ff210afd99f2`，本地候选 `b25e2d4a9b455b776934495b8f3dfd2b71a13fe2`（未集成）。执行包为 `docs/reviews/2026-09-30-wave5b-revision1-independent-review-and-s1-instructions.md` §3，并落实其 §2 C1 历史摘要兼容规则。
- 新增 `tools/module-release-tool/src/catalog.rs`：`catalog-check` 四个 check，默认集合为 `module-catalog` + `pinned-baseline-history` + `baseline-embed`，`baseline-candidate-freeze` **仅在显式 `--only` 时生效**（ADR-0029 未 Accepted 前不作为默认策略）。`scripts/check-architecture.ps1` 调用默认 check，并另加 namespace manifest 的 canonical-LF-only 断言（不复用历史 manifest 的三形容错）。`scripts/test-plan0014-wave5b.ps1` 与 `tools/module-release-tool/tests/wave5b_catalog.rs` 是同一张 `EXPECTATIONS.json` 的两个读取者。
- 架构事实变化：`migrations/modules/{identity-management,policy}` 两份 catalog 存在且状态为 **RESERVED（0 revision）**——这是预留，不是"可独立安装"，更不构成"`migrations/modules` 是已安装 catalog"的声明。baseline embed 点收敛为唯一一处 `crates/runtime-migration/src/lib.rs`；两处 PostgreSQL 测试改用 `runtime_migration::MIGRATOR`（修 P-4）。历史 baseline 文件与历史 `MANIFEST.sha256` **一字未改**：C1 的 7 条 CRLF-only 摘要继续按原样被接受，但每个历史文件另被其 blob 摘要钉住。
- 静态门禁不等于隔离证明：owner 扫描只是 SQL 文本 fitness function，不证明数据库层角色/grant 隔离（设计包 §11.2 #10 的诚实边界）。G-203 的四个记账子项与 G-01 / G-02 / G-201 / G-202 / G-203、Wave 4 的 C1 **一律未关闭**，准入矩阵分母未降。
- 门禁：逐条命令与退出码见 `docs/reviews/2026-09-30-wave5b-s1-gates/`，对账见 `docs/reviews/2026-09-30-wave5b-s1-closeout.md`。PostgreSQL 相关 suite 因无数据库授权 **NOT RUN**；文件 symlink 路径用例 **BLOCKED**（创建符号链接需管理员权限，且仓库卷为 exFAT，不持有 reparse point）。
- 状态：**WAVE 5B-S1 READY FOR INDEPENDENT REVIEW / NOT INTEGRATED；R2 BLOCKED**。未自判 ADR-0029 Accepted（仍 Proposed）；未实施 S2（runner / ledger / 运行时接线）；未连接数据库；未改 `main`；未 push / merge / tag / publish / 部署；未重开 Wave 5A Repair 2，其归档证据未改。
