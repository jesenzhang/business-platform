# PLAN-0012：Runnable v0.1 — 真实认证、model-provider 集成与预生产就绪

文档 ID：PLAN-0012
版本：0.1
状态：Active / local solo fast-forward
日期：2026-08-25
责任边界：Document Intelligence（AI Provider 适配）、Business API（认证）、平台可观测性与预生产环境
前置计划：PLAN-0001～PLAN-0005、PLAN-0008～PLAN-0011 已 Integrated；PLAN-0007 Active（本计划 M0 收尾归档）

## 目标

交付第一个预生产级可运行版本 v0.1：

1. Business API 使用真实 OIDC/JWT 认证（替换 dev token）；
2. ai-worker 使用 `jarvis-model-provider`（独立 Rust 仓库
   `github.com/jesenzhang/jarvis-rs` 的 `crates/model-provider`）实现真实
   LLM 字段提取，替换 `DeterministicLocalExtractor` 的生产路径；
3. 可观测性基础落地（结构化日志、请求 ID 贯穿、`/metrics`）；
4. 持久化预生产环境、备份/恢复演练、安全扫描接入 CI；
5. 完成 v0.1 发布审计并打 tag。

## 非目标

- 不实现 Workspace、Conversation、Skill、Capability Grant 或任何 PLAN-0006 内容；
- 不新增 Customer/Contract/Finance 等核心业务域切片；
- 不引入通用 Workflow、DAG、调度器或 Model Gateway 平台化抽象；
- 不把 model-provider 提升为平台核心依赖——它只存在于 ai-worker 组合根；
- 不修改既有迁移或正式业务事实所有权。

## 架构预检

| 项 | 决定 |
|---|---|
| Bounded Context | Document Intelligence 继续 owns ProcessingJob/Step/AI Task/Candidate；认证属于 Business API 信任边界；可观测性属于平台横切能力。 |
| 数据所有者 | model-provider 无状态接入，不拥有业务事实；JWT 声明由 IdP 拥有，API 只验证映射。 |
| 状态归属 | AI Task 的 lease/fence/retry 语义不变；provider 调用是无外部效果副作用边界（失败可重试）。 |
| 公开能力 | 无新增公开 API 契约；认证中间件行为变化对客户端表现为 401 语义收紧。 |
| 一致性 | provider 调用失败映射为既有 `ClassifiedProcessingFailure` 重试/死信语义，不引入新的分布式事务。 |
| 安全 | API key 通过 `runtime-config` secret_url 承载，fail-closed，不进日志/DTO/公共输出；provider 原始错误对外部响应脱敏；provider base URL 只允许 HTTPS 或 loopback。 |
| 部署 | 新增外部依赖：IdP（demo 用 Keycloak 或等价）、model provider endpoint；均通过配置接入，不改部署拓扑。 |
| model-provider 边界（ADR-0023） | `jarvis-model-provider` 仅存在于 ai-worker 组合根；`document-processing` 核心层与业务 crate 零依赖；由 `check-architecture.ps1` 强制。 |

## 依赖决策（M1 定稿事实）

- `jarvis-model-provider`（lib 名 `jarvis_model_provider`）核心 trait 为
  `ModelProvider::complete/stream`，提供 `OpenAiCompatibleProvider`、
  `AnthropicProvider` 与测试用 `MockProvider`/`ScriptedProvider`；
- 错误模型 `ProviderError`（`ProviderErrorKind` + `FailurePhase` + retry-after）
  可映射到既有 `ExtractionError`/`ClassifiedProcessingFailure`；
- 依赖兼容（M1 实测，见 ADR-0023 第 8 节）：reqwest 0.12 / tokio 1 / edition 2021 /
  rust-version 1.94.1 满足；thiserror 1（1.0.69）与本仓库 thiserror 2（2.0.19）在 Cargo
  图中并存；reqwest 特性并集引入 default-tls（native-tls/schannel/openssl）与 rustls 双
  TLS 栈，接受并记录；tokio 单一版本 1.53.1；
- **接入方式（定稿）**：git dependency 锁定已推送 rev
  `0485827bd3cf735527de330c42aa6c4d85552b92`（GitHub 远端 main 的已验证 HEAD，不跟踪
  main）。仓库根 `.cargo/config.toml` 设 `net.git-fetch-with-cli = true` 使本地与 CI 用系统
  git CLI 拉取，规避 libgit2 内置 fetch 对匿名 public 仓库的偶发 HTTP 401；
