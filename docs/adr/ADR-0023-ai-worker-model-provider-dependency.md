# ADR-0023：ai-worker 的 model-provider 依赖与可替换性边界

> 状态：Accepted  
> 日期：2026-08-25  
> 决策所有者：Document Intelligence（AI Provider 适配）、平台可观测性  
> 关联文档：[PLAN-0012](../plans/current/PLAN-0012-runnable-v1-auth-ai-provider-observability.md)、
> [DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE](./DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md)、
> [DEPLOYMENT_ARCHITECTURE](./DEPLOYMENT_ARCHITECTURE.md)  
> 替代：无  
> 被替代：无

## 1. 背景

v0.1（PLAN-0012 M2+）需要让 `ai-worker` 具备真实 LLM 字段提取能力，替换生产路径上的
`DeterministicLocalExtractor`。候选三方库 `jarvis-model-provider`（独立仓库
`github.com/jesenzhang/jarvis-rs`）已由一个 owner 事件激活，本 ADR 决定：

1. 是否选用该库，以及以何种形式接入（git rev / vendored / crates.io）；
2. 依赖版本锁定与升级策略；
3. 接受引入的传递依赖代价（reqwest TLS 并集、thiserror 双版本）；
4. 密钥承载边界；
5. 可替换性：`DocumentFieldExtractor` port 在替换 provider 实现时保持稳定。

本 ADR 是决策文档，只决定**接入方式与边界**，不包含提取器实现细节（属 M2）。

## 2. 决策驱动因素

- **正确性**：provider 调用失败必须映射到既有 `ClassifiedProcessingFailure` 重试/死信语义，不引入新的分布式事务或状态机。
- **安全性**：API key 不进入日志、DTO 与错误响应；provider base URL 强制 fail-closed（只允许 HTTPS 或 loopback）；错误响应脱敏。
- **可逆性**：接入必须可回退——移除 git 依赖只需还原 ai-worker 组合根与其 Cargo.toml；生产路径可用配置开关在 deterministic 与 real provider 间切换。
- **交付成本**：仓库 owner 事件已激活该库且 API 与需求高度吻合（`ProviderErrorKind`/`FailurePhase`/`retry-after` 可直接映射），避免自研协议层。
- **生态与许可证**：jarvis-rs 为 MIT、public 仓库，git clone 无需凭据；本次在 Windows 本机与 GitHub 传输均验证可拉取。
- **可维护性**：`jarvis-model-provider` 只出现在 ai-worker 组合根，`document-processing` 核心层零新增依赖；provider 实现替换不触碰核心。

## 3. 候选方案

### 方案 A：git dependency 锁定已推送 rev（选定）

描述：在 `apps/ai-worker/Cargo.toml` 声明
`jarvis-model-provider = { git = "https://github.com/jesenzhang/jarvis-rs", rev = "<full-sha>" }`，
`Cargo.lock` 记录 `git+…?rev=<full-sha>#<full-sha>`。

优点：
- 源码与其依赖在同一仓库，CI 可复现拉取（public，无凭据）；实测 git CLI 解析成功。
- 锁定精确 rev，不跟踪 main，未来升级需显式改 rev 并经 ADR 修订——避免上游不稳定。
- `jarvis-rs` 目前无 crates.io 发布，git 是唯一可获得形式。

缺点与风险：
- 上游 `main` 变化不自动影响本仓库（需要此特性）；升级流程受 ADR 修订约束。
- 传递依赖并集（reqwest native-tls、thiserror 1）不可裁剪，见第 6 节。

### 方案 B：vendored 源码纳入本仓库

描述：将 model-provider 源码复制进 `crates/` 并作为 path dependency。

优点：完全本地、无外部拉取依赖、可裁剪传递依赖。

缺点与风险：丢失与上游的版本关联；维护负担转移给本仓库；与"独立 Rust 仓库"目标冲突；
违反本仓库"不随意新增与任务无关文件"与代码规模约束。仅当 GitHub Actions 无法拉取
jarvis-rs（本接线已验证可拉取）时才降级。

### 方案 C：crates.io 发布后依赖 registry

描述：等待 jarvis-model-provider 发布到 crates.io。

优点：版本语义清晰。

缺点与风险：当前未发布，阻塞 v0.1 时间线；引入社区发布节奏依赖。留作未来可选迁移路径，
不构成当前接入。

## 4. 决策

**采用方案 A**：`ai-worker` 以 git dependency、锁定已推送 rev `0485827bd3cf735527de330c42aa6c4d85552b92`
接入 `jarvis-model-provider`。

- 该 rev 是 GitHub 远端 `main` 于 2026-08-25 的 HEAD（`git ls-remote` 与 `git clone` 均确认存在且可拉取）。
- **接入形式约束**：`jarvis-model-provider` 只存在于 `apps/ai-worker/Cargo.toml`；
  `document-processing` 核心层、`business-api`、其它 worker、contract/compiler crate 一律不得依赖它。
  该约束由 `scripts/check-architecture.ps1` 强制（T1.3 门禁，见下）。
- **复现配置**：仓库根 .cargo/config.toml 设 `net.git-fetch-with-cli = true`，使本地与 CI 用系统 git
  CLI 拉取 git 依赖；避免 libgit2 内置 fetch 在本机对匿名 public 仓库偶发 HTTP 401。

