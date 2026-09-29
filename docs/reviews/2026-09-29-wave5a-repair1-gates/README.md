# PLAN-0014 Wave 5A Repair 1 — 修复后的候选工件、门禁与负例证据

生成时间（UTC）：`20260929T075734Z`（编排步骤），门禁步骤时间戳见各日志头部。
代码候选（工件里钉的 `source_revision`）：
`69d9a6b01184db7d9f3d4aa3b688fb569387b2fc`，分支
`codex/plan-0014-wave5a-local-release-pilot`。候选生成时 `git status --porcelain`
为 0 条（`00-toolchain.txt`），即工件的全部输入都来自该提交。
工具链：`cargo 1.94.1 (29ea6fb6a 2026-03-24)` / `rustc 1.94.1 (e408947bf 2026-03-25)` /
`rust-toolchain.toml` channel `1.94.1`。宿主 Windows 10 (19045)，仓库在 `F:`（非 NTFS，
cargo 会打印 hard-link 提示，属宿主属性，不进摘要）。

本目录只证明：**本地候选工件可重复生成、其来源可被 Git 复核、可被 workspace 外（含仓库
目录之外）的真实消费者编译调用，且每个负例都因被断言的那条检查被拒**。
不证明：R2 判定、发布、注册、安装、部署；不关闭任何 C1 阻塞。

## 1. 提交链

| 提交 | 内容 |
|---|---|
| `d0b2d81ffd6a1c5efcdbdd89fec18f9ebdac8c7a` | Wave 5A 原实施记录 + 原证据（被审阅的对象，保留不改） |
| `f198454473ad6b2826ed6cd602845a0ec6bb9949` | 归档本轮独立审阅（NEEDS REPAIR / NOT PUBLISHED），即修复指令 |
| `b88745c4325a69bc0b093bf89d0222c46740297c` | F1–F5 修复代码 + 回归测试 + 脚本重写（Rust 侧最终形态） |
| `69d9a6b01184db7d9f3d4aa3b688fb569387b2fc` | 编排脚本把逐例注入说明与原始日志随证据归档 —— **本目录绑定的代码候选** |

本轮证据目录内的所有日志由**同一提交的一次连续运行**产生。同日更早一次草稿运行
（绑定 `b88745c4`，脚本尚不归档逐例日志）未进入版本库，其产物已移出仓库目录，不在本目录。

## 2. 目录索引

| 路径 | 内容 |
|---|---|
| `F1-F5-closure.md` | 审阅要求的逐项关闭表：修复位置 / 回归名称 / 命令 / 日志 / 实际结果，含先失败回归 |
| `assembly-log.md` | 编排的每一步命令、退出码、期望值、日志文件名 + 负例判定表 + NOT RUN 摘要 |
| `negatives/<case>/` | 15 个负例各自的 `case.md`（注入说明 + 实际改动文件的 sha256 前后值 + 两条命令与退出码）与原始日志（42 个文件） |
| `gates/` | 评审 §4.3 的 8 条门禁命令 + 新脚本回归入口，逐条日志与退出码（`gate-table-rows.md`） |
| `script-regression/` | `scripts/test-plan0014-wave5a.ps1` 的 9 条编排助手回归（含 3 条自测） |
| `red-run/` | 修复前候选 `8036bc65` 上的反例复现（脚本 + 日志 + 汇总表）与 F4/F5 的既有历史证据 |
| `ignored-tests-classified.md` / `ignored-tests-inventory.json` | 70 个 `#[ignore]` 的重新分类清单，纠正「全部 PostgreSQL lane」的旧表述 |
| `artifacts/<module>/` | 两模块的 L1/L2/L3 记录副本（含 `verification.json` 的 10 项检查与明细） |
| `00-toolchain.txt` / `01-cargo-config.txt` | 工具链与 Git 状态；cargo 配置来源（搜索路径逐个列出） |
| `2*/3*` 编号日志 | packed crate 真实构建、两个仓外消费者 + 一次仓库目录外复建、`cargo metadata` 文档与 guard 判定 |

三处 `*-cargo-metadata.log` 里的 metadata 正文（每份约 150 KB，与同名 `.json` 完全重复）
已折叠为一行说明，指向旁边真正交给 `guard` 的那份 `.json`；命令、工作目录、UTC 与退出码
都原样保留。归档副本另外统一去掉了行尾空白与文件末尾多余空行（对齐 `git diff --check` 的口径），不改任何命令行、退出码、摘要或判定文本；除此之外本目录里的日志一字未改。
## 3. 门禁（评审 §4.3 的确切命令）

