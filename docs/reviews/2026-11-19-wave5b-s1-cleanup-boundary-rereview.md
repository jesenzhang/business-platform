# PLAN-0014 Wave 5B-S1 cleanup boundary 独立复评报告（2026-11-19）

> 文档类型：Review
> 状态：Completed
> 日期：2026-11-19
> 复评对象：`b4ccbe888986a9e34454f7b5c5b2fce5fe436aec`（S1 整改提交，父 `eb53250`）
> 上游上下文：集成提交 `09a6fa93acd671fbddd97c8ad71dcc6e8ce4e631`（merge，未改被评脚本）
> 复评分支 / 工作区：`codex/plan0014-wave5b-s1-rereview` @ `b4ccbe8`，
> `E:\working\business-platform-wt\plan0014-rereview`
> 上一轮判定：`docs/reviews/2026-10-22-wave5b-s1-independent-rereview.md` §10 =
> **CHANGES REQUESTED**（C-1..C-4）
> 本轮判定：**PASS WITH COMMENTS**
> 完整 CI：**NOT RUN / 待主会话确认**（上游 draft PR #20 仍在运行；本报告不对 CI 下结论）

## 1. 复评对象、边界与独立性

- 我只读并复跑代码，未修改任何被评代码、未改主工作区 `E:\working\business-platform`、
  未 push、未 merge、未设置 `sandbox_permissions`、未请求审批。唯一仓库写入是本报告。
- `b4ccbe8` 的改动面（`git diff-tree --no-commit-id --name-status -r b4ccbe8`）：
  仅 4 个文件——`scripts/test-plan0014-wave5b.ps1`、
  `scripts/test-plan0014-wave5b-repairs.ps1`、
  `docs/reviews/2026-10-09-wave5b-s1-rereview-repair-closeout.md`、
  `docs/reviews/2026-10-22-wave5b-s1-independent-rereview.md`（+465/−26）。
  无 Rust / `Cargo.lock` / migration / 部署文件变更。
- 两份被评脚本的 blob 在 `b4ccbe8` 与集成提交 `09a6fa9` 完全一致：
  driver `9f2e0238b03557042e2d1d422aed629bceb5c44c`、
  harness `46d7c64da18425f02c67da97bb5392f24530f336`。
  因此以 `b4ccbe8` 为复评基准、在当前工作树复跑门禁可归因到被评提交。
  （`09a6fa9` 相对第一父带入大量 main 变更，确实不止 CI 修复，但没有触碰这两份脚本。）
- 证据目录 `docs/reviews/2026-11-19-wave5b-s1-cleanup-boundary-gates/` 按仓库惯例为
  untracked，在我新建的工作树中不存在；我以只读方式检查了实现方工作树
  `D:\AgentLoop\worktrees\business-platform\engineering-baseline` 下的原始日志，
  并在自己的临时目录独立重跑了关键脚本（命令与结果见下）。

## 2. 我实际重跑的命令与结果

