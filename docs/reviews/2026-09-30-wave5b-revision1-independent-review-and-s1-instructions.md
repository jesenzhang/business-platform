# PLAN-0014 Wave 5B Revision 1 独立复审与 S1 实施指令

> Date: 2026-09-30  
> Review base: `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`  
> Reviewed HEAD: `6decf1ad9ea05233d66dd3539594ff210afd99f2`  
> Branch: `codex/plan-0014-wave5a-local-release-pilot`  
> 判定：B1/B2/B3 **设计修正通过有界复审**；下一包为 **S1 离线实现候选**，附本报告 C1 的必修兼容规则。  
> ADR-0029 **Proposed / NOT ACCEPTED**；Wave 5A Repair 2 **REVIEWED / NOT INTEGRATED**；R2 **BLOCKED / NOT CLAIMED**。

## 1. 审阅范围与证据

开始时工作树干净。审阅范围 `5c8355d..6decf1a` 是 8 个文档，+1107 / -135；没有产品代码或迁移变更。
使用 open-code-review-delegate 工作流：OCR preview 的 total_files=8、reviewable_files=0、excluded_files=8，
均因 Markdown 扩展名排除。人工补审 8/8，skipped=0，人工覆盖率 100%；OCR 自动覆盖率 N/A。
此覆盖率只针对本次文档增量，不表示全仓代码审阅。

| 文件 | 本次人工审阅结果 |
|---|---|
| `docs/README.md` | 状态追加与历史证据分离，未声称实现或集成 |
| `docs/adr/ADR-0029-module-namespaced-migration-catalogs-and-ledger.md` | 保持 Proposed；锁模型与物理隔离非目标已同步 |
| `docs/architecture/ARCHITECTURE_STATUS.md` | 追加 Revision 1 状态；R2 未升级 |
| `docs/plans/current/PLAN-0014-reference-conformance-and-r2-module-readiness.md` | 修复映射、阶段边界与未运行范围已同步 |
| `docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md` | B1/B2/B3 已修正；S1 历史摘要规则需按 C1 落地 |
| `docs/reviews/2026-09-30-wave5b-design-independent-review.md` | 原审阅归档，没有把旧 NEEDS REPAIR 改成新验收 |
| `docs/reviews/2026-09-30-wave5b-design-revision1-closeout.md` | 实施声明与本次文档增量相符；静态推演没有冒充数据库证据 |
| `docs/reviews/2026-09-30-wave5b-r2-admission-matrix.md` | 子项全部保持 OPEN，原验收分母未降低 |

独立核对了现有 migration embed、两个测试 Cargo 配置、静态门禁实现，以及固定历史提交的 manifest 和 SQL blob。
`git diff --check 5c8355d..HEAD` exit 0。Rust workspace 门禁本轮 **NOT RUN**；数据库、并发、崩溃恢复和发布验收均 **NOT RUN**。

## 2. 复审结论

| 原问题 | 本次结论 | 证据边界 |
|---|---|---|
| B1：共享 ledger bootstrap 与连接生命周期 | 设计修正 PASS：固定公共事务锁与 namespace 事务锁分开；统一锁序；同事务连接执行；失败回滚；只读路径无 DDL | 实现及 V8a/V8b/V8c 尚无执行证据 |
| B2：搬迁时间线与独立冷安装 | 设计修正 PASS：方案 B 撤出可执行步骤；物理搬迁与冷安装分别记账；未决项显式 OPEN | 不能据此启动 schema 搬迁或声称单模块安装 |
| B3：互斥 URL 与数据库测试范围 | 设计修正 PASS：互斥环境作用域；逐 suite 记录 sibling DB / 回落 / 清理问题；增量计数 | 本次没有执行这些命令，也没有获得数据库操作授权 |

### C1 [P2] S1 历史摘要规格不能直接照抄实现

