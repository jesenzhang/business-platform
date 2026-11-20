# 2026-11-20 依赖升级盘点（只读）

> 范围：仅盘点，不修改 `Cargo.toml`、`Cargo.lock`、`package.json`、`package-lock.json` 或 CI 配置。
> 基线：`main` @ `ac1774c`（PLAN-0018 已 Integrated，CI run 44690958371 10/10 全绿）。
> 方法：`cargo update --dry-run`、`cargo outdated --workspace`、`npm outdated --json`、本地配置文件读取；审计状态取自 `cargo audit --deny warnings`（EXIT=0，剩余 2 项带理由 ignore）。

## 1. 当前安全态势

- 已关闭：`RUSTSEC-2026-0459`（libsqlite3-sys，PLAN-0018）。
- 仍保留的 ignore：
  - `RUSTSEC-2023-0071`：rsa 仅来自 sqlx 未启用的 `mysql` feature，代码未编译；升级 rsa 本身不会降低暴露面。
  - `RUSTSEC-2026-0543`：chrono 全版本 unmaintained，advisory 无 patched 版本；“升级版本”无效，只能迁移到 jiff/time 或继续接受。

## 2. Rust：可小步升级项

`cargo update --dry-run` 只提出 semver 兼容更新，共 7 个包：

| 包 | 当前 | 建议 |
|---|---|---|
| clap / clap_derive | 4.6.8 | → 4.6.9 |
| generic-array | 0.14.7 | → 0.14.9 |
| pin-project / pin-project-internal | 1.1.13 | → 1.2.0 |
| pin-project-lite | 0.2.17 | → 0.2.18 |
| crypto-common | 0.1.7 | 工具建议“降级”到 0.1.6（同时提示 available 0.1.7） |

注意：`crypto-common` 的“降级”来自 resolver 与 `generic-array` 的约束组合，**不应盲目整包套用** `cargo update`；需要单独核对 `cargo tree -i crypto-common` 后再决定。其余 6 项属于常规补丁/次版本刷新，风险低。

## 3. Rust：破坏性升级候选

`cargo outdated --workspace` 中与直接依赖或关键传递链相关的主要版本差：

| 包 | 当前 → 最新 | 类型 | 说明 |
|---|---|---|---|
| reqwest | 0.12.28 → 0.13.5 | 直接（workspace） | HTTP 客户端主版本，需核 API/行为变化与 TLS feature |
| sha2 | 0.10.9 → 0.11.1 | 直接（workspace） | 摘要 API 可能有破坏变化，影响多个 crate |
| base64 | 0.22 → 0.23 | 直接（workspace） | 编码 API/feature 变化需核 |
| digest / block-buffer / crypto-common | 0.10.7 / 0.10.4 / 0.1.7 → 0.11.3 / 0.12.1 / 0.2.2 | 传递 | 通常由 sha2 主版本升级带动 |
| syn | 2.0.119 → 3.0.10 | 传递（lock 中已并存 3.0.10） | 宏生态兼容需核，但非直接依赖 |
| thiserror | 1.0.69 → 2.0.21 | 传递重复项 | workspace 直接依赖已是 2；旧版本来自某个未刷新依赖链 |
| wasm-streams | 0.4.2 → 0.5.0 | wasm32 only | 当前构建目标不受影响 |

这些应**逐项**开专项 PR，而不是合并成一次大升级。

## 4. 前端依赖

`npm outdated`（apps/business-console）共 25 项：