| 命令 | 退出码 | 日志 |
|---|---|---|
| `rtk proxy cargo test -p module-release-tool --all-features` | 0 | `01-test-module-release-tool.log` |
| `rtk proxy cargo test -p business-module-contracts -p business-application-compiler --all-features` | 0 | `02-test-pure-crates.log` |
| `rtk proxy cargo fmt --all -- --check` | 0 | `03-fmt-check.log` |
| `rtk proxy cargo check --workspace --all-targets --all-features` | 0 | `04-check-workspace.log` |
| `rtk proxy cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | `05-clippy-workspace.log` |
| `rtk proxy cargo test --workspace --all-features` | 0 | `06-test-workspace.log` |
| `rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 | `07-check-architecture.log` |
| `rtk git diff --check` | 0 | `08-git-diff-check.log` |
| `rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1 -EvidenceDir docs\reviews\2026-09-29-wave5a-repair1-gates\script-regression` | 0 | `09-script-regression.log` |

编排（同一目录，另附 `assembly-log.md`）：

```powershell
rtk proxy pwsh -NoProfile -File scripts/plan0014-wave5a.ps1 -OutRoot target/wave5a-repair1 -EvidenceDir docs/reviews/2026-09-29-wave5a-repair1-gates
```

`cargo test --workspace --all-features`：**190 个测试二进制 / 855 passed / 0 failed / 70
ignored**。70 个 ignored **未被执行、不计入通过**，其真实构成见
`ignored-tests-classified.md`（41 个与 PostgreSQL 有关、28 个只要 S3/MinIO 对象存储、1 个
要内网模型 provider）——旧记录里「70 个 ignored 全部是 PostgreSQL lane」的说法在此更正。
定向：`module-release-tool` **52 passed**（lib 4、canonical 5、compatibility 8、isolation 8、
packaging 3、pilot_candidates 10、provenance 6、verification_guards 8）；两个纯 crate
**113 passed / 0 failed**，其源码本轮零改动。

## 4. 两模块四类摘要（本轮实测，`source_revision = 69d9a6b0…`）

| 模块 | contract_digest | package_digest | artifact_integrity_digest | migration_set_digest | 成员 |
|---|---|---|---|---|---|
| `identity` | `3fed976b0ac5e040366f696a1c4a7fe6140bd5bc956996fc8b51a141eba31107` | `b9dfa42ccd09190020c39ee8bf412507de2107810fc2414efdc3415761053c70` | `68deba81c942aaf624f599cd191ef6b09855438b99035cca7010ae4b7d9cc700` | `ef43cfa9a90a367bb9b774834f5b9c150ca67cd27bce65e4016b1c82a062ae03` | 21 |
| `policy` | `52e788ff3187846c4484cc42a3891aa7833f89ff65d666c7b3c7e1af831568c8` | `37e95a19b45c3dcc00e2f4099d91cc72ed63af71f19d769882412af8b2ddd4b6` | `f8add9e0ce13f0c1e33a52eb2f28a85f726c649534e8d10584cdaf4ff6ecb243` | `1a5a43f8b269b3e3624b44a613fa9e498775ad7ccac0cf768018b891168ea597` | 23 |

四类摘要与 `8036bc65` 一轮逐字相同（`artifact_integrity_digest` 亦同：本轮改的是**验证**，
不是打包字节）；随身份一起变化的只有 `source_revision` 一个字段。

正向链路（每项都是编排里的一行，退出码见 `assembly-log.md`）：

1. 两个不同临时根各生成一次 → `compare` 宣告两侧一致（重算后的 payload 摘要与清单逐项相同）；
2. 独立进程 `verify` 两模块各一次，10 项检查全过；
3. packed crate 在 payload 之外的副本上以自身为 workspace root `--all-features` 构建，
   构建后原工件复验仍 PASS（构建不回写工件）；
4. 两个 workspace 外消费者 `build` / `test` / `--locked --offline` 全绿；
5. 每个消费者的 `cargo metadata --format-version 1 --locked --offline` 完整图交给
   `module-release guard` 判定：`status=isolated`，逐包 `verdict=consumer` /
   `verdict=allowed: <工件目录>`，registry 包分别 28 / 20 个；
6. **仓库目录之外**的同卷目录（`F:\wave5a-repair1-outside\<stamp>\consumers\identity`）
   重新构建 + 测试 + guard 判定同样 isolated；该目录的 cargo 配置来源记在
   `30-outside-cargo-config-identity.txt`（`cargo config get` 是 nightly-only 子命令，
   故按文档化搜索路径逐级列出 `.cargo/config.toml` 与 `CARGO_*` 环境变量，并给出结论行）。
   上一轮把 `target\wave5a\consumers\…` 说成"物理上位于仓库之外"是不准确的：本轮两处
   都记，仓内那份只证明"不在 workspace 成员集内"，仓外那份才证明"不在仓库目录内"。

