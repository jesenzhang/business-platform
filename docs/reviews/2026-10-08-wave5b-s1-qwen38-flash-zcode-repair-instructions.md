# Wave 5B-S1：qwen3.8 flash / zcode 修复实施指令

日期：2026-10-08。本文是待执行修复合同，文档编写不代表修复或测试已执行。

## 1. 执行目标、事实基点与权限

你是 zcode 中的 qwen3.8 flash 实施者。在 `F:\Workspace\business-platform` 按本文顺序修复独立复审 R1–R7，补齐真实回归和原始门禁证据，交付新的固定本地候选 SHA 后停止，等待独立复审。

当前结论是 **NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**。这里的“R2 BLOCKED”是原计划 R2 readiness 状态，和本文缺陷编号 R2 不是同一含义。修完 SQL 缺陷 R2 不会自动解除计划 R2 readiness 阻断。

编写本文时核实：

- HEAD：`87e8a318fa32ff43d41ad45da8a0f83b3b19a2a4`。
- 分支：`codex/plan-0014-wave5a-local-release-pilot`。
- 修复实现是以下四个文件的未提交工作区增量，不能只审查 HEAD：
  - `tools/module-release-tool/src/catalog.rs`
  - `scripts/test-plan0014-wave5b.ps1`
  - `tools/module-release-tool/scripts/run-wave5b-gates.ps1`
  - `tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1`
- 最近复审发现 2 项 P1、5 项 P2。原 B5/B6/B7/B8 仅在复审说明的有限离线或 stub 范围内通过；保留这些修复，再做真实入口回归。
- 原有 19 个 focused tests 通过，但未覆盖新反例。本轮 full workspace 与真实全案例驱动仍 NOT RUN。

执行前重新记录 HEAD、分支、status 和四个文件的 SHA-256。如果已变化，先对账变化与当前修复合同；保留其他作者修改，不 reset、clean、stash 或强制切分支。不允许共享 checkout 并行写入，不启动子 Agent；各修复和 cargo 门禁顺序执行。

本任务是离线 tooling 修复，Platform Foundation 为工具所有者；不改变任何业务事实所有权，不执行本文 SQL，不连接数据库，不引入迁移执行器、API、Worker、Wren runtime 或新依赖。不得修改历史基线、接受 ADR、推进 S2、宣称 R2 readiness、push、merge、部署或发布。本地候选提交只用于独立复审。

## 2. 必须读取的权威输入

先读取仓库 `AGENTS.md` 和 `C:\Users\admin\.codex\RTK.md`。命令使用 RTK；多行内容用 UTF-8 文件或单引号 PowerShell here-string，SQL/manifest fixture 明确按字节写入 LF。

依次读取：

1. `docs/reviews/2026-10-08-wave5b-s1-repair-independent-review.md`：当前 R1–R7 的权威问题清单。
2. `docs/reviews/2026-10-08-wave5b-s1-independent-review.md`：B1–B8 原始安全与兼容边界。
3. `docs/reviews/2026-09-30-wave5b-revision1-independent-review-and-s1-instructions.md`，尤其 §3.5–§3.10。
4. `docs/plans/current/PLAN-0014-wave5b-migration-isolation-design.md` 和 `PLAN-0014-reference-conformance-and-r2-module-readiness.md` 的 S1 范围。
5. `tools/module-release-tool/Cargo.toml`、`src/lib.rs`、`src/catalog.rs`、`tests/wave5b_catalog.rs`、fixture EXPECTATIONS 与对应生成器。
6. 架构 Manifest、SERVER_BACKEND_ARCHITECTURE、BOUNDED_CONTEXT_MAP、DATA_OWNERSHIP_AND_CONSISTENCY、SECURITY_ARCHITECTURE、OBSERVABILITY_ARCHITECTURE、CODE_ARCHITECTURE；API_AND_EVENT_CONTRACT_STANDARD、ARCHITECTURE_FITNESS_FUNCTIONS、RUST_CODING_STANDARD；按静态 SQL/migration 检查范围读取持久化与数据库适配专题。

`2026-10-08-wave5b-s1-fixes.md` 是实现者陈述，不能覆盖独立复审。旧日志不能作为本次执行结果。

## 3. 精确文件范围与兼容约束

允许修改：

