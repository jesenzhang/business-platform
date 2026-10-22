# PLAN-0014 Wave 5B-S1 独立复评报告（2026-10-22）

> 文档类型：Review
> 状态：Completed
> 日期：2026-10-22
> 复评对象：`6bcdd89d14d188f62e99bbd068cf786b8207699b`（父 `0f29573`）
> 对应 closeout：`53ef8b2`（`docs/reviews/2026-10-09-wave5b-s1-rereview-repair-closeout.md`）
> 复评时 HEAD：`325d249`；评审期间仅追加文档更正 `50d58a0`
> 判定：**CHANGES REQUESTED**（阻断项 1，P1 2，证据问题 2 高）；R2 仍 **BLOCKED**

## 1. 复评对象与范围

- 候选提交 `6bcdd89` 只改两份脚本：`scripts/test-plan0014-wave5b.ps1`（+224/−38）与
  `scripts/test-plan0014-wave5b-repairs.ps1`（+756/−21）。`git ls-tree` 复核 blob：
  driver `71d44b85…`、harness `5b2e8a25…`，与 closeout §1 记录一致。
- 候选之后到 HEAD 只有文档与运维脚本：`scripts/clean-target.ps1`、`docs/operations/RUNBOOK.md`、
  审计报告与 closeout 更正；`git diff 6bcdd89..HEAD -- '*.rs' Cargo.lock` 为空，两份被评脚本
  blob 未变。因此门禁可以在 HEAD 复跑并归因到候选。
- 范围：两份脚本的 cleanup surface（`Remove-Tree`、`Get-UnsafeLink`、`Register-RunRoot`、
  `Remove-RunScratch`、`Get-ReparseCapableScratch`）、closeout 的声明与证据、离线门禁。
- 非范围：业务 Rust 代码、数据库/对象存储/Provider 契约、部署、R2 admission 本身。

## 2. 方法与独立性

三个隔离通道并行执行，均由未接触实现过程的上下文完成；主导评审另做确定性探针复核：

1. **安全代码对抗评审**：逐行追踪两个脚本与 base `0f29573`，用真实生产函数（AST 抽取）
   在 `%TEMP%` 做 disposable 探针，尝试反证 closeout §2/§6 的安全保证。
2. **证据/声明审计**：重算 `start-state.json`/`final-state.json` 的 SHA-256，逐条比对
   closeout 的量化声明与日志/CSV，核对 fail-first 与 probe 证据的来源。
3. **门禁复跑**：在 HEAD `325d249` 串行重跑 fmt、architecture、driver113、repair-scenarios
   与两个 crate 测试，原始日志落盘。

所有通道只读仓库；唯一写入是证据目录 `docs/reviews/2026-10-22-independent-rereview-gates/`。
评审期间未修改、未提交任何被评代码；分支在评审开始前已推送至 `origin`
（`codex/plan-0014-wave5a-local-release-pilot`，60 commits ahead of `origin/main`）。

## 3. 结论

候选确实修复了 rereview 指定的三个缺陷，且回归覆盖真实生产函数：

- Q1：预置在注册父之上的 junction 现在被拒绝；
- Q2：树内 “pop 前替换” 与 “枚举后替换” 两个窗口被保留（retain）+ 具名拒绝；
- Q3：固定全局 scratch 基座被移除，改为每次运行 GUID 命名并记录所有权。

门禁数字全部复现：driver `113 = 112 matched + 1 BLOCKED + 0 mismatched`，
repair scenarios `29/29`，`module-release-tool 108 passed / 0 failed / 0 ignored`，
`runtime-migration 1 passed`，`cargo fmt` 与 architecture fitness 通过。

但 closeout 的核心安全保证不成立：**遍历期间替换 walk root 之上的祖先目录，外部树会被删除，
且调用返回成功、无异常、无告警**。这与 §2 “for the whole chain it is reached through” 和
§6 “Content outside a run root is never at risk in either case” 直接冲突。加上两处 P1
（子项删除使用枚举缓存属性；retention 不会让门禁失败）与两处高优先级证据不可复现问题，
候选尚不能进入 R2。

## 4. 阻断项 C1 — 遍历中替换 walk root 之上的祖先，外部内容被静默删除

