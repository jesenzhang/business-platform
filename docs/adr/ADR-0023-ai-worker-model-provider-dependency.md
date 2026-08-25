# ADR-0023：ai-worker 的 model-provider 依赖与可替换性边界

> 状态：Accepted（含 2026-08-25 修订：接入形式从 git 依赖改为 vendored 源码）  
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
- **可逆性**：接入必须可回退——移除依赖只需还原 ai-worker 组合根与其 Cargo.toml；生产路径可用配置开关在 deterministic 与 real provider 间切换。
- **交付成本**：仓库 owner 事件已激活该库且 API 与需求高度吻合（`ProviderErrorKind`/`FailurePhase`/`retry-after` 可直接映射），避免自研协议层。
- **生态与许可证**：jarvis-rs 为 MIT。**关键事实（修订依据）**：`github.com/jesenzhang/jarvis-rs` 对 GitHub Actions 运行时不可访问（私有/需凭据），匿名 API 与网页均返回 404；本机可拉取仅因本机缓存了凭据。Main CI 上所有依赖该 git 依赖的 cargo 任务（check/clippy/test/metadata/迁移）在 dependency 解析阶段全部失败。
- **可维护性**：`jarvis-model-provider` 只出现在 ai-worker 组合根，`document-processing` 核心层零新增依赖；provider 实现替换不触碰核心。

## 3. 候选方案

### 方案 A：git dependency 锁定已推送 rev（初选，后因 CI 不可达放弃）

描述：声明 `jarvis-model-provider = { git = "https://github.com/jesenzhang/jarvis-rs", rev = "<full-sha>" }`。

优点：
- 源码与其依赖在同一仓库，锁定精确 rev、不跟踪 main，升级需显式改 rev 并经 ADR 修订。

缺点与风险（最终判定不可行）：
- `Cargo.lock` 保留 git source；**CI 无法拉取私有/受限仓库**。实测 Main CI 拉取该 rev 抛 401/404，
  所有依赖解析的 cargo 任务失败。本机通过波形为 git CLI，能够解析仅因为缓存了凭据；GitHub Actions 无该凭据。
- 传递依赖并集（reqwest native-tls、thiserror 1）不可裁剪，见第 6 节。

### 方案 B：vendored 源码纳入本仓库（选定）

描述：将 model-provider 的锁定 rev 源码复制进 `vendor/jarvis/model-provider/`，ai-worker 以
path dependency 引用。

优点：
- 完全本地、无外部拉取依赖，CI 自包含、不依赖远程仓库可用性。
- 保持与上游 rel 的显式关联（Cargo.toml 头注释记录 provenance rev），可审计、可完整回放。
- 依赖类型为 path+本地，`check-architecture.ps1` 可精确断言隔离边界。

缺点与风险：
- 丢失与上游的自动版本跟踪；维护负担转移给本仓库。接受的代价：升级需人工从上游摘录新 rev 并保留 provenance。
- 传递依赖并集仍不可裁剪（reqwest 默认特性触发 default-tls），见第 6 节。
- 需在仓库内新增 vendor 目录与源码副本；边界必须由门禁约束，仅 ai-worker 可依赖。

### 方案 C：crates.io 发布后依赖 registry

描述：等待 jarvis-model-provider 发布到 crates.io。

优点：版本语义清晰。
缺点与风险：当前未发布，阻塞 v0.1 时间线。留作未来可选迁移路径，不构成当前接入。

## 4. 决策

**采用方案 B（vendored path dependency）**：`ai-worker` 以
`jarvis-model-provider = { path = "../../vendor/jarvis/model-provider" }` 接入，源码 vendored 自
`github.com/jesenzhang/jarvis-rs@0485827bd3cf735527de330c42aa6c4d85552b92`。

- **决策变化记录**：初选方案 A（git rev）。T1.1 本地验证 `cargo check` 通过；但 Main CI 实测无法拉取
  `jesenzhang/jarvis-rs`（私有/受限，匿名 404），全部 cargo 任务在 dependency 解析失败。按 PLAN-0012
  预定的回退策略（"若不可访问则降级为 vendored 源码"）切换为本方案。
- **接入形式约束**：`jarvis-model-provider` 只存在于 `apps/ai-worker/Cargo.toml`；源头
  `vendor/jarvis/model-provider/` 独一份；`document-processing` 核心层、`business-api`、其它 worker、
  contract/compiler crate 一律不得依赖它。该约束由 `scripts/check-architecture.ps1` 强制（T1.3 门禁）。
- **版本锁定**：vendored 源码 = 锁定上游 rev `0485827` 的快照；Cargo.toml 头注释记录 provenance，
  禁止任何依赖声明回退到 git URL。
- **不保留 git 拉取变通配置**：删除为实现 git 依赖而加的 `.cargo/config.toml`（`net.git-fetch-with-cli`），
  因为不再有 git 依赖；保留会让未来误以为 CI 可拉取私有仓库。

## 5. 边界与非目标

决定：
- 接入来源与锁定策略（vendored path，provenance 锁定 rev）。
- 传递依赖并集的接受与记录。
- 密钥承载方式与脱敏要求。
- provider 实现的可替换性（保持 `DocumentFieldExtractor` port 稳定）。
- model-provider 隔离门禁。