| 文件 | 责任 |
|---|---|
| `tools/module-release-tool/src/catalog.rs` | reserved 名称、有限 SQL 对象解析、embed 静态扫描与模块内单元测试 |
| `tools/module-release-tool/tests/wave5b_catalog.rs` | 读取共享 fixture 的回归与 pin 对账集成测试 |
| `tools/module-release-tool/tests/fixtures/wave5b/catalogs/**` | 新增正负案例及 EXPECTATIONS；不改旧案例预期来消除失败 |
| `tools/module-release-tool/scripts/generate-wave5b-fixtures.ps1` | 同步新 fixture 的生成来源；先确认实际存在及生成约定 |
| `scripts/test-plan0014-wave5b.ps1` | 精确参数绑定、run 根登记、保护和 finally 清理 |
| `tools/module-release-tool/scripts/run-wave5b-gates.ps1` | post 参数组装；保持失败传播、唯一证据和旧日志保护 |
| `tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1` | 保留原有效修复；必要时纠正终端 forms 统计，不能换 pin 或重写历史 fixture |
| 新增 `scripts/test-plan0014-wave5b-repairs.ps1` | PowerShell 入口、清理、wrapper 的有界回归驱动 |
| 本轮新修复报告与唯一证据目录 | R1–R7 状态、原始日志、diff、文件摘要、候选定位 |

仅在现有文档结构确有对应脚本入口时更新该入口文档。默认不拆大模块，不新建通用 SQL/Rust parser；如测试必须共用 helper，只允许小型私有脚本 helper，并在报告说明路径与必要性，不复制一份生产逻辑到测试。

禁止修改或执行 `docs/OPERATIONAL_RUNBOOK.md`、`scripts/clean-target.ps1`，它们是其他作者的未跟踪工作。旧独立报告与证据目录保持字节不变。禁止修改 pinned baseline 的 19 个 SQL 和 manifest、既有 baseline history fixture、RuntimeMigration、正式空 catalog、业务 crate 和 apps。

保持：固定历史 pin `5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`；C1 历史 LF 容错；新 manifest LF-only；IAM 既有有限可写对象白名单；SQLite 自有迁移目录正例；既有 CLI、JSON 输出契约、release generate/verify 及四类 digest 语义。新增诊断优先复用已有错误结构，错误必须说明不支持的语法或对象归属，不泄漏内容/凭证。

## 4. 给 flash 模型的执行纪律

一次处理一个小阶段：读目标函数 → 写失败回归 → 运行并保存失败原因 → 最小修复 → 跑本阶段正负对照 → 检查 diff。不要一次重写 catalog.rs。

回归测试必须先证明输入走到目标判据。负例因 CRLF manifest、工具不存在、PowerShell 参数歧义而提前失败，不能算 SQL/路径规则通过。新 LF fixture 使用 `write_bytes` 或等价显式 LF 字节 API，保留首次失败记录，不覆盖日志。

先完成 Rust reserved/SQL/embed，再完成 PowerShell 路径/参数，最后整体验证。每阶段检查未触及禁止范围。失败时保留原始输出并定位，不删测试、不放宽 expected、不跳过关键检查；环境障碍写 BLOCKED 或 NOT RUN 和具体原因。

## 5. 阶段 A：R1，完整保护既有 baseline 对象

### 方法

1. 检查 `BASELINE_RESERVED_TABLE_NAMES` 及 owned 声明接纳路径。
2. 至少补齐 019 的四个 RENAME 目标：`v0_demo_role_permissions`、`v0_demo_user_roles`、`v0_demo_roles`、`v0_demo_permissions`。
3. 保留 CREATE 原名、RENAME 目标及平台 ledger 名称的保守并集；历史旧名也不开放给新模块认领。不得只扫描当前工作区或只看 CREATE。
4. 补上真正的 `baseline_reserved_tables_are_reconciled_with_pinned_sql` 测试。用固定 SHA 的 Git blob 逐文件对账 CREATE 与 RENAME 目标、平台 ledger，并证明都进入 reserved 集合。不得用被测集合自身生成 expected 后再比较自身。测试遇到缺失 pin 或无法解释的相关 DDL 必须失败，不静默跳过。
5. reserved 不等于全部不可由既有合法 owner 使用；保持原 IAM 狭窄例外，不通过扩大白名单处理失败。

### 必须回归

以下 policy catalog 原来错误放行，现在应具名拒绝且 exit 非零：

```sql
-- @bp-owned: v0_demo_roles
CREATE TABLE IF NOT EXISTS v0_demo_roles (id bigint);
DELETE FROM v0_demo_roles;
```

四个改名目标逐一覆盖；`documents`、`_sqlx_migrations`、`platform_module_migrations` 保持拒绝。新增真正自有对象及原 IAM 合法对象仍通过。`v0_demo_users` 不是已验证的 019 改名目标，不把它用作该归属证明。

## 6. 阶段 B：R2 与 R6，按语句语境解析 SQL 对象

### 方法