**代码位置**：

- `scripts/test-plan0014-wave5b.ps1:265-278`：`Remove-Tree` 在进入尝试循环前，对
  `$treeRoot` 之上的祖先链**只检查一次**；
- `scripts/test-plan0014-wave5b.ps1:241-257`：`$unsafeWithin` 在到达 `$WalkRoot`
  时返回空串（248-250），因此每次随动作执行的证明只覆盖 “目录 → walk root”；
- 每次枚举/删除前的证明（288、296、330）因此都不覆盖 walk root 之上。

**冲突的声明**：closeout §2 “repeated at the moment of the action, for the whole chain it
is reached through”；§6 “Content outside a run root is never at risk in either case”。
§6 另一句 “every component between it and the walk root” 字面为真——问题正是证明止步于
Q1 开始的地方。

**确定性复现**（单进程，真实生产函数，与既有 Q2 回归同一 shim 手法）：

```text
probe: docs/reviews/2026-10-22-independent-rereview-gates/probe-ancestor-midwalk/ancestor-midwalk-probe.ps1
setup: anchor/scratch/run-owned（待删树）；outside/scratch/run-owned/{sentinel}
shim : 首次 Get-ChildItem(run-owned) 时 Move-Item anchor → anchor.parked;
       New-Item -ItemType Junction anchor → outside
call : Remove-Tree -Path anchor/scratch/run-owned
```

实测输出：

```text
swapped_during_walk=True
external_sentinel_survives=False
remove_tree_message=            (空：没有拒绝、没有异常)
```

对照探针（同一脚本，禁用替换）：

```text
swapped_during_walk=True        (标记已置位，未执行替换)
external_sentinel_survives=True
```

独立安全通道用 `Remove-RunScratch` 走完 `Register-RunRoot → Remove-RunScratch` 全链路复现：
外部 `sentinel.txt`、`mine.txt` 与 `run-owned` 全部消失，外部条目 2 → 1，RunLog 与 warning
均为空，停放的 run 树原样保留——即“删了别人的树、报告成功、自己的树没删”。

**为什么可达**：在 `-EvidenceDir` 模式（门禁实际使用的模式）下，
`Remove-RunScratch` 的 containment/chain/witness 检查都在前，`Remove-Tree` 的首次
`Get-ChildItem` 是清理的第一个枚举动作；替换发生在此后即可重定向后续所有路径解析。
`$unsafeWithin` 只往下看到 walk root，看不到已被替换的祖先。

**建议修复**（约 10–20 行）：

1. 让 `$unsafeWithin` 走到卷根（保留“必须经过 `$treeRoot`”的语义），而不是在
   `$WalkRoot` 返回；
2. 或在每次枚举前与每批删除前重跑 268-278 的祖先链检查；
3. 新增回归：首次枚举时替换**注册父之上**的组件，断言外部 SHA-256/条目数不变，
   且抛出具名拒绝（当前没有任何异常）；
4. 同步修正 §6 的措辞，使“保证”与实现的证明范围一致。

**验收**：修复前该回归必须失败（外部 sentinel 消失）；修复后 30/30 通过，
外部 sentinel 与条目数不变。

## 5. P1

### P1-1 子项删除决策使用枚举时缓存的属性

`scripts/test-plan0014-wave5b.ps1:302-317` 使用 `$child.Attributes`（304）与
`$child.PSIsContainer`（306），二者是枚举时缓存的 `FileSystemInfo` 值。实测：文件被替换为
junction 后，旧 `FileInfo` 仍报告 `Archive, PSIsContainer=False`，而新建 `Get-Item`
报告 `Directory, ReparsePoint, True`。因此 296 的枚举后证明与 305-310 的删除之间没有
重新读取子项身份；并发写者在该区间替换目录时，`$child.Delete()` 会按名字作用于外部条目。
仅在并发写者存在时可达（closeout 已把多进程对手列为 NOT RUN），故不阻断，但 §6 的
“as it uses it / at that moment” 措辞覆盖了这一情形而实现没有。
建议：删除每个子项前 `Test-ReparsePoint $child.FullName`（或每批前重证 `$directory`），
成本为 O(children) 次 `GetAttributes`。