- **CI 可访问性（验证通过）**：`jesenzhang/jarvis-rs` 为 public 仓库，`git ls-remote`/
  `git clone` 与 `cargo check` 均无需凭据成功解析；GitHub Actions 拉取可行性成立（最终
  以 Main CI 为准）。无 vendor 触发。注意：与原假设不同，本机未发现 jarvis-rs 本地工作树
  clone，因此不依赖"本地未提交修改"这一前提，直接锁定远端已推送 rev。

## 里程碑

### M0 — 基线收尾：PLAN-0007 完成审计与归档（~5h）

| 任务 | 内容 | 预估 |
|---|---|---|
| T0.1 | 核对 PLAN-0007 完成定义：React 上传→Processing→Review 链路、CLI 稳定 JSON、MCP tools/list，逐项记录 PASS/NOT RUN 及原因 | 1.5h |
| T0.2 | 运行全量门禁：Rust 四件套 + check-architecture + check-openapi + console lint/typecheck/test/build + Playwright smoke | 2h |
| T0.3 | 撰写完成审计报告入 `docs/reports/`，PLAN-0007 归档至 `archive/2026/`，同步 `ARCHITECTURE_STATUS.md` 与 `docs/plans/README.md` | 1.5h |

### M1 — model-provider 集成决策与 ADR（~6h）

| 任务 | 内容 | 预估 | 状态 |
|---|---|---|---|
| T1.1 | 验证 git dependency 可解析（锁定 rev 0485827）；`cargo check -p ai-worker` 通过；确认 GitHub Actions 拉取可行性 | 2h | ✅ 完成 |
| T1.2 | ADR-0023：model-provider 依赖选择、版本锁定策略、reqwest 特性并集代价、密钥边界、可替换性（`DocumentFieldExtractor` port 保持稳定） | 2h | ✅ 完成 |
| T1.3 | 更新本计划架构预检为定稿事实；确认 ai-worker 是唯一允许依赖 model-provider 的 crate，并在 `check-architecture.ps1` 增加对应门禁 | 2h | ✅ 完成 |

### M2 — AI Provider 适配层实现（~14h）

架构约束：`jarvis_model_provider` 只出现在 ai-worker 组合根；
`document-processing` 核心层零新增依赖；`DeterministicLocalExtractor`
保留为测试/离线降级路径，由配置开关选择。

| 任务 | 内容 | 预估 |
|---|---|---|
| T2.1 | 契约设计：`ModelBackedExtractor` 实现 `DocumentFieldExtractor`；错误映射表（ProviderErrorKind → ExtractionError/失败分类），不泄漏凭据与原始响应 | 2h |
| T2.2 | 适配器实现：prompt 构造、响应解析为 Candidate 字段、超时/重试语义（复用既有 AI Task retry 分类） | 4h |
| T2.3 | 密钥接入：`runtime-config` secret_url 承载 provider API key；fail-closed 校验；日志脱敏验证 | 2h |
| T2.4 | 契约测试：`MockProvider`/`ScriptedProvider` 注入测试（正常、协议错误、超时、429 retry-after、abort）；确定性提取器回归 | 3h |
| T2.5 | ai-worker 组合注入 + 配置开关（deterministic/real）；lease/fence 语义回归；真实 provider 手工 smoke（可选，密钥可用时） | 3h |

### M3 — 真实认证（~12h）

| 任务 | 内容 | 预估 |
|---|---|---|
| T3.1 | OIDC/JWT 验证：JWKS 拉取与缓存、issuer/audience/exp 校验、fail-closed（生产无 IdP 配置即拒绝） | 4h |
| T3.2 | 租户/权限映射：JWT 声明→TenantContext/ManagementPermission；401/403/跨租户 not-found 契约测试 | 3h |
| T3.3 | demo compose 增加 IdP（Keycloak 或等价轻量方案）+ console 登录流程 + token 刷新 | 3h |
| T3.4 | CLI/MCP token 传递适配与测试 | 2h |

### M4 — 预生产环境与可观测性（~14h）

