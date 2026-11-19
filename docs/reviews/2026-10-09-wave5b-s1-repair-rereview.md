# Wave 5B-S1 修复再次独立复审

日期：2026-10-09。结论：**NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**。

## 固定审阅对象与范围

- 分支：codex/plan-0014-wave5a-local-release-pilot。
- 代码候选：a90cf3277356471511647fbee9b507716c8c81bb。
- 收尾文档 HEAD：0f295731d4cfc5f57cd8638d5a1fd2ca234f0caa。
- 增量基线：840f5a0befcc6aaea815edc244e03835fdd28bf4；前轮代码候选为 5fa0ff46ed29e7ed0dfdebf01f3542779b3e3a22。
- 主线程审阅，独立 shared clone detached 在代码候选运行。实现方 19/19 与 workspace 911 的结果单独审计，不计为本轮独立运行。
- OCR delegate 用于确定文件和规则。增量 55 文件，5 个代码/脚本全部审阅；49 个 fixture/expectation 文件结合生成器、共享期望和两条读取路径核对；1 个既有独立报告按文档核对；另核对本轮最终收尾报告。
- 未修改候选生产代码；未 push/merge/部署、连接数据库或推进 S2。未读取或执行无关 OPERATIONAL_RUNBOOK.md / clean-target.ps1。
- 适用约束：Revision 1 独立审阅与 S1 指令 §3.8 要求 cleanup 只删除本轮拥有的临时目标，不删除证据或外部路径。源代码中的“非递归删除”本身不能证明该边界。

## Q1 — P1：祖先检查在登记 Parent 处停止，遗漏上层 junction

位置：scripts/test-plan0014-wave5b.ps1:98–120，特别是 111–113；注册与清理均以登记 Parent 为 Boundary。

真实反例：先在本轮唯一临时目录中创建 anchor/scratch/run-owned 并登记，再移走 anchor，将 anchor 替换为指向本轮自建 outside-owned-test 的 junction。Root 与 Parent 本身均不是 reparse point，只有 Parent 上层的 anchor 是。Get-UnsafeLink 返回空字符串；调用实际 Remove-RunScratch 后，外部目标下哨兵被删除，停放的原始 root 仍存在。

证据：2026-10-09-wave5b-s1-rereview-gates/ancestor-link-probe.ps1 与 .log。日志 reported_unsafe_link=""、sentinel_survives=false、original_parked_root_survives=true。

影响：字符串登记边界仍指向相同名字，却不再指向原来的物理目录，故 N2 的 parent 场景不足以覆盖祖先被换走。必须将登记与清理绑定到可信物理锚点并校验完整路径链；不能让登记 Parent 自己成为无需证明的信任起点。

## Q2 — P1：目录入栈后的替换绕过子目录属性检查

位置：scripts/test-plan0014-wave5b.ps1:195–205，特别是 197、202。

子目录枚举时的 DirectoryInfo 属性通过检查后，只将 FullName 字符串入栈。出栈直接 Get-ChildItem，没有重新验证目录身份、reparse 属性或祖先链。若该名称在两次访问之间被替换为 junction，枚举与文件删除会落到外部目标；最后 Directory.Delete(path,false) 不会挽回已删除的文件。

证据：queued-directory-link-probe.ps1 与 .log。脚本从候选 AST 提取实际 Remove-Tree，仅对 Get-ChildItem 的枚举返回边界注入一次替换，保持候选删除函数原样。日志 swapped_after_enumeration=true、sentinel_survives=false、original_parked_child_survives=true。

证据性质：确定性的调度边界注入，操作真实 NTFS junction 和本轮自建哨兵；不是自然并发压力测试，不据此宣称已统计实际发生频率。它证明存在真实的检查后使用窗口，当前静态 child-link 场景没有覆盖。

修复应在出栈、枚举以及删除前重新核验身份与完整链，并保证不跟随换入的链接。只增加一次属性读取可缩小窗口，不能自动声明完全消除 TOCTOU；最终方案必须说明无跟随/稳定身份的保证，无法证明时保留对象并具名拒绝清理。

## Q3 — P2：新增 repair fixture 会删除此前运行的全局 scratch

位置：scripts/test-plan0014-wave5b-repairs.ps1:168–188，特别是 169、184–186。

Get-ReparseCapableScratch 优先使用固定的系统临时目录 wave5b-reparse-probe，scenario 名称也固定。发现同名场景后直接 Remove-Item -Recurse -Force。新 OutDir 不能隔离这条路径；旧运行证据/哨兵或并发运行数据可被清除，junction-target/junction-link 名称也共享。

独立反例：使用唯一 scenario-independent-GUID 避免碰触已有真实场景，在同样全局 base 预先创建哨兵，再调用实际 AST helper。返回同一个已有 root，哨兵消失。证据 global-scratch-probe.ps1 与 .log：reused_preexisting_root=true、sentinel_survives=false。

修复应使用每次运行唯一的 capability probe 根与场景根，登记本轮创建与拥有的对象；既有路径不得自动删除。finally 只清理本轮对象，保留 OutDir 证据，能力不足仍具名 BLOCKED。

## 修复项判断与独立验证