### P1-2 retention 永远不会让门禁失败

closeout §2/§6 称 “Retention is a failure the caller has to report”“the gate treats a
retained scratch as a failure rather than a success”。实现并非如此：

- driver `730-732`：`Remove-RunScratch` 捕获 `Remove-Tree` 的具名 retention 拒绝，只
  `Write-Warning`，进程退出码不变；
- harness `169-186`：`Remove-RunOwnedScratch` 捕获所有清理异常，只记 note；
  `1514-1545` 汇总前没有 `$script:RunOwned.Count -eq 0` 断言；
- `%TEMP%\plan0014-wave5b-links-<guid>` 这类 link scratch 没有残留断言。

建议：retention 计入 failures 并在结束时非零退出；harness 断言所有权列表为空；
把 link scratch 纳入残留检查。

## 6. P2

1. **干净工作区无法启动门禁**：默认 scratch 父目录 `target/tmp` 不存在时，
   `530-543` 直接 `throw`。本次独立复评首次运行即复现（需先 `New-Item target\tmp`）。
   建议：仅对脚本默认路径自动创建父目录；用户自定义 `-Scratch` 仍保持拒绝。
2. **`Get-ReparseCapableScratch` 依赖未定义的 `$OutDir`**（repairs `233-235`）：单独导入时
   在尝试 `%TEMP%` 之前就因参数绑定失败；同一进程第二次调用会把自己上一次的 base 判为
   “他人的 scratch”；OutDir fallback base 会进入 `$script:RunOwned` 并被删除，与 §5
   “OutDir is evidence and never enters the list” 不符。
3. **卷根 boundary 永远不可达**：`Get-UnsafeLink` 的 `TrimEnd('\')`（`121`）把 `C:\` 变成
   `C:`，而遍历比较的是 `C:\`，随后 `Split-Path -Parent 'C:\'` 返回 `''` 并跳出循环，
   于是 `Register-RunRoot -Parent <drive root>` 必然抛“never reaches its boundary”。
   fail-closed，当前调用者不会触发，但应修正或记为已知限制。
4. **fail-closed 祖先策略的运维后果未写明**：目标之上的任一祖先为 reparse point 时，
   `Remove-Tree` 抛错、`Remove-RunScratch` 警告并跳过。若仓库或 scratch 路径经过 junction
   （OneDrive 重定向目录、junctioned workspace），门禁将完全不可运行。这是设计选择，
   但 closeout 未说明。
5. **哈希可复现性（LF/CRLF）**：`final-state.json` 的 driver 哈希 `7B7F2095…` 是 LF 字节；
   Windows `core.autocrlf=true` 检出后工作区为 CRLF，`Get-FileHash` 得到 `0E748E10…`。
   内容一致（`git hash-object` = `71d44b85…`），但复评者按字面复算无法复现。
   建议在 closeout/README 声明“哈希按 LF 规范化字节计算”，或对 `scripts/*.ps1` 设
   `.gitattributes eol=lf`。
6. **§10 probe 脚注（2026-10-22 添加）错误**：`docs/reviews/2026-10-09-wave5b-s1-rereview-gates/`
   下的 `.ps1` 是评审轮原始文件（仍指向已删除的 `target/w5b-review-1009/…`），且 queued
   副本没有执行副本所需的 `try`/`catch` 与两行输出，无法产生 §7 的 JSON/拒绝文本；
   执行副本未保留。该脚注已由 `50d58a0` 更正，但“§7 probe 数值不可从仓库复现”的证据缺口
   仍成立。
7. **scratch 基座残留（P3）**：repair 套件在 `%TEMP%\wave5b-reparse-probe` 留下空的基座
   目录（本次运行确认存在）；不污染仓库，但与“退出时自行清理”的描述不完全一致，
   建议在释放所有权列表时一并移除该基座。

## 7. 证据审计

审计覆盖 closeout 全部可量化声明。**所有门禁数字与 10 个日志哈希均可精确复现**，
但以下声明不成立或不可复现：

