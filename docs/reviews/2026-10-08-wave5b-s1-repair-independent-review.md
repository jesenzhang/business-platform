# PLAN-0014 Wave 5B-S1 修复结果独立复审

> 日期：2026-10-08（Asia/Shanghai）
> 结论：**NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**
> 审阅基点/当前 HEAD：`87e8a318fa32ff43d41ad45da8a0f83b3b19a2a4`
> 分支：`codex/plan-0014-wave5a-local-release-pilot`
> 修复载体：**四个文件的未提交工作区增量，没有新的固定实现候选 SHA**
> 问题清单：**2 项 P1、5 项 P2**；B5/B6/B7/B8 在本报告限定证据范围内通过复验。

## 1. 范围与证据

承接 [上一轮 B1–B8 独立审阅](2026-10-08-wave5b-s1-independent-review.md)，对照 [实现者修复总结](2026-10-08-wave5b-s1-fixes.md)，审查 `git diff HEAD` 的四个文件（425 insertions / 44 deletions）及修复报告。遵循同一 S1 指令 §3.5/§3.7/§3.8/§3.9/§3.10，不接受新基线、不推进 S2。

仍使用 open-code-review-delegate，由主线程审阅。OCR workspace preview 的 total_files=23、reviewable_files=11、excluded_files=12；reviewed_files=4、skipped_files=7，OCR 全部 reviewable 的人工覆盖率 4/11=36.36%，**本次修复代码覆盖率 4/4=100%**。7 个 skipped 文件均为上一轮独立审阅遗留的 JSON 原始证据，不是本轮修复：`manifest-crlf-repro.json`、`ocr-preview.json`、`ocr-rules.json`、`repro-results.json`、`scratch-repro.json`、`script-parse.json`、`takeover-repro.json`（均位于 `2026-10-08-wave5b-s1-independent-gates/`）。excluded 中修复总结与上一轮报告作为规格补读；旧日志不计为本轮执行。

本轮证据单独保存在 [repair-independent-gates](2026-10-08-wave5b-s1-repair-independent-gates/)。[reviewed-working-tree.patch](2026-10-08-wave5b-s1-repair-independent-gates/reviewed-working-tree.patch) 与 [reviewed-files-sha256.json](2026-10-08-wave5b-s1-repair-independent-gates/reviewed-files-sha256.json) 固定实际审阅内容，避免将旧 HEAD 当成修复候选。复现输入均置于 `target/wave5b-repair-independent-20261008/`，未执行 SQL。

## 2. 原问题逐项复验

| 原问题 | 独立判定 | 当前证据与剩余项 |
|---|---|---|
| B1 scratch 删除边界 | **PARTIAL / NEEDS REPAIR** | 改为唯一子目录；仓库根被拒绝，旧哨兵存活；但指定 EvidenceDir 启动失败，主目录及 fallback 均不清理，见 R3/R4 |
| B2 误认领 baseline 表 | **PARTIAL / NEEDS REPAIR** | documents 与 _sqlx_migrations 反例现已 exit 1；遗漏 baseline 改名后的 legacy 对象，见 R1 |
| B3 SQL owner fail-closed | **PARTIAL / NEEDS REPAIR** | 原简单 USING/逗号 FROM/ATTACH 反例已拒绝；带别名及 USING 多表仍放行，且合法 index USING 被误拒，见 R2/R6；INHERITS 反例失败是 Symbol 被当 Word 查询的结果，并非正确解析 |
| B4 embed 导入/别名 | **FAIL / NEEDS REPAIR** | 原 bare migrate/alias 反例仍 exit 0；新增空白调用回归，见 R5 |
| B5 新 manifest 不含 CR | **PASS（本轮离线反例）** | 原 CRLF manifest exit 1，具名 LF-only 拒绝；既有历史容错相关测试仍绿 |
| B6 固定历史表生成器 | **PASS（固定 SHA、临时 Out）** | 运行 exit 0；产物 20 条逐字段与 fixture 完全一致，12 LF / 7 CRLF；未覆盖 fixture 或换 pin |
| B7 失败退出码传播 | **PASS（wrapper 隔离 stub）** | 强制正例 exit 7 时外层 exit 1；仅未知 check 负例 exit 1、其余 0 时外层 exit 0；不等于真实全部门禁已执行 |
| B8 旧日志保护 | **PASS（wrapper 隔离 stub）** | 重用已有非空 EvidenceDir 被拒绝，文件 SHA-256 保持不变；默认目录已改为唯一 RunId；post 实际调用另有 R7 |

