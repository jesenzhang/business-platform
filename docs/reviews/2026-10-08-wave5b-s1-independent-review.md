# PLAN-0014 Wave 5B-S1 独立收尾复审

> 日期：2026-10-08（Asia/Shanghai）
> 结论：**NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**
> 实现候选：`b25e2d4a9b455b776934495b8f3dfd2b71a13fe2`
> 复审 HEAD：`87e8a318fa32ff43d41ad45da8a0f83b3b19a2a4`
> 变更起点：`6decf1ad9ea05233d66dd3539594ff210afd99f2`
> 历史基线：`5c8355dceba9cf062a63fa6a2b5c6ef1ff41b601`
> 分支：`codex/plan-0014-wave5a-local-release-pilot`

## 1. 范围与方法

进入复审时工作树干净。审查实现候选及其后 `df0d31a`、`87e8a31` 两次文档/辅助工具提交；这两个提交增加了可执行工具，因此复审范围不限于 b25e2d4。权威执行契约为 [Revision 1 独立审阅及 S1 指令](2026-09-30-wave5b-revision1-independent-review-and-s1-instructions.md) §2/§3；并核对 Wave5B 设计 §11、架构 Manifest、数据所有权/安全/上下文/代码架构和 Rust 标准。

使用 open-code-review-delegate（OCR 仅作文件选择与规则解析，不调用外部 LLM），遵循本仓 AGENTS 由主线程顺序审阅。OCR 统计：total_files=212、reviewable_files=16、excluded_files=196；16 个 reviewable 文件均已审阅，skipped_files=0，reviewable coverage=100%。**该比例只针对 OCR 的 16 个文件，不表示 212 个文件或整仓人工全覆盖。** excluded 文件中的规格、closeout、状态增量、共享期望与历史摘要、归档日志和 CSV 按验收范围补充核对；fixture 由既有 Rust 读取者执行，其他历史日志仅核对结果，不冒充本轮执行。

清单和规则见 [ocr-preview.json](2026-10-08-wave5b-s1-independent-gates/ocr-preview.json)、[ocr-rules.json](2026-10-08-wave5b-s1-independent-gates/ocr-rules.json)、[reviewable-files.csv](2026-10-08-wave5b-s1-independent-gates/reviewable-files.csv)。复现仅在 `target/wave5b-independent-20261008/` 内构造文件，SQL 只作文本输入，未执行任何 SQL。

## 2. 必须修复的发现

### B1 [P1] 任意 Scratch 路径在验证前被递归删除

位置：`scripts/test-plan0014-wave5b.ps1:121-123,262,265-281,709`。

`-Scratch` 直接传给 `New-FreshDirectory`，后者先 `Remove-Tree`；没有验证规范绝对路径、是否为 repo/祖先/证据目录、是否属于本次运行。启动时还先删除 scratch，后检查 EvidenceDir。传错 `-Scratch` 为仓库或已有工作目录会删除其内容；不需要 `-Force`，`-KeepScratch` 也不保护启动删除。固定 OS temp 名称 `plan0014-wave5b-links` 也会清空其他运行的目录。

独立哨兵复现：在本轮 target 目录预置无关哨兵文件，指定为 Scratch，再指定一个已有证据目录。脚本 exit 1 拒绝证据覆盖，但 scratch 哨兵已被删除，证据哨兵仍在。见 [scratch-repro.json](2026-10-08-wave5b-s1-independent-gates/scratch-repro.json)。未在真实仓库根执行危险变体。

违反指令 §3.8：cleanup 必须 finally 且验证绝对目标属于本轮目录，不删除证据或外部路径。修复应在任何删除前验证所有路径关系，创建本轮唯一子目录并只清理该子目录；拒绝 repo、其祖先、证据目录及重叠路径。异常退出用 try/finally 处理所有本轮 scratch（含 link fallback）。

### B2 [P1] 未列入 IAM 白名单的既有 baseline 表可被声明为新表

位置：`tools/module-release-tool/src/catalog.rs:1901-1920`。

`Create` 只拒绝 13 个 `HOST_BASELINE_TABLE_OWNERSHIP` 名称和 `platform_module_migrations`。其他 baseline 对象（如 Document Management 的 `documents`）被视为可声明的新表，随后加入当前 owner 集合。以下 policy catalog 被独立放行（exit 0 / VALIDATED）：

```sql
-- @bp-owned: documents
CREATE TABLE IF NOT EXISTS documents (id bigint);
DELETE FROM documents;
```

`IF NOT EXISTS` 在已有 baseline 上不会创建新表，却能让 scanner 认领它，后面的跨上下文删除也因此放行。`_sqlx_migrations` 同样可声明创建。见 [takeover-repro.json](2026-10-08-wave5b-s1-independent-gates/takeover-repro.json) 和 [repro-results.json](2026-10-08-wave5b-s1-independent-gates/repro-results.json)。这证明静态门禁缺陷，不声称已在数据库删除数据。