| 级别 | 问题 |
|---|---|
| **高** | §7 称 fail-first 使用 “the final scenarios”，日志头却写明来自 `0169d85`（孤儿草稿，harness blob `8b3f272` ≠ 最终 `5b2e8a25`，diff 42/11）；差异恰好落在两个 Q2 场景：queued-swap 的监视目标由 `$child` 改为 `$run`、enum-swap 增加 sibling positive control。因此 closeout §4 描述的 Q2 “queued before pop” 窗口**没有** fail-first 证据 |
| **高** | §10 脚注指向的 probe 副本不是执行副本（见 P2-6），§7 的 probe 数值不可复现 |
| 中 | “final-state hashes every log above” 不成立：`gate-timeline.log`、`git-diff-check.log`、`git-status-short.log` 未纳入 |
| 中 | 两个外来未跟踪文件没有记录哈希，“unmodified” 不可验证；现已被删除/改写并提交（`7d71260`） |
| 中 | “Nothing was pushed” 只对 2026-10-09 成立；分支已于 2026-10-22 推送（`325d249`） |
| 中 | 同一文件出现 `606CCECE…` 与 `FB56AE7E…` 两个 SHA-256，未说明是 LF/CRLF 两种字节形态；`FB56AE7E` 还被误标为 `git show` 输出 |
| 低 | witness 文件名漏写 `.owned` 后缀；“OutDir 永不进入列表”与 fallback 实现矛盾；“4601 probes / 0.14 s” 无归档测量；“12 suites” 把 doc-test 块计入；“per-command logs 同 header” 过度表述；“three depths” 仅 depth 1 有日志；§8 “probes unmodified in logic” 与 §10 自述的 try/catch 改动不一致 |

结论：证据的**数字层**可信，**来源层**需更正；在 §7/§10 修正前，不能把该报告当作可直接
复跑的证据包。

## 8. 门禁复跑（HEAD `325d249`，脚本 blob 与候选相同）

| 步骤 | 命令 | 结果 |
|---|---|---|
| 格式 | `cargo fmt --all -- --check` | exit 0，无输出 |
| 架构适应度 | `pwsh -NoProfile -File scripts/check-architecture.ps1` | `Architecture fitness: PASS`（Cargo metadata + OpenAPI contract） |
| 离线驱动 | `pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1 -EvidenceDir …/driver113` | exit 0；113 例，**112 matched / 1 BLOCKED / 0 mismatched**；BLOCKED = `P2-catalog-symlink` |
| 修复回归 | `pwsh -NoProfile -File scripts/test-plan0014-wave5b-repairs.ps1 -OutDir …/repair-scenarios` | exit 0；**29/29**，`executed=29 skipped=0 failed=0 blocked=0 declared=29` |
| 工具 crate | `cargo test -p module-release-tool --all-features` | **108 passed / 0 failed / 0 ignored**，12 个结果块 |
| 迁移 crate | `cargo test -p runtime-migration --all-features` | **1 passed / 0 failed / 0 ignored** |

证据：`docs/reviews/2026-10-22-independent-rereview-gates/01-…06-*.log`、`driver113/`、
`repair-scenarios/`、`probe-ancestor-midwalk/`；完整命令、时间戳与计数器见同目录
`RUN-MANIFEST.md`。

**运行时序与洁净性**：六步严格串行，墙钟 13:07:43–13:23:00（+08:00），命令总时长 860.7 s；
fmt 1.9 s、architecture 14.7 s、driver 286.0 s、repairs 296.1 s、tool crate 229.2 s、
migration crate 32.8 s。首次 repair 套件运行期间，复评者为更正 §10 脚注修改了 closeout
（纯文档），因此该步不是严格“干净跟踪树”；随后在提交 `50d58a0` 后于干净跟踪树重跑
repair 套件（`repair-scenarios-clean/`），结果一致。driver 步骤在更正前完成；两个 crate
测试在其后运行，但输入 `*.rs` 与 `Cargo.lock` 与候选一致，且被评脚本 blob 复评期间未变。

**NOT RUN（明确记录，不视为通过）**：

- `cargo check/clippy/test --workspace --all-targets --all-features`（全量工作区；本次只跑了
  两个相关 crate；候选改动不触及 Rust，但 R2 前仍应由 CI 或复评方补跑）；