位置：设计包 §11.3，第 808–810 行。`git rev-parse <sha>:<path>` 得到的是 Git object ID，
并非内容 SHA-256；只检查固定提交自身也不能发现当前工作树漂移。
更具体地，独立读取 `5c8355d` 的 19 个 SQL blob 与 manifest 后：**12 项匹配 blob/LF 摘要，7 项只匹配 CRLF 摘要**。
七项是 `002_document_metadata.sql`、`003_outbox_reliability.sql`、`004_outbox_state_reconciliation.sql`、
`005_document_idempotency.sql`、`006_inbox_idempotency.sql`、`007_document_integrity_constraints.sql`、
`008_document_query_indexes.sql`。现有 `Assert-MigrationManifest` 已兼容 raw/LF/CRLF。

影响：若 S1 要求这 19 条历史 manifest 都等于 blob SHA-256，未经修改的合法历史也会失败；
若为变绿重写旧 manifest，则违反历史保护。**本问题随 S1 一并修正，不需要再单独跑一轮设计修订。**
按下文同时实现“固定历史内容保护”和“历史 manifest 兼容”，新 namespace catalog 则只采用规范 LF 摘要。

### 实施时必须消除的两处说明歧义

- S1 文件清单没有列出第二读取者，但 §11.1 要求 release tool 参与共同 schema 测试。下文补齐其离线 validator 与测试范围。
- `audit-postgres` 目前没有 `runtime-migration` dev-dependency；只换测试调用会编译失败。下文补齐，`messaging` 已有该依赖。

以上允许进入 S1 离线实现候选，不代表 ADR 已接受、G-203 子项关闭或 S2 已放行。

## 3. 可直接交给 zcode 的唯一下一实施包

### 3.1 任务与固定输入

实施 **PLAN-0014 Wave 5B-S1：离线 catalog 验证、静态 fitness 与单一 baseline embed 修复**。
起点必须为 `6decf1ad9ea05233d66dd3539594ff210afd99f2`，继续当前分支。
先检查 HEAD、分支和工作树；如已推进，记录实际 SHA 并检查新增变化，不 reset、不覆盖他人修改。
存在无关 dirty 文件时保留并记录；若与许可范围重叠而无法安全区分，停止相应修改并报告。

固定历史基准是 `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`，不是动态 HEAD、main 或最新标签。
权威输入：AGENTS.md、适用 Accepted ADR、架构 Manifest、服务端/上下文/数据所有权/安全/持久化/代码架构 Baseline、
API/Event、Rust 与 Fitness 标准；随后阅读 PLAN-0014、Wave5B 设计 §3/§5.2/§8/§11，以及本报告 C1。
修改前读取目标 Cargo.toml、lib.rs 和现有 `canonical.rs`、`migrations.rs`、静态门禁。

本包是可撤回的离线实现候选。**不进行生产 catalog 激活，也不把 ADR 的 baseline 全局冻结决策提前落为默认全仓政策。**
既有 19 个 SQL 的不可变检查可以落地；“平面目录永远只能有这 19 个文件”的完整候选规则由 S1 独立测试入口显式执行，
正式全局冻结仍等待 ADR-0029 的处理结果。新增文件 `migrations/modules/` 不应被误判成 baseline SQL。

### 3.2 许可文件范围

| 范围 | 精确职责 |
|---|---|
| `migrations/modules/identity-management/catalog.json`、`MANIFEST.sha256` | 新增 RESERVED 元数据与空 manifest，无 SQL |
| `migrations/modules/policy/catalog.json`、`MANIFEST.sha256` | 同上 |
| `tools/module-release-tool/src/catalog.rs`（新增） | 离线严格 catalog validator，确定性结果与具名错误；复用现有 serde/摘要依赖 |
| `tools/module-release-tool/src/lib.rs` | 导出离线 validator；保持所有既有 release schema 常量和行为 |
| `tools/module-release-tool/src/main.rs` | 新增工具命令 `catalog-check --repo <root>`，只读文件；不得接入 generate/verify 的既有工件流程 |
| `tools/module-release-tool/tests/wave5b_catalog.rs`、`tests/fixtures/wave5b/`（新增） | Rust 及脚本共用的正负 fixture；历史基准摘要表也放此受版本控制范围 |
| `scripts/check-architecture.ps1` | 接入离线 catalog validator；扩展 baseline embed 扫描；已有门禁继续执行 |
| `scripts/test-plan0014-wave5b.ps1`（新增） | 临时副本驱动正负例、工作树漂移与固定历史检查；完整候选冻结检查在此显式执行 |
| `crates/messaging/tests/outbox_reliability.rs` | 复用 `runtime_migration::MIGRATOR`，保留原测试语义与 ignored 属性 |
| `crates/audit-postgres/tests/chain_pg.rs`、`crates/audit-postgres/Cargo.toml` | 同上；新增 `[dev-dependencies] runtime-migration = { workspace = true }` |
| 当前设计、PLAN-0014、状态页、docs/README、准入矩阵 | 仅同步本包解释与实际证据；修正 C1 及文件清单；历史记录不改 |
| `docs/reviews/2026-09-30-wave5b-s1-closeout.md`、`docs/reviews/2026-09-30-wave5b-s1-gates/`（新增） | 实施报告、失败/成功日志、fixture 对账与未运行项 |