违反 §3.5 的“实际新表、一一核对、不能抢占其他 owner”，以及单一数据所有者规则。修复须区别可操作的 owner 白名单与完整的 baseline/平台保留对象集合；所有已存在的非自有名称不能通过新表声明获得权限。保留集合来源必须钉住且可对账，不把其他上下文表加入 IAM 可写白名单。

### B3 [P1] SQL 子集不是 fail-closed，多种跨 owner 表引用静默漏扫

位置：`tools/module-release-tool/src/catalog.rs:1797-1860`（尤其 1839 的关键字名单）。

仅识别 REFERENCES/FROM/JOIN，未验证其余 body 语法是否属于支持子集。policy catalog 中以下四个独立文本均 exit 0 / VALIDATED：

```sql
-- @bp-owned: wave5b_probe
CREATE TABLE wave5b_probe () INHERITS (platform_users);
-- 以下分别是另三个独立 case：
DELETE FROM roles USING platform_users WHERE roles.role_id = platform_users.user_id;
INSERT INTO roles SELECT roles.* FROM roles, platform_users;
ALTER TABLE roles ATTACH PARTITION platform_users DEFAULT;
```

直接 `UPDATE platform_users ...` 的对照例 exit 1，说明 validator 确实运行了，缺陷在遗漏引用位置。见 [repro-results.json](2026-10-08-wave5b-s1-independent-gates/repro-results.json)。SQL 是否能在具体数据/分区配置下执行不是本轮判据；所有这些引用都必须被正确归属或具名拒绝。

违反 §3.5：无法可靠归属/未支持语法必须具名失败。修复可收紧有限 SQL 子集，无需建设通用 SQL 平台；上述四种形式及同 owner 对照、字符串/注释对照都加入共享期望表，两个读取者重跑。

### B4 [P2] 导入及别名调用绕过单一 baseline embed 扫描

位置：`tools/module-release-tool/src/catalog.rs:2868-2870`。

只识别紧邻 `sqlx::` 的 migrate 标识，其他写法直接 continue。第二个 crate 的以下两种实际 Rust 宏调用均被忽略；扫描仍报 baseline_embed_points=1、exit 0：

```rust
use sqlx::migrate;
static M: sqlx::migrate::Migrator = migrate!("../../migrations");
// 另一独立 case：
use sqlx::migrate as embed;
static M: sqlx::migrate::Migrator = embed!("../../migrations");
```

直接 `sqlx::migrate!` 的第二处 embed 对照例被正确拒绝。见 [repro-results.json](2026-10-08-wave5b-s1-independent-gates/repro-results.json)。违反 §3.7 的“无法确定的新写法不得静默跳过”。修复应解析静态 import/alias 或具名拒绝此类导入，保留 SQLite crate 自有 migrations 正例。

### B5 [P2] Rust catalog 读取者接受含 CR 的新 manifest

位置：`tools/module-release-tool/src/catalog.rs:1040-1041`。

`trim_end_matches('\r')` 静默接受 CRLF manifest。把原 validated fixture 的 manifest 换为 CRLF 后，离线 CLI exit 0 / VALIDATED；见 [manifest-crlf-repro.json](2026-10-08-wave5b-s1-independent-gates/manifest-crlf-repro.json)。架构脚本另有拒绝 CR 的断言，因此两个入口规则不一致。

Wave5B 设计 §11.3 C1 明确新 manifest 本身不得含 CR，closeout §2 又声称 catalog 校验各自独立实现了这条。本轮只验证了摘要是否 canonical，未验证 manifest 的换行约束。修复在 Rust 读取层拒绝 CR 并加共享反例；历史 manifest 的 raw/LF/CRLF 容错保持不变。

### B6 [P2] 历史表生成器无法解析，交付的复现入口不可用

位置：`tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1:10`。

参数默认值为未闭合的 `[string]$Out = '`。PowerShell AST ParseFile 检出 8 个语法错误（包含字符串缺少终止符）；其余四个新增脚本没有语法错误。见 [script-parse.json](2026-10-08-wave5b-s1-independent-gates/script-parse.json)。这来自 df0d31a 辅助工具提交，不能仅复审 b25e2d4 而漏掉。

README 把该文件列为可复现入口，closeout 又以“已入库”消除之前的缺口，但当前脚本根本不能运行。修复语法后，在固定 SHA 上生成到本轮临时 Out，与已钉住 fixture 逐字段/摘要对账；不得改 pin 或覆盖原 fixture 来使测试通过。

### B7 [P2] 门禁汇总脚本不传播失败退出码

位置：`tools/module-release-tool/scripts/run-wave5b-gates.ps1:66-124`。