## 5. 边界与非目标

决定：
- 接入来源与锁定策略（git rev）。
- 传递依赖并集的接受与记录。
- 密钥承载方式与脱敏要求。
- provider 实现的可替换性（保持 `DocumentFieldExtractor` port 稳定）。
- model-provider 隔离门禁。

不决定：
- 不在本 ADR 实现 `ModelBackedExtractor`（M2）、密钥种子接入（M2 T2.3）、契约测试（M2 T2.4）。
- 不把 model-provider 提升为平台核心依赖；不做通用 Model Gateway 抽象（非目标是 PLAN-0012 本次没有的）。
- 不引入 Keycloak/IdP（M3）；不新增 OpenAPI 契约（计划无新增）。

## 6. 后果

### 正面

- provider 错误模型与既有失败分类高度吻合，M2 映射成本低。
- 锁定 rev + 门禁隔离 → 可复现、可审计、可回退。
- git CLI 复现配置同时服务本地与 CI，消除 libgit2 偶发 401。

### 负面与成本

- **reqwest TLS 并集**：工作区原有
  `reqwest = { default-features = false, features = ["json","rustls-tls","multipart"] }`；
  jarvis-model-provider 依赖的 reqwest 带默认特性（含 default-tls）。Cargo 对同一 reqwest 版本做
  feature 并集后，ai-worker 依赖树同时编译 native-tls（`hyper-tls`→`native-tls`→`schannel`/`openssl`）
  与 rustls（`hyper-rustls`）。此为可测量代价：额外构建产物与潜在的 openssl 构建要求。Windows 上
  native-tls 走 schannel，Linux CI 走 openssl（需要 openssl-dev 或编译）。已接受并记录。
- **thiserror 双版本**：jarvis-rs 用 thiserror 1，本仓库统一 thiserror 2；二者在 Cargo 图中并存
  （`1.0.69` 与 `2.0.19`），各 crates 引用各自版本，编译通过。此为合理并存，非错误。
- **tokio 单一版本**：`tokio 1.53.1` 统一，无重复。

### 风险

- 上游 jarvis-rs `main` 演进：已锁定 rev，无漂移风险；升级需显式改 rev 并经本 ADR 修订。
- GitHub Actions 拉取：仓库 public，已验证可拉取；若未来私有化需在 CI 配置凭据并按本 ADR 修订。
- reqwest openssl 在 CI 的构建可用性：GitHub runner 具 openssl 头文件，Linux 可编译；若遇构建失败按
  PLAN-0012 M2 门禁停止并上报。

## 7. 实施

- 修改范围：
  - `apps/ai-worker/Cargo.toml`：新增 git 依赖（已完成，T1.1）。
  - 根 `.cargo/config.toml`：`net.git-fetch-with-cli = true`（已完成，T1.1）。
  - `Cargo.lock`：记录 `jarvis-model-provider` 的 git source（T1.1 cargo 生成）。
  - `scripts/check-architecture.ps1`：新增 model-provider 隔离门禁（T1.3）。
  - `docs/plans/current/PLAN-0012-*.md`：架构预检定稿（T1.3）。
  - `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`、`DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`：
    记录 provider 边界（M2 或后续里程碑）。
- 迁移步骤：无数据库迁移；无业务事实变更。
- 兼容策略：不新增公开 API；认证中间件行为（401 语义收紧）属 M3。
- 测试与验收：ai-worker 全量门禁；新增隔离门禁在 `check-architecture.ps1` 断言
  `jarvis-model-provider` 仅出现于 `apps/ai-worker/Cargo.toml`。
- 回滚：还原 `apps/ai-worker/Cargo.toml` 与 `.cargo/config.toml`；`Cargo.lock` 随 `cargo` 自动回退；
  AI 提取回退 deterministic 开关（M2）。移除 git 依赖无需触碰业务事实。

## 8. 验证证据

- `cargo check -p ai-worker`：PASS，`jarvis-model-provider v0.1.0 (git …?rev=0485827…#0485827)` 被编译。
- `cargo tree -p ai-worker -i reqwest`：唯一 jarvis-model-provider → ai-worker 路径。
- `Cargo.lock`：记录 `git+https://github.com/jesenzhang/jarvis-rs?rev=0485827…#0485827`；
  `thiserror` 双版本（1.0.69 / 2.0.19）；`tokio 1.53.1` 单版本；reqwest TLS 并集证据（hyper-tls + hyper-rustls）。
- `git ls-remote https://github.com/jesenzhang/jarvis-rs.git` 返回 HEAD `0485827bd3…`（main）。
- .cargo/config.toml 使 libgit2 401 场景经系统 git CLI 成功获取。

## 9. 后续复审条件

以下任一事实变化时重新评估本 ADR：
- jarvis-model-provider 发布 crates.io（可评估迁移方案 C）。
- 上游 jarvis-rs 变更影响本仓库锁定 rev（升级需修订）。
- GitHub Actions 拉取失败（仓库私有化或网络策略）。
- 引入第二个需要 reqwest 默认特性的 crate，导致 TLS 栈进一步并集。
- 平台需要将 model-provider 提升为共享核心依赖（当前明确排除）。