1. 先划分语句语境，再提取对象：DELETE USING 是表列表，CREATE INDEX USING 是 access method，JOIN USING 是列列表。不能全局看到 USING 就认领下一个 token。
2. 对支持的 FROM/DELETE USING 表列表采用显式游标：读取表名 → 可选 bare 或 AS alias → 逗号后读取下一表；遍历到明确结束符，不能遇别名直接 break。
3. 现有 JOIN 引用继续逐个检查。限定名、引号标识符、子查询等只能按既有契约准确处理；没有完整支持时返回具名 unsupported，不能解析半句后报告 VALIDATED。
4. 同一表列表中的每个引用都必须进行 owner 检查，覆盖 foreign 在首、中、末位置。
5. CREATE INDEX 的 ON 表执行 owner 校验；至少 `USING btree` 合法通过。method 不作为表，未知 method 依受支持范围具名拒绝。
6. INHERITS 的 `(` 是 `Token::Symbol`，不能再用 Word keyword 检查。最小安全实现可明确拒绝 INHERITS 全语句并报告 unsupported；若支持则逐父表完整检查。不得把旧反例的偶然解析失败说成正确归属检查。
7. ATTACH PARTITION 等原负例保持 fail-closed。避免引入通用 SQL 引擎、执行 SQL 或依赖外部服务。

### 必须共享 fixture

下列各自拒绝，诊断确认指向外部对象归属：

```sql
INSERT INTO roles SELECT r.* FROM roles r, platform_users p;
DELETE FROM roles USING roles r, platform_users p;
```

扩展 AS alias、首/中/末 foreign、逗号后的限定名；对照同 owner 的带 alias 多表引用通过。以下必须通过：

```sql
CREATE INDEX wave5b_ix ON roles USING btree (role_id);
```

保留注释/字符串中的 FROM、USING、逗号不被当真实引用的正例；JOIN USING、INHERITS 按选定的具名拒绝或完整支持策略测试。每个 fixture 的 SQL、LF manifest、EXPECTATIONS 和生成来源同步；Rust 与 PowerShell 两个读取者一致。

## 7. 阶段 C：R5，可靠的 embed 扫描

### 方法

1. 不再以 migrate 子串和紧邻字符 `!` 判断调用。读取静态 token，允许路径分隔和 `!`、括号之间的空白/注释。
2. 两遍处理：先收集相关 import/alias，再识别调用；源码中的 import 顺序不能改变结果。
3. 受支持范围覆盖直接 `sqlx::migrate!`、bare import、alias、group import；别名不能只搜索 migrate needle。
4. 若复杂 import、glob、crate alias 或路径无法可靠解析，对可能隐藏 baseline embed 的形式具名拒绝；不得当作“未发现额外 embed”。可保守拒绝 sqlx migrate 导入形式作为最小策略，但必须明确诊断并由原 S1 契约确认允许；不能拒绝既有必需正例。实现报告说明究竟支持哪些形式。
5. 字符串/注释中的伪调用不增加 embed 计数；真实宏路径参数无法静态确定时按契约具名拒绝。保留 SQLite 自有路径正例。

### 必须回归

在原 baseline embed 之外新增第二处以下形式，每个都不得以 baseline_embed_points=1 错误通过：

```rust
use sqlx::migrate;
static M: sqlx::migrate::Migrator = migrate!("../../migrations");

use sqlx::migrate as embed;
static M: sqlx::migrate::Migrator = embed!("../../migrations");

use sqlx::{migrate};
static M: sqlx::migrate::Migrator = migrate!("../../migrations");

static M: sqlx::migrate::Migrator = sqlx :: migrate ! ( "../../migrations" );
```

分别建例，不把四个错误合到一个文件后只证明其中一个被发现。追加 group alias、import 在调用之后、注释间隔、直接调用负例和 SQLite 自有路径正例；已有字符串/注释正例不回退。

## 8. 阶段 D：R3 与 R4，入口与 run 生命周期

### R3

将 `Assert-PathsDoNotOverlap` 调用改为完整 `-Label1`、`-Label2`，检查全部同类调用。用有效 Repo、互不重叠的 Scratch/EvidenceDir/TargetDir 真正启动 driver，证明已越过 preflight 并进入实际案例。非法路径负例的诊断必须是路径规则，不能是参数绑定失败。

### R4

建立明确的本轮 run 根登记，不把用户传入 Scratch 父目录登记成可删目录：