不新增 crate、服务、runtime feature 或外部依赖。若 Cargo.lock 必须更新，仅允许由上述开发依赖产生的最小变更并解释。
不修改历史 SQL / 历史 MANIFEST、`runtime-migration/src/lib.rs`、启动迁移、迁移 CLI、release spec/mapping、四类摘要公式或既有 Wave5A 工件。
本包 `catalog-check` 是 release **工具的离线检查入口**，不是 S3 的 migration CLI `--namespace` / `verify`。

### 3.3 实施顺序

1. 记录起点、工作树、许可文件和架构预检；建立基于固定 Git blob 的历史摘要表。
2. 先写负 fixture，使未实现 validator、错误 hash、重复 embed 等场景呈红；记录具名失败与退出码。
3. 写单一 Rust validator 与 `catalog-check` 入口；PowerShell 通过该入口验证同一 fixture，避免两套 schema 规则漂移。
4. 新增两个 RESERVED catalog，扩展静态 embed 扫描，修两个测试调用及 audit 开发依赖。
5. 跑正负回归、focused gates、完整 workspace 与架构门禁，逐项归档；失败不得通过放宽规则消除。
6. 同步文档、生成 closeout；以固定实现候选 SHA 交付复审。到 S1 为止。

### 3.4 元数据与路径合同

两个正式 catalog 的内容分别如下（第二份只将 namespace、owner 改为 `policy`）：

```json
{
  "schema_version": "plan-0014.wave5b.catalog.v1",
  "namespace": "identity-management",
  "owner": "identity-management",
  "ledger": "platform_module_migrations",
  "sql_schema": "public",
  "depends_on": ["platform-baseline@19"]
}
```

严格拒绝缺字段、未知字段、重复 JSON 字段、错误类型、未知 schema 版本；namespace/owner/目录名必须一致且属于权威 context。
未知 context、未知 namespace dependency、非正/不可用 baseline revision、重复依赖、自依赖与环拒绝。
本包正式依赖仅用 `platform-baseline@19`；其他 namespace 依赖的实际字符串格式应在 validator、fixture、设计 §5.2 同步明确，不能自行宽松猜测。
输出稳定排序，不随枚举顺序变化。

读取前检查路径与 reparse point：根、namespace 目录及 catalog/manifest/SQL 都不得借 symlink/junction 逃逸。
manifest 条目只能是 basename，严格 `NNN_名字.sql`，禁止绝对路径、分隔符、`..`、大小写重复和重复条目。
不要“先跟随链接读取、再检查”。Windows reparse 测试无法执行时记 NOT RUN，并报告该覆盖缺口。
空 manifest 必须实际 0 字节；正式目录无 SQL。输出只能 `RESERVED`，不得 `INSTALLED`。
非空 fixture 成功也只能 `VALIDATED`，离线检查不产生安装事实。

### 3.5 SQL、manifest 和 owner 静态合同

