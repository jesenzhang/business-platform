# Wave 5B-S1 Q1–Q3 修复独立复审

日期：2026-10-09。结论：**NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**。

## 固定候选与覆盖

代码候选：6bcdd89d14d188f62e99bbd068cf786b8207699b。
文档 HEAD：53ef8b289f2a1bf11b108cb5504d90f21153ebc8。
基线：0f295731d4cfc5f57cd8638d5a1fd2ca234f0caa。
分支：codex/plan-0014-wave5a-local-release-pilot；开始时无已跟踪改动。

OCR delegate preview：total_files=4，reviewable_files=2，reviewed_files=2，skipped_files=0，代码覆盖率 100%。两个脚本已审阅；两个排除的 Markdown 文件也作为审阅要求与收尾声明核对。生产文件 SHA256 见 reviewed-source-sha256.json。独立运行使用 target/w5b-q123-review 下 detached 于代码候选的 shared clone。

适用要求：S1 独立审阅指令 §3.8，cleanup 只能清理本轮拥有的临时目标，不得删除外部路径或证据。未修改候选代码、未 push/merge/部署、未运行数据库或外部服务、未推进 S2。无关 OPERATIONAL_RUNBOOK.md 与 clean-target.ps1 未读取或执行。

## Q4 [P1] — 最后一次检查后换入 junction，文件已删才报告保留

位置：scripts/test-plan0014-wave5b.ps1:296–310，特别是文件分支 309–310。

本轮在实际候选 AST 导入的 Remove-Tree 上复现：枚举 run-owned/shared.txt，完成枚举后的第二次路径检查，随后将 run-owned 移开并在原处建立指向本轮自建 outside-owned-probe 的真实 junction。两个目录内有同名 shared.txt。

探针仅包裹 Test-ReparsePoint 的真实实现，在真实属性读取完成后注入一次替换，再返回该次真实读取的结果；没有伪造“安全”返回值，没有修改 Remove-Tree 的删除实现。随后 DirectoryInfo.Attributes setter / Delete 仍通过旧路径访问目标文件。文件被删除后，最后的目录检查才发现 junction 并抛出保留拒绝。

证据：2026-10-09-wave5b-s1-q123-independent-gates/last-check-race-probe.ps1 与 .log：

- swapped_after_post_enumeration_check=true
- sentinel_survives=false
- original_parked_file_survives=true
- thrown="Remove-Tree retained 1 object(s) … is itself a reparse point …"

证据性质：确定性调度边界注入，使用实际 NTFS junction、实际文件删除和本轮独占的测试目录；没有声称运行自然并发压力测试或测得发生概率。

现有两个 Q2 测试将替换安排在检查之前，因而能通过；本探针把替换安排在新增的最后一次检查之后，证明窗口仍可导致外部内容损失。非递归 Directory.Delete 对目录链接名称的处理，不能证明其子文件的绝对路径删除不会跟随祖先链接。

收尾报告 §6 “残余代价仅一个目录名，never external content”及源码 215–220 的无外部风险保证，与复现结果冲突。不是仅仅“未测”的声明限制，而是受检保证已被反例否定。

修复要求：文件的属性变更、打开与删除都必须绑定到不会被祖先重定向的稳定目录/文件身份。增加更多路径属性检查只能缩小窗口，不能据此宣称已关闭。采用可论证的稳定句柄/无跟随机制，或在无法证明时保守保留并明确拒绝清理；禁止以检查后重新使用绝对路径来替代该保证。必须补最后一次检查返回后、文件操作前的失败回归与外部哨兵 checksum 不变断言。

## Q5 [P1] — 普通文件分支在祖先链检查之前直接删除

位置：scripts/test-plan0014-wave5b.ps1:225–233；完整祖先检查直到 268–278 才执行。

静态反例，无竞态：创建真实 junction linked-parent 指向本轮自建外部目录，在该目标写 sentinel.txt，然后调用实际 Remove-Tree(linked-parent/sentinel.txt)。最终叶子是普通文件，自身无 ReparsePoint；函数走 231–233 的 Remove-Item 分支并直接返回，完全不检查 linked-parent。

证据：file-ancestor-probe.ps1 与 .log：sentinel_survives=false，thrown=""。没有替换检查函数、没有调度 shim，只有真实文件和真实 junction。

该函数确实用于文件清理（例如内部 evidence 清理会逐子项调用）；新增 ast-remove-tree-linked-ancestor-retains-tree 只覆盖目录路径，没有覆盖文件路径。修复把完整链检查放在目录分支之后，留下同一删除边界在文件入口处的缺口。仅修登记根的静态祖先反例不足以关闭整个清理 surface。

修复要求：统一所有叶子类型的进入校验与安全删除策略；文件、目录、链接均须检查所经过的祖先链。叶子自身为链接时，也须先证明其父链不通向外部对象再进行 unlink。增加普通文件经 junction 祖先、目录经 junction 祖先、正常 owned-file 删除的正负对照。

## 原 Q1–Q3 对账