1. preflight 完成后在已验证父目录下创建唯一 run 子目录。记录绝对 run 根、实际验证过的父目录、run identity/归属；fallback 同样登记自己的验证父目录。
2. 清理允许删除已登记的 run 根自身及其本轮内容。不能用“必须位于主 run 根的严格子目录”排除主根或 fallback。
3. 删除前重新确认目标是本轮登记的准确根、在登记父目录内、不是 repo/evidence/受保护路径或祖先；Windows 比较忽略大小写且有分隔符边界。处理 reparse point/junction，无法安全证明时拒绝清理并报告，不能遍历到用户目录。
4. 使用单一 PowerShell 的 `Remove-Item -LiteralPath`；不要跨 shell 拼接删除。不要删除 Scratch 父目录、共享 target 或任意旧目录。Force 不能取消保护。
5. 正常和异常退出均走 finally；fallback 创建后发生异常也必须清理。KeepScratch 显式保留本轮 run 根，记录位置。
6. 默认内部 evidence 模式必须给证据明确生命周期：把证据放在可保留目录，或只保留证据子树并清理其他临时内容；不能因清理 bug 偶然留下整个 run。外部 evidence 永不随 scratch 删除。

### PowerShell 必须回归

新增 repair driver 测试实际生产函数和真实入口；若 AST 提取函数进行隔离测试，执行的必须是当前生产 AST，不能复制实现，且隔离测试不能替代真实入口。

- 主 run 正常退出、异常退出、fallback 正常/异常：清理行为正确。
- Scratch 父目录已有哨兵和旧 run：全部保持字节不变。
- repo、repo 祖先、路径前缀相似但非后代、大小写变体、Scratch/EvidenceDir 重叠：正确拒绝，受保护哨兵存活。
- KeepScratch：保留本轮目录，其他保护仍有效。
- 默认与显式外部 evidence：证据存活，非证据工作目录按约定清理。
- reparse point 无权限创建：记录 BLOCKED/NOT RUN 与原因，不算通过；可创建时证明不会删除外部哨兵。

## 9. 阶段 E：R7 与原 B5–B8 回归

R7 用中间变量组装，避免在数组内做裸路径加法：

```powershell
$caseEvidenceDir = Join-Path $EvidenceDir 'wave5b-cases'
# 将 $caseEvidenceDir 作为 -EvidenceDir 后的唯一元素。
```

stub 必须捕获真实 Arguments 数组，断言唯一完整路径、参数个数及 Repo 绑定；覆盖空格路径，再执行真实 post 入口。不能仅检查渲染后的日志文本或外层 exit。

保留并重新验证：

- B5：新 manifest CRLF 具名拒绝；历史 C1 容错仍通过。
- B6：固定 SHA 生成到新的临时 Out；JSON 结构逐字段对账 20 entries，SQL forms 为 12 LF / 7 CRLF。不要改 fixture 迎合产物；terminal forms 若打印则按真实条目统计。
- B7：正例命令 stub exit 7 导致外层非零；仅预定未知 check exit 1，其余成功时外层成功。记录 expected/actual；不能“任意非零都算负例”。
- B8：每阶段新 EvidenceDir；重复非空目录拒绝，已有文件 SHA-256 不变。fast/heavy/post 分别唯一目录，不能第二阶段覆盖第一阶段。

## 10. 执行命令与证据

先记录基点，使用唯一 RunId，不覆盖任何旧目录。下面命令是实施者待执行命令，本交接没有执行这些修复门禁。

```powershell
Set-Location 'F:\Workspace\business-platform'
rtk proxy git rev-parse HEAD
rtk proxy git branch --show-current
rtk proxy git status --short
rtk proxy git diff --check

$repairRunId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8)
$repairEvidence = Join-Path $PWD ('docs/reviews/wave5b-s1-repair-' + $repairRunId)
$repairTarget = Join-Path $PWD ('target/wave5b-s1-repair-' + $repairRunId)
$env:CARGO_TARGET_DIR = $repairTarget
```

以新 evidence 目录保存逐命令完整 stdout/stderr、开始/结束、精确参数数组、expected/actual exit。原 wrapper 的修复回归前不要用其摘要作为门禁可信来源；先独立执行修复后的 focused tests 与 PS regressions。

```powershell
rtk proxy cargo test -p module-release-tool --test wave5b_catalog --all-features
rtk proxy cargo test -p module-release-tool --all-features
rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5b-repairs.ps1
rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1 -Repo $PWD.Path -EvidenceDir (Join-Path $repairEvidence 'cases') -Scratch (Join-Path $PWD 'target/tmp/plan0014-wave5b') -TargetDir $repairTarget
rtk proxy cargo fmt --all -- --check
rtk proxy cargo check --workspace --all-targets --all-features
rtk proxy cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk proxy cargo test --workspace --all-features
rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1
rtk proxy git diff --check
```

