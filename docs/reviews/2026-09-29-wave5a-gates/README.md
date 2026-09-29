# PLAN-0014 Wave 5A — 本地 R2 发布候选工件与外部消费者验证证据

生成时间（UTC）：`2026-09-29T02:49:00Z`（工件与消费者步骤）；门禁步骤见各日志头部时间戳。
源 revision：`8036bc65ba7c77666e48afaa6439911ba7c083d7`（分支 `codex/plan-0014-wave5a-local-release-pilot`）。
工具链：`cargo 1.94.1 (29ea6fb6a 2026-03-24)` / `rustc 1.94.1 (e408947bf 2026-03-25)` / `rust-toolchain.toml` channel `1.94.1`。
宿主：Windows 10 Home China 22H2 (19045)，仓库位于 `F:`（非 NTFS 卷，cargo 会打印 hard-link 提示，属宿主属性，不影响摘要）。

`00-toolchain.txt` 中 `working-tree-dirty : 1 entries` 指本证据目录自身（生成时尚未提交），不是未提交的代码改动：候选工件生成时，代码改动已全部落在 `8036bc65`。

本目录只证明：**本地候选工件可重复生成、可独立验证、可被 workspace 外真实消费者编译调用**。
不证明：R2 判定、发布、注册、安装、部署，也不关闭任何 C1 阻塞。

## 1. 命令 <-> 日志 <-> 退出码

### 1.1 仓库门禁（任务 7，逐条执行）

| 命令 | 日志 | 退出码 |
|---|---|---|
| `cargo fmt --all -- --check` | `60-gate-fmt.log` | 0 |
| `cargo check --workspace --all-targets --all-features` | `60-gate-check.log` | 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | `60-gate-clippy.log` | 0 |
| `cargo test --workspace --all-features` | `60-gate-test.log` | 0 |
| `pwsh -NoProfile -File scripts/check-architecture.ps1` | `60-gate-architecture.log` | 0 |
| `git diff --check` | `60-gate-git-diff-check.log` | 0 |

`cargo test --workspace --all-features`：187 个测试二进制、**833 passed / 0 failed**、**70 ignored**。
70 个 ignored 全部是既有 PostgreSQL lane（`#[ignore]` + 运行说明），本轮无数据库授权，**未执行、未计入通过**（见 4 节）。

定向测试（同一提交的代码状态）：

| 命令 | 日志 | 结果 |
|---|---|---|
| `cargo test -p module-release-tool --all-features` | `61-targeted-module-release-tool.log` | 30 passed / 0 failed（lib 4、canonical 5、compatibility 8、packaging 3、pilot_candidates 10） |
| `cargo test -p business-module-contracts -p business-application-compiler --all-features` | `62-targeted-contract-compiler.log` | 全部 passed / 0 failed（含 compiler 18、planning 26、release_surface_digest 17 等） |

架构门禁新增的 Wave 5A 规则做过**反向验证**：临时给消费者模板注入一条 `path = "../crates/identity"`、以及把 `specs/policy.json` 的 `module_version` 改成平台版本 `0.1.0`，两次 `check-architecture.ps1` 均退出 1；文件还原后恢复退出 0。规则不是永真式。

### 1.2 工件装配与验证（`scripts/plan0014-wave5a.ps1`，脚本汇总见 `assembly-log.md`）

30 个步骤全部命中预期退出码（正向步骤 0，篡改步骤 1）。临时根：`<REPO>\target\wave5a`（仓库内、被 `.gitignore` 覆盖、与仓库同卷；本轮不用系统盘）。

| 步骤 | 日志 | 预期退出码 |
|---|---|---|
| `generate` x {run-a, run-b} x {identity, policy} | `10-generate-run-{a,b}-{identity,policy}.log` | 0 |
| `compare` run-a <-> run-b | `12-compare-{identity,policy}.log` | 0 |
| `verify`（独立进程） | `13-verify-{identity,policy}.log` | 0 |
| packed crate 以自身为 workspace root 的真实构建（在 payload 之外的副本上） | `40-crate-build-{identity,policy}.log` | 0 |
| 构建之后重新 `verify` 工件未被写脏 | `41-verify-after-build-{identity,policy}.log` | 0 |
| 消费者 `cargo build` | `20-consumer-{identity,policy}-build.log` | 0 |
| 消费者 `cargo test`（真实调用公开契约） | `21-consumer-{identity,policy}-test.log` | 0 |
| 消费者 `cargo build --locked --offline`（缓存预热后） | `22-consumer-{identity,policy}-offline-locked.log` | 0 |
| 消费者解析后的依赖与 feature | `23-consumer-{identity,policy}-tree.txt` | - |
| 消费者 `Cargo.lock` 副本 | `24-consumer-{identity,policy}-lock.txt` | - |
| 无回指源工作区路径检查 | `25-consumer-{identity,policy}-path-guard.txt` | 0（hits=0） |
| 篡改：改一个 packed 源字节 | `50-tamper-content-{identity,policy}.log` | 1 |
| 篡改：删一个 payload 成员 | `51-tamper-missing-file-{identity,policy}.log` | 1 |
| 篡改：`release.json` 的 `module_version` | `52-tamper-wrong-version-{identity,policy}.log` | 1 |
| 篡改：`release.json` 的 `module_id` | `53-tamper-wrong-module-id-{identity,policy}.log` | 1 |