不决定：
- 不在本 ADR 实现 `ModelBackedExtractor`（M2）、密钥种子接入（M2 T2.3）、契约测试（M2 T2.4）。
- 不把 model-provider 提升为平台核心依赖；不做通用 Model Gateway 抽象。
- 不引入 Keycloak/IdP（M3）；不新增 OpenAPI 契约。
- 不推进 jarvis-rs 仓库公开化或为 CI 授予私有仓库读取凭据（那是仓库 owner 的运维决策；若采用，
  本 ADR 应重新评估是否切回 git/registry 方案）。

## 6. 后果

### 正面

- 管道自包含：CI 无需外部凭据即可解析依赖；Main CI 不再因私有仓库而失败。
- provider 错误模型与既有失败分类高度吻合，M2 映射成本低。
- 门禁可精确断言 vendored path 与隔离边界，可审计、可回退。

### 负面与成本

- **reqwest TLS 并集**：工作区原有
  `reqwest = { default-features = false, features = ["json","rustls-tls","multipart"] }`；
  jarvis-model-provider 依赖的 reqwest 带默认特性（含 default-tls）。Cargo 对同一 reqwest 版本做
  feature 并集后，ai-worker 依赖树同时编译 native-tls（`hyper-tls`→`native-tls`→`schannel`/`openssl`）
  与 rustls（`hyper-rustls`）。Windows 上 native-tls 走 schannel，Linux CI 走 openssl。已接受并记录。
- **thiserror 双版本**：jarvis-rs 用 thiserror 1，本仓库统一 thiserror 2；二者在 Cargo 图中并存
  （`1.0.69` 与 `2.0.19`），各 crates 引用各自版本，编译通过。此为合理并存，非错误。
- **tokio 单一版本**：`tokio` 统一单版本，无重复。
- **维护负担**：vendor 副本不随上游自动同步；升级需人工摘录新 rev 并更新 provenance。源码规模纳入
  本仓库，由隔离门禁控制暴露面（仅 ai-worker 可见）。

### 风险

- 上游 jarvis-rs 演进：vendored 快照无漂移风险；升级需人工摘录并经本 ADR 修订。
- 若未来希望脱离 vendored，可由 jarvis-rs 公开/发布 crates.io 后评估迁移（方案 C）；或为 CI 配置
  私有仓库读取凭据后评估切回 git（方案 A）。
- reqwest openssl 在 CI 的构建可用性：GitHub runner 具 openssl 头文件，Linux 可编译；若遇构建失败按
  PLAN-0012 M2 门禁停止并上报。

## 7. 实施

- 修改范围：
  - `vendor/jarvis/model-provider/`：vendored 源码快照（自 rev `0485827` 摘录，含 src/tests/Cargo.toml/README/ARCHITECTURE）。
  - `apps/ai-worker/Cargo.toml`：新增 path 依赖（T1.1/T1.3）。
  - `Cargo.lock`：记录 jarvis-model-provider 为 `path+` 依赖，无 git source。
  - `scripts/check-architecture.ps1`：model-provider 隔离门禁（断言 vendored path、禁止 git URL、禁止核心 crate 依赖）（T1.3）。
  - `docs/plans/current/PLAN-0012-*.md`：架构预检定稿，记录 CI 不可达→vendored 回退（T1.3）。
  - `docs/architecture/DEPLOYMENT_ARCHITECTURE.md`、`DURABLE_DOCUMENT_PROCESSING_ARCHITECTURE.md`：
    记录 provider 边界（M2 或后续里程碑）。
- 迁移步骤：无数据库迁移；无业务事实变更。
- 兼容策略：不新增公开 API；认证中间件行为（401 语义收紧）属 M3。
- 测试与验收：ai-worker 全量门禁；隔离门禁在 `check-architecture.ps1` 断言 jarvis-model-provider 仅出现于
  `apps/ai-worker/Cargo.toml` 且必须为 vendored path。
- 回滚：移除 ai-worker 的 path 依赖并删除 `vendor/jarvis/`；`Cargo.lock` 随 `cargo` 自动回退；
  AI 提取回退 deterministic 开关（M2）。移除依赖无需触碰业务事实。

## 8. 验证证据

- `cargo check -p ai-worker`：PASS，`jarvis-model-provider v0.1.0 (F:\…\vendor\jarvis\model-provider)` 被编译。
- `Cargo.lock`：jarvis-model-provider 条目无 `source` 字段（path 依赖），依赖含 `thiserror 1.0.69`、
  `sha2 0.10.9`、`tokio`、`reqwest` 等。
- `scripts/check-architecture.ps1`：PASS（含 vendored-path 断言与隔离断言）。
- Main CI：首次 git 依赖提交（72470bb）全部 cargo 任务在 dependency 解析失败（私有仓库不可达）；切换
  vendored 后应恢复绿（见最终 CI 记录）。
- `git ls-remote https://github.com/jesenzhang/jarvis-rs.git` 无需凭据将失败；本机凭据缓存使本地可拉取。

## 9. 后续复审条件

以下任一事实变化时重新评估本 ADR：
- jarvis-model-provider 发布 crates.io（可评估迁移方案 C）。
- jarvis-rs 仓库公开化或为 CI 配置私有仓库读取凭据（可评估切回 git 方案 A，消除 vendor 维护负担）。
- 上游 jarvis-rs 变更需要升级 vendored 快照（人工摘录新 rev）。
- 引入第二个需要 reqwest 默认特性的 crate，导致 TLS 栈进一步并集。
- 平台需要将 model-provider 提升为共享核心依赖（当前明确排除）。