- SQL 与 manifest 双向集合一致；摘要严格 lowercase SHA-256；重复 revision、从 001 开始的缺口、幽灵条目、遗漏文件均拒绝。
- 新 namespace 的 SQL 统一按 `canonical.rs` 的 CRLF/CR → LF 折叠重算摘要；仅该规范摘要合法。不得继承旧 baseline 的多口径兼容。
- 将已按 owner 分类的 host 表形成显式白名单，注明权威来源；不得把其他 owner 的表或平台 ledger 加入。
- `-- @bp-owned: <table_name>` 只能声明当前 revision 实际创建的新表；声明和创建一一核对；本 revision 后续语句可引用该新表，后续 revision 可复用。
- 声明但未创建、创建但未声明、抢占其他 owner、引用未知/其他 owner 表必须拒绝。多 namespace 新表同名冲突也必须拒绝。
- owner 扫描明确是文本静态 fitness。识别注释、字符串、限定名与表引用位置，避免注释导致误放行/误拒；
  对动态 SQL、无法可靠解析的对象引用或未支持语法给出具名“不支持”错误，不能返回成功。正例只需覆盖 S1 fixture 的明确 SQL 子集。
- 至少提供空 catalog 与一条真正含 `CREATE TABLE wave5b_fixture_...` 的非空 SQL fixture；本轮只读并校验，不执行 SQL。
- 不创建数据库，不添加 runtime fixture 注入或 S2 build.rs。S1 不比较尚不存在的 namespace release spec；只验证摘要规范与集合输入，为 S4 留测试。

### 3.6 历史保护——C1 的必须实现规则

1. 固定基准为上述完整 `5c8355d...`。用 binary-safe subprocess 读取 `git show <fixed>:<path>` / `git cat-file blob <fixed>:<path>` 字节，
   对字节计算 SHA-256；不能把 `git rev-parse` 的 OID 当 SHA-256，不能经 PowerShell 文本管道改变换行。
2. 固化原 19 个 SQL 的 basename 和 blob SHA-256，以及旧 MANIFEST 的 blob SHA-256。摘要表必须从固定提交产生，
   运行时不允许用当前文件自动重生成“期望值”。缺少固定提交时明确 BLOCKED，不回退到 HEAD。
3. 同时比较当前 HEAD/index/工作树中这些历史路径：HEAD/index 以 blob 原始字节校验；工作树允许 checkout EOL 差异，
   对当前和固定 SQL 都 LF 折叠后比较。历史 manifest 工作树也只允许换行表示变化，其他字节变化拒绝。
   工作树新增/删除/改写历史 SQL、改 SQL 并同步改 manifest、只改 manifest 都应失败。
4. 对历史 manifest 的一致性检查保留现有 raw/LF/CRLF 兼容，并将 7 条 CRLF 历史条目作正例。
   因为历史文件和 manifest 另有固定摘要保护，此兼容不能用来接受内容变化。禁止将旧 manifest 重写成 LF 摘要。
5. 完整候选冻结 fixture 校验恰好 19 个**顶层** SQL，允许 `migrations/modules/`；不递归把模块 SQL 算成 baseline。
   默认门禁对既有历史不可变与正式 catalog 校验生效；ADR 未接受前，全局拒绝未来新增 baseline SQL 的政策仅保留在显式候选检查中。
6. 日志打印 pinned SHA、blob/working-tree 模式与命中路径，不打印连接串或环境变量值。

### 3.7 baseline 单一 embed 点

扫描 `apps/**`、`crates/**` 的 Rust 源码（包括 tests），允许 baseline catalog embed 的唯一位置是
`crates/runtime-migration/src/lib.rs`。检测宏空白/换行及能静态解析的路径等价写法，不能只搜索一个固定字符串。
SQLite crate 自有 `./migrations` 必须继续合法：按 source 所属目录解析路径，不能把所有 `sqlx::migrate!` 都禁掉。
无法确定是否指向 shared baseline 的新写法不得静默跳过；给出可处理的失败信息。
两个 PG 测试改用已有 MIGRATOR，其连接、断言与 ignored 属性保持原义；本轮不执行 ignored 测试。

### 3.8 必须保留的正负回归