证据：[repro-results.json](2026-10-08-wave5b-s1-repair-independent-gates/repro-results.json)、[scratch-preflight.json](2026-10-08-wave5b-s1-repair-independent-gates/scratch-preflight.json)、[scratch-repo-refusal.json](2026-10-08-wave5b-s1-repair-independent-gates/scratch-repo-refusal.json)、[cleanup-repro.json](2026-10-08-wave5b-s1-repair-independent-gates/cleanup-repro.json)、[generator-reconciliation.json](2026-10-08-wave5b-s1-repair-independent-gates/generator-reconciliation.json)、[wrapper-results.json](2026-10-08-wave5b-s1-repair-independent-gates/wrapper-results.json)。

## 3. 必须修复的发现

### R1 [P1] Reserved 集合遗漏 019 改名后的既有 legacy 表

位置：`tools/module-release-tool/src/catalog.rs:132-184,2074-2084`。

新增集合只列 CREATE 名称，未考虑固定 baseline 的 RENAME 结果。019 的第 21–24 行产生 `v0_demo_role_permissions`、`v0_demo_user_roles`、`v0_demo_roles`、`v0_demo_permissions`，它们均未被保留集合保护。policy catalog 中以下文本仍 exit 0 / VALIDATED：

```sql
-- @bp-owned: v0_demo_roles
CREATE TABLE IF NOT EXISTS v0_demo_roles (id bigint);
DELETE FROM v0_demo_roles;
```

这是在已存在 legacy 表上 IF NOT EXISTS 不创建新表，却让扫描器认领并授权 DELETE 的同一 B2 绕过。其他 owner 的 baseline 对象不得通过声明获得权限。修复应对账固定迁移执行序列的保留名称（含改名目标及平台 ledger），而不是只抽取 CREATE TABLE 名称。新增注释引用的 `baseline_reserved_tables_are_reconciled_with_pinned_sql` 测试**并不存在**：仓库搜索仅命中该注释；必须补上真实回归。

### R2 [P1] 逗号表列表修复仍漏掉别名及 USING 后续表

位置：`tools/module-release-tool/src/catalog.rs:1943-1948,1971-2013`。

FROM 的逗号循环只在首表后立即遇逗号才运行；首表别名会使其提前 break。USING 只取第一个对象，无列表处理。以下两条各自仍 exit 0 / VALIDATED：

```sql
INSERT INTO roles SELECT r.* FROM roles r, platform_users p;
DELETE FROM roles USING roles r, platform_users p;
```

两者都是明确的跨 owner 引用。原无别名样例 exit 1 不足以证明 fail-closed。修复应按受支持的有限语法解析列表/别名/限定名，不能猜一个 token 就跳过；无法完整归属时具名拒绝。共享 fixture 应覆盖首表别名、多表 USING、AS 别名、逗号后的限定名和同 owner 对照。

### R3 [P2] 所有显式 EvidenceDir 调用在前置校验处失败

位置：`scripts/test-plan0014-wave5b.ps1:349`。

函数签名是 Label1 / Label2，调用却传 `-Label "scratch"`。PowerShell 不能在 Label1 和 Label2 中消歧，真实执行报“参数名称 Label 存在歧义”，exit 1；尚未进入案例或 cargo。修复应使用精确 `-Label1`，并以独立 scratch/evidence 路径跑正常启动及保护反例。不能将该 exit 1 当作成功拒绝非法路径。

### R4 [P2] finally 永远不清理本轮主目录和 fallback

位置：`scripts/test-plan0014-wave5b.ps1:355-379,813-819`。

RunDirectories 记录的主路径就是 canonicalScratchRoot，但清理要求路径必须以 `canonicalScratchRoot + separator` 开头，因此相等的根永远被拒绝。OS temp fallback 不在该根下，也永远被拒绝。提取**当前代码原函数**，在本轮 target 内的两处已验证哨兵目录调用后，两目录都仍存在，警告明确“not under validated scratch root”。

违反 B1 的本轮所有 scratch 在异常/正常退出 finally 清理要求。修复应为每个创建出的运行根记录其受验证父目录及归属，并允许删除已登记的根自身；保留内部证据的模式必须显式区分，不用错误的祖先判断偶然留下证据。哨兵、KeepScratch、默认内部证据和外部证据模式均要对照测试。

### R5 [P2] embed 导入修复无效，并新增空白写法漏扫

位置：`tools/module-release-tool/src/catalog.rs:3050-3105`。

检查 use 前缀时游标仍在 migrate 之前，而 `use` 实际在 `sqlx::` 之前；随后仍先要求 sqlx:: 前缀，而且搜索 needle 只有 migrate，别名 embed 根本不会被遍历。原 `use sqlx::migrate; migrate!(...)`、`use sqlx::migrate as embed; embed!(...)` 和组式导入全部仍被忽略，exit 0。

新 is_migrate_call 还要求 migrate 后紧接 `!`，跳过了此前可识别的 `sqlx :: migrate ! ( "../../migrations" )`。四个第二处 embed 反例均报 baseline_embed_points=1 / exit 0；直接 sqlx::migrate! 的对照例正常 exit 1。修复应以可靠的静态 token/import 解析或具名拒绝覆盖 bare/alias/group/空白形式，继续保留 SQLite 自有目录正例。