新 repair driver 顶层默认应安全执行本任务离线回归；若增加显式参数，更新以上命令和入口文档。先用 PowerShell parser 检查所有修改脚本无语法错误，随后实际运行，parser 成功不能代替运行。

再用各自新目录真实跑 `run-wave5b-gates.ps1 -Phase fast/heavy/post`，确认 wrapper 自身行为与逐命令证据一致。可以引用同一候选已跑原始日志减少人工重复；不能因此把尚未执行的 wrapper 阶段写 PASS。全 cargo 顺序运行，禁止清 target、杀占锁进程；环境阻断则保存日志，换本轮专用 target 或记录 BLOCKED。

统计实际测试总数与 ignored 明细。原 76-case driver 的案例数只是旧分母；加入共享 fixture 后按当前 EXPECTATIONS 重新对账。记录 MATCH/MISMATCH/BLOCKED/NOT RUN，各项总和覆盖全部枚举案例，MISMATCH 必须为 0。symlink/大小写冲突无法在当前文件系统表达时保留阻断原因。旧 883 passed/70 ignored 等历史数字不能写成新结果；PostgreSQL ignored 不变成实际数据库验证。

按 AGENTS 记录真实 Provider/DB 契约、Secret/漏洞/许可证/镜像扫描的适用性与执行状态；本任务不连接真实服务，未执行项写 NOT RUN 并说明离线范围，不能宣称生产/发布接受。

## 11. 候选提交与最终交付

先检查完整 diff 和禁止文件的摘要未变。按原 S1 本地候选规则显式逐文件 stage，只包含本次授权修复、测试、必要文档；禁止 `git add .`，不吞入其他作者工作。不修改旧独立报告或证据。新提交不 amend 旧候选或历史 baseline。

如果某 packaging 测试需要 committed tree，保留提交前失败原因，在本地候选提交后复跑；只能依据明确 committed-input 差异解释，不允许笼统说“提交后会绿”。报告分别记录代码候选 SHA 与后续证据文档 SHA/HEAD；后续代码变化需要重新固定候选并复跑受影响门禁。

新增当前修复收尾报告，至少包含：

1. 开始 HEAD/branch/dirty 清单；最终固定代码候选 SHA、文件 SHA-256、diff。
2. R1–R7 每项对应函数/行号、修复方法、失败回归、正负对照、原始日志路径及结果。
3. B5–B8 与 CLI/digest/历史 pin 兼容回归。
4. 实际新增 fixture/test 数量；Rust/PowerShell 两个读取者对账；所有 ignored/BLOCKED/NOT RUN。
5. 临时 run 根清理清单、KeepScratch/evidence 行为、保护哨兵摘要。
6. 所有门禁精确命令与 exit；旧历史证据和本轮执行严格区分。
7. 是否触及边界/所有权/契约：本任务应为“否”，若实际发生则停下并记录偏离，不能静默扩大范围。

## 12. 完成定义与停止条件

| 验收项 | 完成判据 |
|---|---|
| R1 | 固定 pin 保留名真实对账；四个 RENAME 目标不能认领；原合法 owner 正例保持 |
| R2 | alias/AS、多表 FROM/USING 中全部引用检查；foreign 任意位置拒绝；同 owner 对照通过 |
| R3 | 显式 EvidenceDir 正常入口进入实际案例，非法路径由正确规则拒绝 |
| R4 | 主/fallback 正常与异常生命周期正确；父目录/旧文件/evidence 保护；KeepScratch 明确 |
| R5 | 每个第二 embed 形式拒绝或正确计数；空白写法无漏扫；SQLite 正例保持 |
| R6 | USING btree 自有 index 通过；语境不误判；unsupported 具名拒绝 |
| R7 | post 捕获参数数组正确，含空格路径；真实 driver 调用成功 |
| 兼容 | B5–B8、固定 pin、LF 容错、CLI/release 输出与 digest 无回退 |
| 工程 | focused/full crate/workspace fmt/check/clippy/test/architecture 原始证据；分母与未运行项诚实 |
| 交付 | 固定本地候选 SHA、当前收尾报告、唯一证据目录；无越界修改 |

全部任务内判据通过后只能写 **READY FOR REVIEW / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**。仍有本任务必需门禁失败或环境阻断，保持 **NEEDS REPAIR / NOT INTEGRATED**，逐项报告原因；不能为了到达 ready 降低验收。

完成上述候选和报告即停止。不启动独立审阅 Agent，不自动集成 main、不继续 S2、不推送、不发布。最终回复给出候选 SHA、R1–R7 状态、关键门禁、剩余阻断和报告路径，供用户交给独立复审。