所有重跑在 `%TEMP%\w5b-rereview-indep\`，并显式把 cwd 设为复评工作树；
过程-scoped 设置 `GIT_CONFIG_COUNT=1 / GIT_CONFIG_KEY_0=core.longpaths /
GIT_CONFIG_VALUE_0=true`（只影响子进程，不改仓库或全局 git 配置）。

| 目的 | 命令（要点） | 我的结果 |
| --- | --- | --- |
| 4 个新增场景 | `pwsh -NoProfile -File scripts/test-plan0014-wave5b-repairs.ps1 -Repo <wt> -OutDir <tmp>\fixed-four -Only ast-remove-tree-ancestor-swapped-on-first-enumeration,ast-retention-fails-the-gate,static-retention-fails-gate,entry-c4-default-scratch-from-empty-target` | exit 0；`executed=4 skipped=29 failed=0 blocked=0 declared=33` |
| C1 fail-first（原始 blob） | 用 `git cat-file blob 71d44b8…` 取得修复前 driver，临时 harness 仅从 `$wanted` 去掉 `Assert-NoRetainedScratch`（修复前 driver 无此函数），`-Only <C1场景>` | exit 1；`FAIL … external tree went from 5 to 1 item(s) … Remove-Tree said: `（空） |
| C1 单一 hunk 隔离 | 在修复后 driver 的临时副本中只把 `$unsafeWithin`（driver:255-286）还原为“到 walk root 即返回”的旧体，其余修复保留，`-Only <C1场景>` | exit 1；同样的外部树 5→1、无拒绝 → 证明因果关系在 C-1 本身，而非 P1-1 |
| C1 修复后 | 同上，使用工作树真实 driver | exit 0；`PASS`；具名拒绝：`… is reached through the reparse point …\anchor while it was being enumerated` |
| 全量修复回归 | `pwsh -NoProfile -File scripts/test-plan0014-wave5b-repairs.ps1 -Repo <wt> -OutDir <tmp>\repair-full3` | exit 0；`executed=33 skipped=0 failed=0 blocked=0 declared=33` |
| 113 例 driver 门禁 | `pwsh -NoProfile -File scripts/test-plan0014-wave5b.ps1 -Repo <wt> -EvidenceDir <tmp>\driver113-indep2 -Scratch C:\w5brs2` | exit 0；`summary total cases=113 mismatched=0 blocked=1`（88/11/7/5/2），BLOCKED=`P2-catalog-symlink` |
| 显式 `-Scratch` 负向探针 | driver `-Scratch C:\w5b-no-such-parent-9f3a\scratch -EvidenceDir <tmp>` | exit 1；`scratch parent '…' does not exist; cannot validate scratch path`（driver:595） |

**我自己的失败尝试（如实记录，均为调用环境问题，不是代码缺陷）**：
第一次全量回归与 driver 运行继承了我会话的默认 cwd（主工作区 `main` 分支，无
`tools/module-release-tool`），报 `package ID specification … did not match any packages` /
`entry-r4-internal-default-lifecycle` 失败；第二次全量回归未设 `core.longpaths`，
在深层工作树 scratch clone 触发 `Filename too long`（与 RUN-MANIFEST §Environment notes
记录完全一致）。改为显式 workdir + process-scoped `core.longpaths` 后两次均通过。
这些失败恰好独立印证了实现方对环境限制的记录是诚实的。

## 3. 逐项独立验证

### C-1 — 祖先链重证到卷根、仍要求经过 walk root；外部目录不被删除：已修复

**代码证据**（driver）：
- `test-plan0014-wave5b.ps1:255-286`：`$unsafeWithin` 从 `$Directory` 逐组件走到卷根；
  每遇到 reparse point 立即返回该组件（270-271）；经过 walk root 时置 `$sawWalkRoot`
  （273-275）；到卷根结束循环后若未经过 walk root 则返回 `$Directory` 作为不安全
  （282-284）。这同时满足“重证到卷根”与“仍要求经过 walk root”两个要求。
- 该闭包在**每次动作**被调用：进入目录前 317、枚举后 325、删除前 382。
  进入尝试循环前还有一次独立的祖先检查 297-307（保留的纵深防御）。
- `Remove-Tree` 在拒绝时抛具名 refusal（371-377、392-397、412-414），不是静默成功。

**回归证据**（harness `test-plan0014-wave5b-repairs.ps1:1099-1150`）：
`ast-remove-tree-ancestor-swapped-on-first-enumeration` 用真实 `Move-Item` +
`New-Item -ItemType Junction` 在首次 `Get-ChildItem(run-owned)` 时把注册父之上的
`anchor` 换为指向外部树的 junction；断言外部 sentinel 与 nested 文件的 SHA-256 不变、
外部条目数不变、抛出具名拒绝、被停放的 run 树完好、种下的 junction 被保留。
这是真实执行 `Remove-Tree`（harness 通过 AST 从当前 driver 导入生产函数，
见 harness:290-329），不是静态字符串断言。

**fail-first**：我实际重跑证明三态——修复前 blob `71d44b8` FAIL（外部 5→1、
sentinel 消失、无拒绝、exit 1）；只回退 C-1 hunk 也 FAIL；修复后 PASS（具名拒绝、
exit 0）。实现方归档 `failfirst-c1-stdout.log` / `failfirst-c1-rerun/` 与
`positive-c1-stdout.log` / `positive-c1/` 与之逐字一致（含 5→1 与外部 sentinel 路径）。
上一轮 §6 “Content outside a run root is never at risk” 的措辞已同步到
driver:214-234 与 closeout §6/§11，且现在由机制支撑。

**结论：C-1 真实修好。**

### C-2 — P1-1 子项身份重读、P1-2 retention 非零退出：均已修复

**P1-1**（driver:331-363）：
`childNow = Get-Item -LiteralPath $childPath -Force -ErrorAction Stop`（348）在动作时
重读；把 `(link, container)` 与枚举时快照比较（346-351），任一不同即 `retained` +
`continue`（351-355）；已为 link 的子项仍按名字 unlink（356-357，驱动自带 junction
fixture 不再被误判）；普通文件用重读后的 `childNow` 删除（360-363）。
我独立复跑的 113 例 driver 门禁 exit 0、0 mismatched：若清理把驱动的
`link-junction/.../policy` 自有 junction 误判为 retention，P1-2 会让进程非零退出——
实跑未发生，直接反证了“不误判驱动自有 junction”。

**P1-2**（driver:63-66、723/733/743/761/775/794、800-810、1360-1365）：
`Remove-RunScratch` 在 6 条拒绝路径写入 `$script:RetainedScratch`；
`Assert-NoRetainedScratch` 在 `finally { Remove-RunScratch }` 之后调用（1365），
有任何记录即抛 `retained scratch … Retention is a gate failure`（800-810）。
有意保留的 internal evidence 不计入（closeout §11 明确说明），避免假阳。

**对应回归**：
- `ast-retention-fails-the-gate`（harness:1603）：真实导入的 `Remove-RunScratch` +
  `Assert-NoRetainedScratch`，含空列表正控；我的独立重跑 PASS。
- `static-retention-fails-gate`（harness:1638）：AST 检查 driver 确实调用该断言且
  `Remove-RunScratch` 确实填充列表——这是**明确标注的静态伴随检查**，与上面的
  动态用例配对，不是用字符串代替执行。
- 实现方归档 `focus-after-p11-refine/` 显示 P1-1 改动后 7/7 PASS（含两条 Q2 窗口回归），
  与我复跑的 33/33 一致。

**结论：C-2 真实修复，动态/静态/正控三类回归有效。**

### C-3 — closeout 来源、证据范围、LF/CRLF 口径的更正：诚实可追溯

我从 git 独立核对了每一条更正：

| 更正 | 实现方声明 | 我的核对 |
| --- | --- | --- |
| fail-first 来源 | `0169d85…`，harness blob `8b3f272…`，最终 `5b2e8a25…`，diff 42/11 | `git show -s 0169d85` = `fix(plan-0014): prove the wave 5b-s1 cleanup path, not its name`；`git rev-parse 0169d85:…repairs.ps1` = `8b3f272ab4…`；`6bcdd89` = `5b2e8a253…`；`git diff --numstat` = `42 11` ✓ |
| “queued before pop 无最终 harness 的 fail-first” | 明确写为 source gap | 与上一轮 §7 的发现一致，是降级/撤回而非新 PASS ✓ |
| §10 证据范围 | 撤回 “hashes every log above”，列出 3 个未纳入日志 | 属于撤回过度表述，方向诚实 ✓ |
| LF/CRLF 哈希口径 | driver LF `7B7F2095…` / CRLF `BD1B84EF…` / blob `71d44b85…`；harness LF `F235F5F2…` / CRLF `B9ED44ED…` / blob `5b2e8a25…` | 我用 `git cat-file blob` 原始重定向实测 LF SHA-256 = `7B7F2095…`、`F235F5F2…`；工作树 `Get-FileHash` = `BD1B84EF…`、`B9ED44ED…`；`git hash-object` = `71d44b85…`，全部吻合 ✓ |

**PASS 表述审计**：新增 §11 的每个 PASS 都指向 `RUN-MANIFEST.md` 的具体日志；
§11 明确列出 NOT RUN 并两次声明 “R2 admission remains BLOCKED / NOT CLAIMED”。
上一轮 closeout 早期章节仍保留当时（2026-10-09）的全工作区 PASS 数字
（911 passed / 70 ignored 等），但那是历史轮次结果，且本轮 §11 对全工作区明确写
NOT RUN，没有把未跑项写成通过；这些历史证据目录在我当前工作树不存在、无法复核，
记为 comment 而非阻断。

**结论：C-3 的更正诚实、可追溯，未发现新的无证据 PASS。**

### C-4 — 默认 target/tmp 可创建、显式 -Scratch 仍 fail-closed：已修复

- driver:55-60 记住 `-Scratch` 是否由调用方给出；591-601 只有默认路径
  （`UsingDefaultScratch=true`）才 `New-Item` 缺失的父目录；显式路径仍走 594-595 的
  `throw`，fail-closed 保留。
- harness `entry-c4-default-scratch-from-empty-target`（1661）会停放已有 `target/tmp`、
  以真实 driver 子进程启动、要求越过 preflight 并到达 evidence 保护规则、且确认默认父
  目录被创建；我的独立重跑 PASS，其 `entry-c4-child-output.txt` 显示
  `evidence directory … already holds 1 item(s)`（说明已越过 scratch preflight）。
- 显式 `-Scratch` 的负向路径没有自动化回归，但我做了直接探针：exit 1 +
  `scratch parent 'C:\w5b-no-such-parent-9f3a' does not exist; cannot validate scratch path`，
  与代码一致（记为非阻断 comment）。

**结论：C-4 修复到位。**

## 4. 新增回归是否真的执行脚本

是。逐项性质如下：

| 场景 | 执行内容 |
| --- | --- |
| `ast-remove-tree-ancestor-swapped-on-first-enumeration` | 从 driver AST 导入的真实 `Remove-Tree`；真实 `Move-Item` + junction + 作用域内 `Get-ChildItem` shim；断言外部文件 SHA/条目数与具名拒绝 |
| `ast-retention-fails-the-gate` | 真实导入的 `Remove-RunScratch` 与 `Assert-NoRetainedScratch`，含空列表正控 |
| `static-retention-fails-gate` | 明确标注的 AST 伴随检查（非唯一证据） |
| `entry-c4-default-scratch-from-empty-target` | 真实 `pwsh -File driver` 子进程，检查退出码与输出文本，并检查文件系统副作用 |

我独立重跑时，C1 场景日志的
`PS>TerminatingError(): "Remove-Tree retained 1 object(s) … is reached through the reparse point …\anchor …"`
证明拒绝来自被评代码路径本身。

## 5. 证据目录与 RUN-MANIFEST 自洽性

逐条重算（实现方工作树中的归档日志）：

- `04-repair-scenarios-final.log`：33 行 `PASS`，末行
  `executed=33 skipped=0 failed=0 blocked=0 declared=33`；与 manifest 第 39 行一致。
- `03-driver113-wave5b-final2.log`：
  `summary total cases=113 mismatched=0 blocked=1`，家族 88/11/7/5/2，
  BLOCKED=`P2-catalog-symlink`；与 manifest 第 38 行一致。
- `failfirst-c1-stdout.log`：`executed=1 skipped=29 failed=1 … declared=30`，exit 1；
  `positive-c1-stdout.log`：`executed=1 skipped=29 failed=0 … declared=30`，exit 0；
  与 manifest 第 29-30 行一致。`declared=30`（当时仅新增 C1 场景）与最终 `declared=33`
  的时间线差异由 manifest 的阶段记录解释，自洽。
- `05b-cargo-test-module-release-tool-standalone.log`：12 个 `test result: ok` 块
  （31+5+8+8+3+10+6+9+8+20 = 108），`0 failed / 0 ignored`；与 manifest 第 41 行一致。
- `06-cargo-test-runtime-migration.log`：`1 passed / 0 failed / 0 ignored`；与第 42 行一致。
- 脚本身份表：LF/CRLF/git blob 三种口径均已由我从 git 原始 blob 与工作树文件复算吻合
  （见 C-3）。

未发现日志计数与 manifest 互相矛盾之处。

## 6. BLOCKED / NOT RUN 是否被写成 PASS

没有。核查点：

- driver 的最终表述是 “every case that could run matched … 1 case(s) are BLOCKED”，
  且在 closeout §8 明确写 “Not 113/113, and not written as such”。
- `P2-catalog-symlink` 在 driver 日志和两份文档中始终为 BLOCKED，未计入通过。
- 全工作区 `cargo check/clippy/test --workspace --all-targets --all-features`、
  `check-openapi.ps1`、70 个外部服务测试、Linux symlink、文件符号链接、mount point、
  ReFS/网络卷、secret/许可证/漏洞扫描、真实多进程对抗，均明确 NOT RUN。
- 链接工作树里 `module-release-tool` 的 3 个 `source_derivation` 失败（exit 101）没有
  被掩盖成通过，而是记录原因并用独立克隆的 108/108 作为权威结果。

## 7. 改动 bounded 性

见 §1：`b4ccbe8` 只改 2 份脚本 + 2 份文档，+465/−26，无顺带大重构、无 Rust/Cargo.lock
变更；新增行为集中在 `Remove-Tree`、`Remove-RunScratch`、`Assert-NoRetainedScratch`、
scratch 父目录创建与 4 个回归场景。`09a6fa9` 是大范围 main 合并，但没有改这两份脚本
（blob 逐一比对相同），不影响被评对象。

## 8. 发现清单

**阻断项：无。**

非阻断 comments：

1. **C-4 的显式 `-Scratch` 负向路径缺少自动化回归**；当前由代码守卫（driver:594-595）
   与我的一次性探针覆盖。建议后续补一个小场景固化，但不足以阻断本轮。
2. **早期 closeout 的全工作区 PASS 数字不可在本工作树复核**；其证据目录不存在。本轮 §11
   已明确 NOT RUN，不存在把未跑写成 PASS 的问题，但历史数字的可追溯性仍依赖原证据目录。
3. **Windows MAX_PATH 环境依赖未完全产品化**：深层工作树需要 process-scoped
   `core.longpaths` 或短 `-Scratch`（我自己复跑也复现）。已在 manifest 与
   §11 写明，但门禁在任意深路径下“开箱即跑”仍不成立。
4. **残余 TOCTOU 窗口**（一次属性读与删除之间相隔一个 syscall）由 driver:226-234 与
   closeout §6 明确承认；属于已披露的设计边界，不是本轮新引入缺陷。

## 9. 三项明确回答

1. **是否解除上一轮 CHANGES REQUESTED？——是。**
   C-1..C-4 均已由代码证据、我实际重跑的 fail-first/正控/全量门禁独立确认；建议主会话
   据此解除上一轮 CHANGES REQUESTED。本轮 verdict 为 **PASS WITH COMMENTS**（上述 4 条
   为非阻断 comments）。
2. **是否授权进入 S2？——有条件，本复评不单独授权。**
   安全边界层面的阻塞已清除（离线 driver 113/112 matched/1 BLOCKED、repair 33/33、
   C1 fail-first 双向可复现）。但上一轮 §10 明确要求“完整 CI（含 Linux symlink 覆盖）
   通过后重新提交复评”，而完整 CI 仍在进行；本复评只能把 CI 记为 NOT RUN/待主会话确认，
   不能替 CI 下结论。因此：解除 CHANGES REQUESTED 后，S2 准入以主会话确认完整 CI 通过
   为前提。
3. **R2 admission 是否仍 BLOCKED？——是，仍然 BLOCKED。**
   本次是整改后的复评证据，不是验收；不构成 R2 证据，不授权合并或 admission。

## 10. 进入 S2 前仍需满足的条件

- 主会话确认上游 draft PR #20 的**完整 CI 通过**（含 Linux symlink 覆盖）；在此之前 CI
  只记为 NOT RUN/待确认，不得写成 PASS。
- 主会话基于本报告正式解除上一轮 CHANGES REQUESTED（本复评给出的是建议与证据）。
- 合并集成提交 `09a6fa9` 后，在干净工作区复跑一次本报告的 4 个新增场景与 113 例 driver
  门禁（我的复跑在 `b4ccbe8` 完成；脚本 blob 在 `09a6fa9` 未变，但仍建议在最终合并态确认）。
- 可选（非阻断）：补 C-4 显式 `-Scratch` 负向自动化场景；把 deep-worktree 的 long-path
  处理写进门禁文档。

## 11. CI 状态声明

完整 CI 由上游 draft PR #20 运行中。本报告**未**运行、也**未**看到完整 CI（含 Linux
symlink 覆盖）的最终结果，按要求记为 **NOT RUN / 待主会话确认**，不对其下通过或失败结论。
本报告中我实际运行的只有本地离线脚本门禁（`driver113`、`repair sweeps`、定向 fail-first
与探针）与只读证据核对，均已逐条列出命令与计数器。

## 12. 复评者声明

- 未修改被评代码、未改主工作区、未 push、未 merge、未请求审批、未设置 sandbox 提权。
- 唯一仓库写入：本报告；提交到 `codex/plan0014-wave5b-s1-rereview`，工作区干净。
- 临时重跑产物位于 `%TEMP%\w5b-rereview-indep\`（未纳入仓库）；复评结束后已清理工作树内
  `target/tmp` 测试残留。