每次 Invoke-Gate 返回 exit code 后均 `Out-Null`，末尾只打印 phase complete，无期望码比较或失败汇总退出。在本轮 target 假仓库用 PowerShell stub 将所有 rtk 命令设为 exit 7，git 设为 exit 0；整个外层 pwsh 仍 exit 0，日志明确记录 catalog 检查 exit 7。见 [gate-wrapper-forced-failure.log](2026-10-08-wave5b-s1-independent-gates/gate-wrapper-forced-failure.log)。该复现只证明 wrapper 行为，不代表真实 cargo 失败。

违反 §3.9 的“原生进程失败必须显式传播”。修复逐项声明期望退出码（未知 check 的负例明确期望 1，其余正例 0），归档后汇总 mismatch 并非零退出；不能用“任何非零即失败”误伤合法负例。

### B8 [P2] 可复跑门禁入口会覆盖已归档日志

位置：`tools/module-release-tool/scripts/run-wave5b-gates.ps1:15,26,54-58`。

固定 2026-09-30 归档目录、固定文件名、WriteAllText 无存在检查；正常照 README 重跑会直接覆盖旧证据。上述假仓库先写 OLD EVIDENCE SENTINEL，执行 fast 后同名文件被替换成新的 exit 7 日志，未要求 Force。这与 README 的“重跑写新文件名并保留旧日志”直接冲突，也复现了 closeout 已承认的旧拒绝日志覆盖风险。

修复应要求新 EvidenceDir/RunId，并拒绝覆盖已存在文件；异常与正常结束均保留本轮日志。旧 2026-09-30 工件保持原样，本轮没有在真实归档上运行此入口。

## 3. 验证与证据边界

- 新增反例均来自本轮离线执行；已记录两个直接拒绝对照，避免把工具未运行误计为反例成功。
- 既有归档独立重算：workspace 883 passed / 0 failed / 70 ignored / 192 suites；cases 76 = 75 MATCH + 1 BLOCKED，0 MISMATCH。两份归档日志头均 exit 0。**这是 2026-09-30 历史证据核对，不是本轮全 workspace 复跑。**
- 当前 `rtk proxy cargo test -p module-release-tool --all-features`：**PASS，exit 0，76 passed / 0 failed / 0 ignored**（包含 S1 19 tests；整条命令输出为本轮工具执行记录，没有伪造一个事后原始日志）。
- 当前 focused/catalog/fmt/architecture：**全部 exit 0**。新证据目录的 `focused-catalog-tests-attempt2.log` 为 19 passed；`catalog-default-attempt2.log` 确认两 catalog RESERVED、历史 40 blob/index 对比 + 20 working-tree 路径、7 条 C1 兼容、单一直接 baseline embed 和两 SQLite 自有 embed；`fmt-attempt2.log` 与 `architecture-attempt2.log` 为 PASS。绿色结果只覆盖既有场景，不能反驳 B1–B8 的独立反例。
- 本轮捕获器首轮误用 PowerShell 自动变量 `$Args` 为参数名，四条记录实际仅调用 `rtk` 而未调用底层门禁，exit 2；对应 `focused-catalog-tests.log`、`catalog-default.log`、`fmt.log`、`architecture.log` 原样保留。修正捕获器后以 attempt2 新文件名重跑成功，不把首轮工具调用错误归因于产品。
- 原 76-case PowerShell 驱动本轮 **NOT RUN**：已确认其 scratch 生命周期有 B1 缺陷，不再运行会清空固定 temp 路径的完整驱动；共享 50-case Rust 读取者已执行，新反例单独隔离。
- 当前完整 workspace check/clippy/test、PostgreSQL ignored、真实安装/升级/回滚/并发/恢复、安全/漏洞/许可证/镜像扫描：**NOT RUN**。本轮为固定候选独立审阅及离线反例，没有数据库或发布授权；发现阻断问题后不借历史绿色日志填本轮 PASS。
- S1 的两正式 catalog 仍仅 RESERVED，ADR-0029 仍 Proposed；G-203a-i 的 S4 工件钉定、其他准入分母及 R2 状态没有改变。

## 4. 修复与停止条件

仅允许下一次修复本报告 B1–B8 及对应共享 fixture/两个读取者/工具测试/收尾文档；不是 S2 授权。保留固定 SHA、历史 SQL/manifest、release generate/verify 契约、PostgreSQL ignored 属性和旧证据。

修复后先验证 scratch 路径拒绝与哨兵存活、平台表不可认领、SQL/宏/CRLF 反例具名拒绝、固定生成器输出对账、wrapper 期望码与日志保护；再跑既有 focused 与完整 workspace/architecture 门禁，逐项记录失败、NOT RUN 和 BLOCKED，提供新的固定本地候选供独立复审。

本轮仅新增审阅报告及独立证据目录，未修改产品代码、实现候选或旧收尾状态；未提交、push、merge、部署、连接数据库、接受 ADR 或推进 S2。结论为 **NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED**。