| 任务 | 内容 | 预估 |
|---|---|---|
| T4.1 | observability crate 落地：结构化 JSON 日志 + 请求 ID 贯穿 API/Worker | 4h |
| T4.2 | `/metrics` 暴露（处理任务吞吐、租约、AI 时延/失败率）+ 基础 dashboard 配置 | 3h |
| T4.3 | 备份/恢复演练脚本（PostgreSQL pg_dump/restore + MinIO mirror）+ 演练记录 | 3h |
| T4.4 | 安全扫描接入 CI：cargo-audit / gitleaks / trivy（当前持续 NOT RUN 的缺口） | 2h |
| T4.5 | Runbook 初版：部署、升级、回滚、故障处置 | 2h |

### M5 — v0.1 发布审计（~7h）

| 任务 | 内容 | 预估 |
|---|---|---|
| T5.1 | 性能/容量 smoke 基线：上传、列表 keyset、处理吞吐、并发 worker | 3h |
| T5.2 | 预生产端到端演练：真实认证 + 真实 AI 提取 + 备份恢复全链路，记录证据 | 2h |
| T5.3 | 完成审计报告 + tag `v0.1` + 全文档状态同步 | 2h |

## 里程碑依赖

```text
M0 ──→ M1 ──→ M2 ──┬──→ M5 (v0.1)
              ┌─────┴─────┐
              M3(认证)    M4(环境/可观测)   ← M3/M4 可与 M2 后期并行
```

## 质量属性与验收

- 安全：生产配置无 IdP/provider 密钥时 fail-closed 拒绝启动相应能力；
  API key、原始 provider 响应不进入日志、DTO 和错误响应（有回归测试）。
- 恢复：AI Task 在 provider 超时/5xx/429 下的重试与死信分类符合既有语义；
  认证中间件故障不拖垮健康检查。
- 兼容性：OpenAPI 契约不变；CLI/MCP 仅增加 token 参数，输出格式不变。
- 可维护性：架构门禁保证 model-provider 仅存在于 ai-worker；替换 provider
  实现不需要修改 document-processing 核心。
- 可观测性：请求 ID 贯穿 API→Worker→AI Task 日志；/metrics 可被 scrape。

## Fitness Functions

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `scripts/check-architecture.ps1`（含新增 model-provider 隔离门禁）
- `scripts/check-openapi.ps1`
- console `npm run lint/typecheck/test/build` + Playwright smoke（M0/M3）
- M2 新增：MockProvider 契约测试、密钥脱敏回归测试
- M3 新增：JWKS 验证、token 过期/篡改、跨租户隔离契约测试
- M4 新增：备份恢复演练脚本化验证、安全扫描 CI job

## 风险

| 风险 | 等级 | 缓解 |
|---|---|---|
| jarvis-rs main 不稳定（本地有未提交 V1 hardening） | 高 | git dep 锁定已推送 rev；升级显式走 ADR 修订 |
| GitHub Actions 无法拉取 jarvis-rs | 中 | M1 首先验证；不可用则 vendor 源码（ADR-0023 记录） |
| reqwest 特性并集引入 native-tls | 低 | 接受并记录；编译与体积代价可测量 |
| Keycloak 增加 demo 复杂度 | 中 | 可先落地 JWKS 静态验证最小闭环，IdP 完整接入后置到 T3.3 |
| 晚期评审一次性否决（PLAN-0011 教训） | 中 | M1 ADR 即独立评审；M2/M3 完成即做 focused 评审，不留到最后 |

## 文档、部署与回滚

- 同步：ADR-0023（新增）、`DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`
  （AI provider 边界章节）、`DEPLOYMENT_ARCHITECTURE.md`（IdP/预生产）、
  `OBSERVABILITY_ARCHITECTURE.md`（实现状态）、`ARCHITECTURE_STATUS.md`；
- 迁移 manifest 仅在新增文件时更新；
- 回滚：认证回退 dev token（配置开关）；AI 提取回退 deterministic（配置
  开关）；移除 model-provider 依赖只需还原 ai-worker 组合根与 Cargo.toml。
  不回滚既有迁移，不删除业务事实。

## 完成定义

全部里程碑完成且各自验收 PASS 或带原因 NOT RUN；预生产环境可完成
"登录→上传→真实 AI 提取→Review→备份恢复"全链路演练并有记录；安全扫描
CI job 生效；v0.1 tag 存在且指向通过 Main CI 的提交；PLAN-0012 归档。