## 2. 候选工件与四类摘要

工件目录（生成物，未提交；`artifacts/<module>/` 保存其可离线核对的记录：L2 `artifact-manifest.json` / `release.json` / `release-bundle.json`、L3 `verification.json`，以及三个 L1 payload 记录 `module.manifest.json`、`contract/contract-surface.json`、`migrations/migration-mapping.json` 的副本）：

- `<REPO>\target\wave5a\run-a\identity-0.1.0-rc.1\`、`run-b\identity-0.1.0-rc.1\`
- `<REPO>\target\wave5a\run-a\policy-0.1.0-rc.1\`、`run-b\policy-0.1.0-rc.1\`

上述生成目录是临时工件，收口时已删除（`target/` 本就被 git 忽略）；本目录的 `artifacts/<module>/` 保留其 L2/L3 记录副本。复现命令：`pwsh -NoProfile -File scripts/plan0014-wave5a.ps1 -EvidenceDir <证据目录>`（临时根默认 `<repo>\target\wave5a`，30 步应全部命中预期退出码），复现后四类摘要应与下表逐字相同。唯一会变的字段是 `release.json.source_revision`：它取生成时刻的 HEAD，因此在文档收口提交之后重跑会钉到新的 HEAD——该字段属 L2，不在 `artifact_integrity_digest` 覆盖范围内，摘要不受影响。

| 字段 | identity | policy |
|---|---|---|
| module id / version | `identity` / `0.1.0-rc.1` | `policy` / `0.1.0-rc.1` |
| `contract_digest` | `3fed976b0ac5e040366f696a1c4a7fe6140bd5bc956996fc8b51a141eba31107` | `52e788ff3187846c4484cc42a3891aa7833f89ff65d666c7b3c7e1af831568c8` |
| `package_digest` | `b9dfa42ccd09190020c39ee8bf412507de2107810fc2414efdc3415761053c70` | `37e95a19b45c3dcc00e2f4099d91cc72ed63af71f19d769882412af8b2ddd4b6` |
| `artifact_integrity_digest` | `68deba81c942aaf624f599cd191ef6b09855438b99035cca7010ae4b7d9cc700` | `f8add9e0ce13f0c1e33a52eb2f28a85f726c649534e8d10584cdaf4ff6ecb243` |
| `migration_revision.set_digest` | `ef43cfa9a90a367bb9b774834f5b9c150ca67cd27bce65e4016b1c82a062ae03` | `1a5a43f8b269b3e3624b44a613fa9e498775ad7ccac0cf768018b891168ea597` |
| payload 成员数 | 21 | 23 |
| `source_revision` | `8036bc65...`（= 代码候选提交） | 同左 |
| `build_toolchain` | `rust-toolchain-1.94.1` / `1.94.1` | 同左 |
| 显式排除成员 | `artifact-manifest.json`、`release.json`、`release-bundle.json`、`verification.json` | 同左 |
| `verification.json` 检查项 | 5 项全 PASS | 5 项全 PASS |

五个检查项：`artifact-integrity-digest`、`contract-surface-file-digest`、`release-identity-consistency`、`migration-mapping-matches-formal-catalog`、`source-provenance`（payload 成员增删改在第一个检查项内逐条报差异）。

`compare`（run-a <-> run-b，两个不同临时目录）在两类摘要与成员集合上零差异；`cargo test -p module-release-tool --test packaging` 在测试里独立重做同一件事。

### 2.1 不信任本工具的复核方式

审阅者可以只用 `sha256sum`（或 `certutil -hashfile`）复核，不需要运行本工具：

- `sha256sum artifacts/<module>/contract-surface.json` 应等于表中 `contract_digest`（该文件逐字节就是编译器哈希的前镜像）；
- `artifacts/<module>/artifact-manifest.json` 的 `files[]` 列出每个 payload 成员的规范化 SHA-256 与路径，`excluded_members` 列出被排除的 L2/L3 文件与理由；按规则前缀 `plan-0014.artifact-integrity.v1`（§4.4 第 5、6 条：`"<sha> <path>\n"` 按路径字节序拼接，再前置规则行求 SHA-256）即可从 `package/` 复算 `artifact_integrity_digest`；
- `artifacts/<module>/module.manifest.json` 反序列化后交给 `cargo test -p module-release-tool --test compatibility`（`cargo run -p module-release-tool -- verify` 走同一条路径），可独立重算 `contract_digest` 与 `package_digest`；
- `migration-mapping.json` 每条 `sha256` 应与 `MANIFEST.sha256` 记录一致，也能对磁盘文件直接重算（`raw` 或 CRLF→LF 两种形态之一，字段 `digest_form` 标明用哪种）。

## 3. workspace 外消费者

两个消费者位于 `<REPO>\target\wave5a\consumers\<module>`，由仓库内模板 `tools/module-release-tool/consumers/<module>` 复制而来，`__ARTIFACT_CRATE_DIR__` 被替换为工件内 `package/contract/packages/<module>` 的绝对路径。

| 项 | identity 消费者 | policy 消费者 |
|---|---|---|
| 依赖声明 | `identity = { path = "<artifact>", features = ["testing"] }`，`chrono =0.4.45`，`uuid =1.26.0 (v4,v7)`；dev：`tokio =1.53.1 (macros,rt-multi-thread)` | 同形（`policy`） |
| 解析结果 | `identity v0.1.0-rc.1 (...run-a\identity-0.1.0-rc.1\package\contract\packages\identity)`，`chrono 0.4.45`、`uuid 1.26.0` | `policy v0.1.0-rc.1 (...policy)` |
| `build` / `test` / `build --locked --offline` | 0 / 1 passed / 0 | 0 / 2 passed / 0 |
| 真实调用 | `tenant_access_fails_closed_and_then_grants_a_seeded_membership`（先 `NoMembership` 拒绝，注入成员关系后 `Active`） | `an_unbound_permission_is_denied_by_the_released_contract`、`a_permission_catalog_alone_does_not_grant_anything`（默认拒绝，`policy_reference` 形如 `policy:v1:...`） |
| 图中是否出现 `sqlx` / `*-sqlite` / `axum` / `apps` | 否 | 否 |
| 回指源工作区路径（`<REPO>\crates`、`<REPO>\apps`） | hits=0 | hits=0 |

## 4. NOT RUN / BLOCKED（不得当作通过）

| 项 | 状态 | 原因 |
|---|---|---|
| PostgreSQL 迁移执行、`--include-ignored` PG lane（70 个） | **BLOCKED** | 本轮无数据库授权（承前 G-02 边界）；SQLite/离线证据不得替代 |
| 单模块在 PostgreSQL 上独立安装/升级 | **BLOCKED** | identity 与 policy 与 organization 共享 `migrations/019_identity_authorization_foundation.sql`、同 `public` schema、被 `runtime-migration` 全量 embed（G-203 未关闭）；工件里该条 `module_owned=false` |
| publish / registry 注册 / install / upgrade / rollback 执行 | **NOT RUN** | 任务边界明令禁止；本轮只做本地候选工件 |
| 正式发布 tag、merge、push、部署 | **NOT RUN** | 同上；模块成熟度未改 |
| 漏洞扫描（`cargo audit`）、许可证扫描、镜像扫描 | **NOT RUN** | 本轮未执行，无结果可报 |
| CI | **NOT RUN** | 本地分支，未推送 |
| Document ACL / Notification / 通用日志脱敏 | **不在本轮范围** | 承前边界 |

## 5. 独立审阅栏（不得自填）

| 项 | 结论 |
|---|---|
| 独立 reviewer | `<pending>` |
| Reviewed HEAD | `<pending>` |
| 工件可校验性（第三方仅凭工件 + `sha256sum` 能否复核四类摘要） | `<pending>` |
| 来源可追溯性（`source_revision` 与提交内容） | `<pending>` |
| 外部消费者证据是否成立（是否真正脱离 workspace、是否真正调用公开契约） | `<pending>` |
| 重复构建一致性是否成立（两目录 + `--locked --offline`） | `<pending>` |
| G-01 / G-201 / G-202 / G-203 可否关闭 | `<pending>` |