| 原项 | 本轮结果 |
|---|---|
| Q1 登记 Parent 上层静态 junction | 原始探针通过；链延伸到卷根后能检测 anchor；仍存在 Q5 的普通文件入口缺口 |
| Q2 排队目录换入 junction | 原始探针具名拒绝，外部哨兵保留；最后检查后的文件操作窗口仍有 Q4 |
| Q3 全局固定 scratch 跨轮删除 | 原始探针通过，reused_preexisting_root=false、sentinel_survives=true；重复/并行隔离独立通过 |

原探针脚本与日志按 old-* 保存在本轮证据目录。queued-directory 探针只增加 try/catch 以记录修复后具名异常并继续输出哨兵状态，未改变其操作逻辑。

## 独立验证与证据边界

证据目录：docs/reviews/2026-10-09-wave5b-s1-q123-independent-gates/。

- 完整 repair driver：退出 1；declared=29、executed=29、skipped=0、failed=1、blocked=0；28 PASS。repair29.log。
- 唯一失败 entry-r4-internal-default-lifecycle 的子进程在 scratch history clone checkout 报 Filename too long，未达到全部家族与最终成功验收；属于本轮复审克隆路径长度限制，不能写独立 29/29。entry-r4-child-output.txt 保留完整错误；该失败路径中的内部 evidence 保留日志可见，但不等于完整成功生命周期已通过。
- 上述内部模式子进程完成 catalog 期望读取后停在 history checkout；本轮没有独立完成全部 113 场景，明确 NOT COMPLETED。
- Q3 的两次顺序子进程、两个真实同时运行子进程均 exit=0，scratch/base/run id 不同，外部哨兵字节与各自清理断言通过；helper-scratch-isolation-report.txt 及 4 个 child 日志。
- 三个旧反例重放通过；两个新增删除反例真实复现。所有外部目标均为本轮自建目录，没有触碰用户数据。
- 本轮独立全 workspace check/clippy/test、crate 108 项、fmt、architecture、ignored/external services/security scans：NOT RUN；Rust 与 Cargo.lock 未改，因此没有重复运行上一轮 crate 测试。本轮以 PowerShell 清理与 isolation 回归为主。
- 实现方真实 gates-fast/heavy/post/gates 下 22 条日志的 actual/expected 退出码全部匹配；delivered-gate-audit.json。没有把 repair 场景的 wrapper stub 日志计入真实门禁。
- 实现方真实 workspace 日志核算：192 suites / 911 passed / 0 failed / 70 ignored。候选 anchors 指向 6bcdd89。提交方 29/29 与 113 中 112 matched/1 BLOCKED 有相应日志，但属于实现方证据，不计作本轮独立通过。
- P2-catalog-symlink 沿用提交方 BLOCKED 状态，本轮未重测文件 symlink 权限；不混同已在 NTFS 上真实复现的 junction 删除问题。

## 给 zcode / Qwen3.8 Flash 的下轮限定修复指令

只修 Q4/Q5 及对应安全声明，保留 Q3 隔离与原始正负对照。不重开 SQL/Rust parser、不改迁移/manifest/业务 crate、不推进 S2。

1. 固定当前 SHA 与 dirty 状态，将本轮两个 probe 先转为失败回归；保持真实候选函数、真实文件系统操作，禁止伪造返回值让测试变绿。
2. Q5 先补所有叶子类型的祖先链校验，证明静态链接路径不能进入文件删除分支；正常本轮文件仍能删除。
3. Q4 明确实际文件操作的身份稳定机制。写出从枚举到属性变更、删除的保证，说明父路径换入链接时为何不能转到外部目标。无法证明就保留并具名拒绝；不要把重新读取属性当作原子 no-follow 删除。
4. 至少覆盖最后一次检查后/属性赋值前、属性赋值后/删除前、根以上祖先变化、普通文件经已有链接四种边界。外部对象存在、SHA256 和 item count 均不变，同时保留普通 owned-root/file 真删除的正对照。
5. 保留 Q1 原始静态祖先、Q2 排队与枚举后替换、Q3 同名旧目录/顺序/并行隔离，内部 evidence、KeepScratch、受保护路径与异常 finally。
6. 修正源码与收尾报告对 no-follow、稳定身份及“外部内容永不受影响”的保证，逐条对应实际实现与测试；未覆盖能力明确 NOT RUN/BLOCKED。
7. 最终代码 SHA 下串行运行 repair 全量、真实 113 驱动及仓库门禁。修复前后的证据不能混用；Windows history clone 使用足够短的自建工作路径，或仅对本轮进程启用经过记录的 longpaths 配置，不改用户全局 Git 配置。
8. 交付 Q4/Q5 逐项对账、失败先行与正负对照、外部哨兵字节不变证明、门禁计数及最终 SHA。完成即停止；不 push/merge/deploy，R2 BLOCKED / NOT CLAIMED。

本轮结论上限：原始 Q1–Q3 反例已被阻断，Q3 跨轮隔离在本轮受检范围通过；清理安全仍存在两个 P1，不能宣称全部修复已通过独立复审。