## 5. 负例

15 个负例（11 个 verify 类 + 1 个 compare 类 + 3 个 guard 类）全部 **OK**：期望检查 ID 与
实际检查 ID 相同，无一例 `NOT REFUSED` / `WRONG REASON` / `INJECTION DID NOT LAND`。
每个负例的干净副本来源、注入实际改了哪些字节（sha256 前后值）、两条命令与退出码，见
`negatives/<case>/case.md`；对照表在 `F1-F5-closure.md`。

编排助手自身的 9 条回归（`script-regression/script-regression-log.md`）里有三条是**自测**：
期望错检查 ID 必须判 `WRONG REASON`、注入未改字节必须抛错、基线不绿必须判 `BROKEN`。
这三条若哪天静默通过，说明负例又变成装饰品。

## 6. 与 `docs/reviews/2026-09-29-wave5a-gates/` 的关系

旧目录原样保留，不改动、不删除。本目录**替代**其中以下结论：

| 旧结论 | 现状 |
|---|---|
| 「`source_revision` 已钉住来源」 | 当时从不解析该提交；现在它必须真的是提交对象且真的包含被打进工件的输入（F1） |
| 「`compare` 证明重复构建一致」 | 当时只比两条记录字符串；现在两侧 payload 各自重算后才比（F2） |
| 「4 类篡改 × 2 试点全部被具名检查拒绝」 | 其中「错误 module_id 被相应检查拒绝」不成立（拒绝来自 `[bundle-identity]`，且是上一例的残留）；更正见 `red-run/F4-F5-from-retained-history.md`（F3/F5） |
| 「消费者依赖树中无回指仓库的 path」 | 当时是两次文本扫描 `hits : 0`；现改由完整解析图判定（F4） |
| 「70 ignored 全部是既有 PostgreSQL lane」 | 已按测试清单重新分类（见 `ignored-tests-classified.md`） |
| 「packed crate 构建 + 消费者构建证明了工件被真实构建」 | 仍成立，但补充：`cargo build` / `cargo test` 确实执行过工件（旧记录未明说），且新增一次真正位于仓库目录之外的复建 |
| 「门禁：833 passed / 187 suites」 | 属 `8036bc65` 的历史事实，保留；本候选为 855 passed / 190 suites |

## 7. NOT RUN / BLOCKED（不得当作通过）

| 项 | 状态 | 原因 |
|---|---|---|
| PostgreSQL 迁移执行、70 个 `#[ignore]` 用例 | **BLOCKED** | 本轮无数据库授权（承前 G-02）；SQLite 与离线证据不得替代 PostgreSQL 结论 |
| 单模块在 PostgreSQL 上独立安装 / 升级 / 回滚 | **BLOCKED** | 共享 `019_identity_authorization_foundation.sql`、同 `public` schema、`runtime-migration` 全量 embed（G-203）；工件内该条 `module_owned=false` |
| publish / 注册 / install / upgrade / rollback 执行 | **NOT RUN** | 任务边界禁止；本轮只做本地候选工件与验证 |
| push / merge / 正式发布 tag / 部署 | **NOT RUN** | 同上；两个试点成熟度未改 |
| 漏洞、许可证、secret、镜像扫描；CI | **NOT RUN** | 未执行 |
| 「哪个工具链进程真正产出了 packed 字节」 | **NOT PROVEN（工具侧）** | `toolchain-declaration` 只比对身份声明与该提交里的 `rust-toolchain.toml`；本次运行所用工具链由 `00-toolchain.txt` 的 `rustc -vV` 作为外部证据 |

## 8. 独立审阅栏

| 项 | 结论 |
|---|---|
| 独立 reviewer | `<pending>` |
| Reviewed SHA | `<pending>`（实现者交付的代码候选 `69d9a6b01184db7d9f3d4aa3b688fb569387b2fc`） |
| F1 来源绑定是否成立 | `<pending>` |
| F2 compare 是否成立 | `<pending>` |
| F3 记录一致性是否成立 | `<pending>` |
| F4 隔离判定是否成立 | `<pending>` |
| F5 负例是否有效 | `<pending>` |
| G-01 / G-02 / G-201 / G-202 / G-203 可否关闭 | `<pending>`（实现者不判定，本轮一律未关闭） |
| R2 | **NOT CLAIMED**（实现者不判定） |