- `scripts/check-openapi.ps1`；
- 70 个 `#[ignore]` 的 PostgreSQL/S3/MinIO 契约测试；
- `runtime-migration` 在本 HEAD 仅执行 1 个单元测试（另 1 个 doc-test 块 0 用例），
  对该 crate 是窄信号，不能替代全量工作区测试；
- secret、漏洞、许可证、镜像扫描；
- 文件符号链接（需提权 → `P2-catalog-symlink` 永久 BLOCKED）；
- mount point、ReFS、网络卷、真正的多进程对抗。

## 9. 已证实正确的部分

- 候选身份与 blob、改动范围（仅两脚本、无 Rust/Cargo.lock/migration 变更）；
- Q1：预置祖先 link 在注册与清理入口被拒绝；
- Q2：树内 pop 前替换、枚举后替换均被 retain 并抛具名拒绝（既有 29 例回归真实使用
  `Move-Item` + `New-Item -ItemType Junction`，并断言外部 SHA/条目数）；
- Q3：固定全局 scratch 基座消除，GUID + 所有权列表 + 既有 base 拒绝；
- witness：GUID 名 + token 内容，且在 containment/chain/protected 之后读取；
- 无递归删除：两脚本均无 `Remove-Item -Recurse`，目录删除为
  `[System.IO.Directory]::Delete($dir, $false)`，junction 只删链接（实测目标存活）；
- `GetAttributes` 不跟随链接、`Resolve-Path` 保留 junction 拼写——修复的前提正确；
- Q1/Q3 的 fail-first 与 positive control 证据有效；29/29、113、108、911/70 等数字复现。

## 10. 判定与准入条件

**CHANGES REQUESTED。** 候选在指定缺陷上是真实修复，但其安全声明超出机制能力，且存在
可复现的外部删除路径与两处证据来源问题。

必须完成：

1. **C-1**：修复 C1（证明覆盖到卷根，或每次动作前重证祖先链），新增“首次枚举时替换
   walk root 之上组件”的回归，修复前失败、修复后通过，并同步 §6 措辞；
2. **C-2**：修复 P1-1（子项删除前重读身份）与 P1-2（retention 必须使门禁非零退出），
   或在 closeout 中把它们降级为明确的已知限制并停止使用“failure”表述；
3. **C-3**：更正 §7 fail-first 来源（用候选 harness 重跑，或明确写出 `0169d85` 及其差异）、
   §10 证据范围（gate-timeline 哈希、probe 副本不可复现），统一 LF/CRLF 哈希口径；
4. **C-4**（建议同批）：让默认 `target/tmp` 在干净工作区可创建，使门禁从空 `target/`
   可直接运行。

**R2 admission 仍为 BLOCKED**：本次复评不构成 R2 证据，也不授权合并；需先完成 C-1..C-4，
再在完整 CI（含 Linux symlink 覆盖）通过后重新提交复评。

## 11. 证据与复现

```text
docs/reviews/2026-10-22-independent-rereview-gates/
├── 01-cargo-fmt.log
├── 02-check-architecture.log
├── 03-driver113-wave5b.log
├── 04-repair-scenarios.log
├── 05-cargo-test-module-release-tool.log
├── 06-cargo-test-runtime-migration.log
├── driver113/                     # 113 例逐例 CSV 与日志
├── repair-scenarios/              # 29 例逐例日志
└── probe-ancestor-midwalk/        # C1 复现脚本、对照脚本与两份输出
```

复现 C1：

```powershell
pwsh -NoProfile -File docs/reviews/2026-10-22-independent-rereview-gates/probe-ancestor-midwalk/ancestor-midwalk-probe.ps1
# 期望（修复后）: external_sentinel_survives=True 且 remove_tree_message 非空（具名拒绝）
# 现状: external_sentinel_survives=False 且 remove_tree_message 为空
```

该目录按仓库惯例保持 untracked；复评结束前未修改任何被评代码。

## 12. 复评者声明

- 三个通道与主导评审均为只读；除证据目录外未写入仓库，未修改、未提交被评代码。
- 评审开始前分支已由提交者推送至 `origin`（`325d249`）；本次复评新增的文档更正为
  `50d58a0` 与本报告。
- 未进行 merge、deploy 或 R2 admission；未接触数据库、对象存储或外部 Provider。