| 组 | 最低覆盖 |
|---|---|
| schema | 两正式 RESERVED 正例；未知/缺失/重复字段、类型、版本、owner 不一致反例 |
| path | `..`、绝对路径、大小写冲突、文件与目录链接/reparse、路径外目标反例 |
| manifest | 双向遗漏、幽灵、重复条目、重复 revision、缺口、错误摘要、LF/CRLF 新 SQL 正例、非规范摘要反例 |
| dependency | 未知依赖、错误 baseline、重复、自依赖、环；稳定拓扑正例 |
| owner | 合法新表及下一 revision 引用正例；他人表、未声明新表、虚假声明、动态/不支持语法反例 |
| history | 原 19 + 旧 manifest 正例；7 条历史 CRLF 摘要正例；SQL 与 manifest 同改、仅改 manifest、staged/dirty 漂移、缺固定提交反例 |
| embed | 当前两处违规红例；修后绿；SQLite 自有目录正例；额外 shared embed 与路径等价变体反例 |
| status | 空只 RESERVED、非空只 VALIDATED；错误不能打印成功状态 |

所有破坏性 fixture 都在独立临时副本中，不能修改仓库实际 catalog/历史文件。
临时副本含独立 Git 历史供 HEAD/index/dirty 场景验证；不要触碰本仓库 index。
脚本以 case ID、期望/实际退出码、具名错误输出对账；负例必须真的失败，全部符合预期时脚本才 exit 0。
cleanup 使用 finally 并验证绝对临时目标属于本轮目录；不得删除证据日志或外部路径。

### 3.9 命令与证据

以下 shell 命令按本机规则使用 RTK。逐条记录 exit code、时间与日志路径；原生进程失败必须显式传播。

```powershell
rtk proxy cargo test -p module-release-tool --all-features
rtk proxy cargo run -p module-release-tool -- catalog-check --repo F:/Workspace/business-platform
rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1
rtk proxy cargo test -p messaging -p audit-postgres --all-features
rtk proxy cargo fmt --all -- --check
rtk proxy cargo check --workspace --all-targets --all-features
rtk proxy cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk proxy cargo test --workspace --all-features
rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1
rtk proxy git diff --check
```

不加 `--include-ignored`，不运行 migration app，也不读取/设置数据库 URL。
构建目录若因已有进程文件占用失败，保存原失败日志，使用本包专用 target-dir 重跑受影响 Rust 命令，
记录实际路径和参数；不杀其他进程。环境或网络阻塞记 BLOCKED / NOT RUN，不能借用旧绿色数字。
安全/漏洞/许可证等适用仓库门禁如未执行，逐项记录 NOT RUN 及原因。

`864/191/70` 是历史对账基准，不是本轮硬编码目标。新增测试按名称、suite 与 ignored 状态对账；
数量减少须解释到具体测试，不能把新增数当成独立验收数。既有 release tool 测试继续通过，不重组或覆盖历史 Wave5A 工件。

### 3.10 交付与停止条件

closeout 必须含：起止 SHA、逐文件职责、C1 修复与摘要口径、正负例对账、实际门禁结果、所有 NOT RUN/BLOCKED、
固定历史未变证明、两正式 catalog 的 RESERVED 输出，以及回滚方法（撤回本包新增 validator/脚本/元数据与测试调用变更，无数据库回滚）。
同步状态只能写 **WAVE 5B-S1 READY FOR INDEPENDENT REVIEW / NOT INTEGRATED；R2 BLOCKED**。
ADR 保持 Proposed；G-203a-i 还缺 S4 工件钉定，G-203/G-202/A-12、其他既有 gap 全部保持原状态。

实现与门禁完成后提交一个范围清楚的本地候选，报告候选 SHA；证据文档如另行提交，区分实现 SHA 与文档 HEAD。
可归档本报告但不得改写审阅内容。工作树干净或明确列出保留的无关 dirty 工作。
**到此停止，等待固定候选独立复审；不自动进入 S2/S3/S4/S5，不改 main、不 push/merge/tag/publish/deploy，
不启动 PLAN-0006，不接受 ADR，不执行 schema 搬迁或数据库操作。**