### R6 [P2] 通用 USING 扫描误把 index access method 当表

位置：`tools/module-release-tool/src/catalog.rs:1943-1948`。

body_events 被 CREATE INDEX 和 DELETE 等多种语句共用。新增全局 keyword==using 分支无条件把后面 token 当表，导致合法的既有自有表 index 语法被拒绝：

```sql
CREATE INDEX wave5b_ix ON roles USING btree (role_id);
```

当前 exit 1，错误为 references btree、不是 policy 自有表。这不是跨 owner 拒绝，而是 SQL 对象种类误判。修复 USING 时必须携带语句语境，区分 DELETE 表列表、index method、JOIN 列列表等；不支持的形式可具名拒绝，但不能把 access method 当表。至少保留 USING btree 的 index 正例。

### R7 [P2] post 阶段的 EvidenceDir 拼接产生额外位置参数

位置：`tools/module-release-tool/scripts/run-wave5b-gates.ps1:140`。

数组中的 `$EvidenceDir + '/wave5b-cases'` 未加括号；PowerShell 将其求值为数组追加元素，而不是路径字符串拼接。隔离 post stub 捕获的真实参数为：

```text
...|-EvidenceDir|<EvidenceDir>|/wave5b-cases
```

预期应是单一 `<EvidenceDir>/wave5b-cases` 值。额外位置参数会使 driver 误绑定 Repo/报错，且日志命令行已显示中间的空格。使用 Join-Path 或给字符串表达式加括号，再验证传入参数数组，而不仅检查 wrapper 的汇总返回码。此项独立于已通过的 B7/B8 失败传播与日志保护。

## 4. 本轮门禁与限制

- `cargo build -p module-release-tool`：exit 0，[build.log](2026-10-08-wave5b-s1-repair-independent-gates/build.log)。
- `cargo test -p module-release-tool --test wave5b_catalog --all-features`：exit 0，**19 passed / 0 failed / 0 ignored**，[focused-tests.log](2026-10-08-wave5b-s1-repair-independent-gates/focused-tests.log)。这是原有 19 个测试；本轮没有新增任何 fixture/expectation/test 文件，修复声明不能据此闭合新增反例。
- `cargo fmt --all -- --check`：exit 0，[fmt.log](2026-10-08-wave5b-s1-repair-independent-gates/fmt.log)。
- `scripts/check-architecture.ps1`：exit 0，[architecture.log](2026-10-08-wave5b-s1-repair-independent-gates/architecture.log)。只覆盖当前正式空 catalog 及既有静态约束，不覆盖 R1–R7。
- 固定历史生成器：exit 0，结构与每个 entry 完全相同。生成器终端仍打印 forms: lf=19，实际表是 12 LF / 7 CRLF；本报告以逐条对账结果为准，不据打印数字改写摘要口径。
- 完整 module-release-tool 全 suite、workspace check/clippy/test、真实 76-case PowerShell 驱动、PostgreSQL ignored、发布/安装/升级/恢复、安全/漏洞/许可证/镜像扫描：本轮 **NOT RUN**。实现者宣称的 workspace check PASS 没有本轮独立复跑或原始日志，不能算独立通过；driver 有真实前置阻断。

本轮首批新增 Python fixture 使用 write_text，使 Windows manifest 写成 CRLF，先被 B5 抢先拒绝。保留 `repro-results-attempt1.json`，修正为 LF 字节后再计 SQL 结果。初始 renamed-table 探针用了不属于 019 改名目标的 v0_demo_users，因此弃用其归属结论；保留 attempt2，最终用 019 明确产生的 v0_demo_roles 重跑并确认放行。一次串行重跑前 binary 暂不可用，架构构建完成后已重试成功；未把该工具启动失败计入 SQL 判据。

## 5. 收尾边界与下一次修复要求

当前不能接受实现者“8 项均已修复”的总结。允许保留 B5/B6/B7/B8 的有限验证结论，其他项保持 NEEDS REPAIR。必须补齐 R1–R7 对应共享正负 fixture、两个读取者及 PowerShell 正常/异常/cleanup/wrapper 参数回归；禁止只针对已知字符串再加局部条件。修复后提供实际门禁原始日志与新的固定本地候选 SHA，再独立复审。

结束时另出现未跟踪的 `docs/OPERATIONAL_RUNBOOK.md` 和 `scripts/clean-target.ps1`，不属于本轮开始时的修复清单；未读取、修改或执行，保留给其作者。本次不对这两项作审阅结论。四个被审阅修复文件的最终 SHA-256 与记录一致。

本轮只新增本报告及独立证据目录，保留已有四个 dirty 修复文件和所有旧证据，没有修改修复实现、提交、push、merge、连接数据库、接受 ADR 或推进 S2。结论为 **NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED**。