| 项目 | 本轮判断 |
|---|---|
| N1 SQL 完整表列表、alias column list、JOIN ON 后逗号引用 | 本轮覆盖通过；旧反例不再绕过，新增 16 fixture 纳入共享期望 |
| N2 删除安全 | 部分修复，Q1/Q2 未关闭；新增测试还有 Q3 |
| N3 raw identifier 与完整 use item | 本轮覆盖通过，raw alias / raw import 旧反例拒绝正确 |
| N4 Rust 字面量词法屏蔽 | 本轮覆盖通过，双引号字符旧反例不再遮蔽后续 embed |
| N5 unknown-only 与场景计数 | 本轮覆盖通过；未知过滤具名拒绝，有效过滤计数正确 |

本轮独立运行：

- module-release-tool --all-features：12 suites，108 passed，0 failed，0 ignored；crate-tests.log。
- 真实 offline driver：113 declared，112 matched，1 BLOCKED，0 mismatched；driver113.log 与 driver113/。**不得写 113/113 全绿**。
- repair driver：19 declared，18 executed/pass，1 SKIP（entry-r4-internal-default-lifecycle），0 failed，0 blocked；repair-scenarios.log。**本轮未独立重跑该 internal-default 场景，不写 19/19**。
- 11 个旧 SQL/embed 反例及正负对照重放符合预期；catalog-embed-probes.json。它们使用实际候选 CLI，不执行 SQL。
- 新增 3 个删除反例均复现；所有外部目标与哨兵都是本轮自建目录。
- cargo fmt --all -- --check：PASS，fmt.log。
- scripts/check-architecture.ps1：串行重跑 PASS，architecture-serial.log。初次与 driver 共用构建目录时遇到 module-release-tool.exe 文件锁失败（architecture.log）；这是本轮调度干扰，保留失败记录，不归因候选代码。
- pinned history 生成器：重新生成与 checked-in fixture 字节相同，SHA256=3FFA3E9633097AA85022D310E835AD9BF377B4970B7D3C17A34E117413DC80BB；pinned-generator.log / pinned-regenerated.json。

实现方证据审计：

- 明确读取 gates-fast/heavy/post/gates/ 下的真实门禁，排除 repair 场景内同名 stub 日志。22 个门禁退出码符合各自预期，delivered-gate-audit.json。
- 真实 gates-post/gates/37-test-workspace.log 汇总为 192 suites / 911 passed / 0 failed / 70 ignored；候选 anchors 指向 a90cf3。此为提交方日志审计，不是本轮独立 workspace 测试。
- 本轮独立全 workspace check/clippy/test、70 ignored、外部服务、安全扫描：NOT RUN。
- P2-catalog-symlink：本轮真实驱动仍因管理员权限 BLOCKED。junction 反例已在可创建 junction 的 NTFS 临时目录真实运行，不混同该限制。

## 给 zcode / Qwen3.8 Flash 的限定修复合同

只修 Q1–Q3，不重开 SQL/Rust parser，不推进 S2。主修改范围：

1. scripts/test-plan0014-wave5b.ps1 的路径登记、链校验与清理函数。
2. scripts/test-plan0014-wave5b-repairs.ps1 的隔离 scratch helper 与新增回归。
3. 本轮新报告与证据文件。若需要新增共享清理辅助文件，应先解释其必要性与调用边界，禁止顺手修改迁移、manifest、runner 或业务 crate。

按顺序实施：

1. 固定当前代码 SHA、分支与 dirty 状态；保护其他人文件。将本报告 3 个 probe 改成独立失败回归，先在未修复候选证明外部哨兵会丢失。保留当前真实 AST/真实 cmdlet，不能以修改 stub 返回值伪造修复。
2. 修 Q1：明确可信物理锚点、登记身份与完整祖先链；覆盖注册前已存在的上层链接，以及登记后 Parent 上方不同深度的祖先换入链接。禁止仅将 Boundary 向上一层移动。
3. 修 Q2：覆盖队列入栈后、出栈/枚举前、枚举后/文件删除前的替换。遇到身份或链变化 fail closed，不能访问/删除外部目标。对无法原子保证的路径操作明确保守保留；不能用“非递归 API”或静态 link 属性测试代替安全证明。
4. 修 Q3：所有 probe/target/link/scenario 路径按唯一 RunId 隔离；创建前遇既有路径拒绝或选择新名字，绝不先删旧树。补同名旧目录、重复运行、两轮并行的哨兵不变测试。
5. 保留正常 owned-root 清理、内部 evidence 保留、KeepScratch、受保护路径/大小写/分隔符拒绝、异常 finally 与 root/parent/static child junction 场景。所有外部哨兵验证存在且字节 checksum 不变，同时校验普通目录正对照真的清除，防止一律不清理造成假通过。
6. 每个新增场景进入 AST scenario manifest；验证 unknown-only 拒绝和 declared/executed/skipped/blocked 计数。能力不足仍 BLOCKED，不通过 stub 把实际能力检测抹掉。
7. 串行运行完整 repair driver、113 offline driver、module-release-tool all-features、fmt、architecture 及仓库要求的 workspace gates；新代码变化后重跑相关门禁。每条绑定最终代码 SHA。Windows 不并发使用同一构建目录中的正在执行 exe。
8. 交付逐项 Q1/Q2/Q3 对账、失败先行证据、正负对照、字节不变断言、独立目录位置与最终 SHA。明确 NO push/merge/deploy、R2 BLOCKED / NOT CLAIMED；完成后停止，等待独立复审。

复审上限：当前可认可 N1/N3/N4/N5 的本轮受检修复和记录的独立门禁；**删除安全未通过，不能声称全部修复已通过独立复审或已获集成接受**。