- **semver 范围内可刷新（13 项）**：`@playwright/test`、`@tanstack/react-query`、`@types/react(-dom)`、`autoprefixer`、`echarts-for-react`、`postcss`、`react`/`react-dom`、`react-router-dom`、`typescript-eslint`、`vite`、`zustand`。这些属于 lockfile 刷新，不改变 `package.json` 的版本约束。
- **主要版本落后（13 项）**：`@eslint/js` 10、`@types/node` 26、`@vitejs/plugin-react` 6、`eslint` 10、`eslint-plugin-react-hooks` 7、`globals` 17、`jsdom` 31、`lucide-react` 1.77、`tailwindcss` 4.3、`typescript` 7、`vite` 8、`vitest` 5 等。
  - **Tailwind 3 → 4 不得执行**：AGENTS.md §17 明确保留 Tailwind 3，禁止引入 Astryx 的 Tailwind v4 桥。
  - TypeScript 7、Vite 8、ESLint 10、Vitest 5 均属于工具链主版本，需单独验证构建、类型检查、Playwright 与 Astryx doctor。
  - Astryx 包当前固定 0.7.16，升级必须按 `docs/operations/ASTRYX_UPGRADE_RUNBOOK.md` 执行，不能夹带在通用依赖升级里。

## 5. 工具链

- Rust：`rust-version = 1.94.1`，CI `RUST_TOOLCHAIN = "1.94.1"`；sqlx 0.9.1 的 MSRV 为 1.94.0，当前正好满足。升级工具链前需重新核所有依赖 MSRV 与 CI 镜像。
- Node：CI frontend job 固定 Node 22；本地 npm 11.4.2。前端升级若涉及 Node 要求，须同步 CI。

## 6. 建议的受控升级顺序

1. **低风险 lockfile 刷新**：先做前端 semver 范围内刷新（不改 `package.json` 约束），跑全部前端门禁 + Playwright + Astryx doctor；单独 PR、单独回滚。
2. **Rust 小步刷新**：只应用 clap / pin-project / generic-array 等明确安全的兼容更新；**排除** crypto-common 的自动降级，先查依赖树。
3. **直接依赖主版本专项**：reqwest 0.13、sha2 0.11、base64 0.23 各一个 PR，逐项核 MSRV、feature 与公共 API 影响。
4. **chrono 迁移专项**：这是唯一能真正移除剩余安全 ignore 的路径，但会改公共时间/序列化契约与 OpenAPI/DB 映射，必须先立 ADR 与计划（可复用 PLAN-0018 的评审方法）。
5. **rsa**：保持现状，直到 sqlx 接受修复版本或启用 mysql feature。

## 7. 执行结果（2026-11-20 续）

盘点后已获授权执行升级，按独立 PR 逐个验证并合并：

| 阶段 | PR | CI run | 结果 |
|---|---|---|---|
| 前端 lockfile + Rust 小步刷新 | #22 | 44698514751 | 10/10 全绿，已合并 |
| reqwest 0.13（workspace + vendored jarvis，`rustls-tls` → `rustls`） | #23 | 44701587467 | 10/10 全绿，已合并 |
| sha2 0.11 + base64 0.23（digest 输出改用 `hex` 编码） | #24 | 44704620728 | 10/10 全绿，已合并 |

三次升级均先在隔离分支通过本地 `cargo check`、`cargo audit`，再由完整 CI 验证；`main` 已完成合并。最终主干 CI 以合并后的最新 run 为准。

### 未执行项与理由

- **chrono（RUSTSEC-2026-0543）**：advisory 无 patched 版本，升级版本无法关闭；迁移到 jiff/time 涉及约 **101 个文件、1002 行** chrono 相关代码，且触及公共 DTO、时间/序列化契约与 OpenAPI/DB 映射。按 AGENTS.md §4/§14 必须先通过 ADR 与专项设计，不能夹带在通用依赖升级中；因此本次保留带理由与复审触发条件的 ignore，不宣称已关闭。
- **rsa（RUSTSEC-2023-0071）**：仅来自未编译的 sqlx mysql feature，升级不降低暴露面，保持 ignore。
- **Tailwind 3 → 4**：AGENTS.md §17 明令禁止，不执行。
- **TypeScript 7 / Vite 8 / ESLint 10 / Vitest 5 / jsdom 31 等前端主版本**：需独立工具链验证，不在本次“依赖升级且项目仍可运行”的范围内执行。

本轮结论：低风险刷新与三个直接依赖主版本升级已全部完成并经 CI 验证；剩余 ignore 均为有明确理由、无法通过单纯版本升级消除的项。
