# Wave 5B-S1 最终修复候选独立复审

日期：2026-10-08（Asia/Shanghai）。结论：**NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**。

多项修复有效，原始门禁数字成立；但本轮新增 **2 项 P1、3 项 P2**，原 R2、R4、R5 仍未闭合。实现者“全部完成”不能作为独立接受结论。

## 1. 固定范围与工作区差异

起点：87e8a318fa32ff43d41ad45da8a0f83b3b19a2a4。
原代码提交：6c612722a841a7d0be5d5d56569de55e7fe4b9bf。
本轮固定代码候选：**5fa0ff46ed29e7ed0dfdebf01f3542779b3e3a22**。
收尾报告提交/当前源 HEAD：840f5a0befcc6aaea815edc244e03835fdd28bf4。
分支：codex/plan-0014-wave5a-local-release-pilot。

开始复审时四个文件另有未提交增量：catalog.rs、主 driver、repair driver、pinned-history generator，共 19 insertions / 5 deletions，涉及诊断文案、相对路径失败、未知 Only 参数拒绝和 forms 统计。不能凭文件存在推断作者或修改时间；本报告不归因，也不把它们算入固定候选。

在 target/wave5b-final-independent-20261008/candidate/ 建立独立 clone，detached 于 5fa0ff4。生产代码构建、测试和 AST 提取均使用该副本，原工作区保持原状。身份见 [candidate-file-identities.json](2026-10-08-wave5b-s1-final-independent-gates/candidate-file-identities.json)，差异见 [working-tree-drift.patch](2026-10-08-wave5b-s1-final-independent-gates/working-tree-drift.patch)。

采用 open-code-review-delegate；按仓库指令主线程顺序审阅。OCR 75 files，reviewable 7、excluded 68；7/7 reviewed、0 skipped、coverage 100%。excluded 的 64 个 fixture/expectation 文件通过生成来源、共享 expectation、Git blob 摘要及两个读取者补读/对账；4 份报告/交接作为规格补读。另读取 840f5a0 收尾报告。见 [review-coverage.json](2026-10-08-wave5b-s1-final-independent-gates/review-coverage.json)、[fixture-audit.json](2026-10-08-wave5b-s1-final-independent-gates/fixture-audit.json)。

## 2. 必须修复的发现

下述行号指固定候选 5fa0ff4。契约依据为原 S1 指令 §3.5：无法可靠解析的引用/未支持语法不能成功；§3.7：无法确定的 embed 写法不得静默跳过；§3.8：finally 清理不得删除外部路径。

### N1 [P1] 表列表在列别名、JOIN 条件、继承星号之后漏掉外部表

位置：tools/module-release-tool/src/catalog.rs:1918-1955,2087-2093。对应原 R2。

table_list_events 消费一个 alias 后，仅在下一个 token 立即为逗号时继续；遇到 alias 的列名括号、JOIN 的 ON 条件或 PostgreSQL 表继承标记 * 就停止。body scan 不恢复逗号列表，后续表没有 owner 检查。以下四条分别被错误判定 exit 0 / VALIDATED：

    INSERT INTO roles (role_id)
    SELECT r.id FROM roles AS r(id), platform_users p;

    INSERT INTO roles (role_id)
    SELECT r.role_id FROM roles r
    JOIN role_permissions rp ON rp.role_id = r.role_id, platform_users p;

    DELETE FROM roles USING roles AS r(id), platform_users p;

    INSERT INTO roles SELECT r.* FROM roles * r, platform_users p;

它们都是 policy 对 identity-management 的跨 owner 引用。普通 FROM roles r, platform_users p 对照正常 exit 1；同 owner 对照 exit 0。manifest 为 LF 字节，错误不来自摘要或启动失败。

修复必须有明确语法和结束边界：支持则完整消费列表/连接，无法支持则具名拒绝，不能半解析后成功。四种各自补共享负例及同 owner 对照。见 [catalog-embed-probes.json](2026-10-08-wave5b-s1-final-independent-gates/catalog-embed-probes.json)。

### N2 [P1] 登记根替换为 junction 后 cleanup 删除根外内容

位置：scripts/test-plan0014-wave5b.ps1:64-73,103-132,402-435。对应原 R4。

Get-CanonicalPath 用 Resolve-Path，返回 junction 的逻辑路径。cleanup 比较逻辑父目录，没有拒绝 run 根本身的 ReparsePoint；Remove-Tree 对这个根递归枚举，进入 junction 实际目标并删除其文件。

独立探针在唯一 OS temp 目录中登记 registered-parent/run-owned-test，然后将该空根替换为 junction，指向本探针自建的 outside-owned-test-target（位于登记父目录外）。原生产函数执行结果：

    attributes = Directory, ReparsePoint
    canonical = <registered-parent>/run-owned-test
    sentinel_survives = false
    root_survives = false
    messages = []

正常/fallback 清理通过不证明替换边界安全。探针只操作本轮自建目录，没有读取或删除用户数据。

删除前必须验证根自身与祖先链的 reparse 状态/真实归属，拒绝或安全断开跳转；不得先递归枚举被替换根。补 root replacement、parent junction 及根外哨兵字节不变测试。见 [cleanup-root-junction-probe.ps1](2026-10-08-wave5b-s1-final-independent-gates/cleanup-root-junction-probe.ps1)、[cleanup-root-junction-probe.log](2026-10-08-wave5b-s1-final-independent-gates/cleanup-root-junction-probe.log)。

### N3 [P2] raw identifier import 半解析，额外 embed 静默放行

位置：tools/module-release-tool/src/catalog.rs:3395-3446,3535-3543。对应原 R5。

word_at 不消费 r# 前缀。alias r#embed 被绑定为 r，调用中的 embed 不在 bindings；sqlx::r#migrate 被识别为路径尾 r，余下 token 没有拒绝。以下两种分别作为第二 embed 时均 exit 0 / baseline_embed_points=1：

    use sqlx::migrate as r#embed;
    static M: sqlx::migrate::Migrator = r#embed!("../../migrations");

    use sqlx::r#migrate as embed;
    static M: sqlx::migrate::Migrator = embed!("../../migrations");

同形本地 macro 词法/名字绑定控制经 rustc --edition 2021 --crate-type lib 编译成功，排除了非法 Rust 拼写；它不是 SQLx runtime 接受证据。普通 alias 对照正常发现两处 embed。

完整识别 raw identifier，或具名拒绝无法消费的 use tree，不能忽略尾部。见 [catalog-embed-probes.json](2026-10-08-wave5b-s1-final-independent-gates/catalog-embed-probes.json)、[rust-lexical-controls.log](2026-10-08-wave5b-s1-final-independent-gates/rust-lexical-controls.log)。

### N4 [P2] 双引号 char 被当字符串起点，遮蔽后续宏调用

位置：tools/module-release-tool/src/catalog.rs:3134-3152,3786-3843。对应原 R5。

掩码留下 char literal，却把里面的双引号当字符串开头；前置 comment stripping 也没有 char 状态。以下合法源码的后续 import/call 因此被遮蔽：

    const QUOTE: char = '"';
    use sqlx::migrate as embed;
    static M: sqlx::migrate::Migrator = embed!("../../migrations");

作为第二 embed 时 exit 0 / baseline_embed_points=1；普通 alias 对照 exit 1，同形 Rust 编译 exit 0。

统一处理 comment、普通/raw/byte string、char/byte char 与 lifetime 的词法边界，无法判定时具名失败。补双引号 char、转义 char、lifetime 与真实调用组合测试。见上述 JSON 与 lexical controls。

### N5 [P2] 未知 Only 参数零场景执行仍打印成功

位置：scripts/test-plan0014-wave5b-repairs.ps1:54,62-65,519-527。

固定候选真实执行 -Only NONEXISTENT-SCENARIO：全部 14 项 SKIP，exit 0，结尾 every scenario matched its expectation。参数拼错会制造零分母的成功。

执行前校验筛选名称、拒绝未命中/零执行，并报告 executed/skipped/failed。相关修补当前仅在源工作区，未进入 5fa0ff4，需要固定新候选并补真实参数负例。见 [unknown-only.log](2026-10-08-wave5b-s1-final-independent-gates/unknown-only.log)。

## 3. 原 R1–R7 本轮判定

| 原项 | 独立判定 | 证据/限制 |
|---|---|---|
| R1 | PASS（离线） | rename reserved 修复、真实固定 blob 对账测试、两读取者通过 |
| R2 | PARTIAL / NEEDS REPAIR | 原 alias/AS/USING 反例拒绝；N1 仍放行 |
| R3 | PASS（正常/异常入口） | 精确 Label 参数正确；显式 EvidenceDir 真正跑 97 案例；非空目录按 evidence 规则拒绝 |
| R4 | PARTIAL / NEEDS REPAIR | 正常 main/fallback 删除、内部 evidence AST、KeepScratch 和旧哨兵通过；N2 仍删根外内容 |
| R5 | PARTIAL / NEEDS REPAIR | 普通 bare/alias/group/空白形式通过；N3/N4 仍漏扫 |
| R6 | PASS（有限子集） | USING btree 正例；未知 method/INHERITS 具名拒绝；JOIN USING 实际 fixture 预期为 unsupported 拒绝，不是语法已支持 |
| R7 | PASS（stub/真实参数） | Join-Path 单一参数正确，空格路径捕获通过，真实 driver 成功 |

B5/B6/B7/B8 在本轮对应离线/fixture/generator/wrapper 范围内保持有效，不能推导生产接受。

## 4. 交付证据核对

**22 个 gate**（fast 9、heavy 6、post 7）actual 均符合 expected。原 workspace 日志独立重算 **192 suites / 901 passed / 0 failed / 70 ignored**。这是核验实施者原始执行，不是本轮独立重跑 workspace。原日志摘要见 [delivered-gates-audit.json](2026-10-08-wave5b-s1-final-independent-gates/delivered-gates-audit.json)。

post gate 54 锚定 6c6127，gate 53 当时无 tracked dirty。5fa0ff4 仅修改 repair driver 断言，报告另声明终版 driver 重跑；可保留这个证据分层，但下一轮应统一最终候选。

收尾报告另需澄清：

- “旧独立报告 uncommitted as before”不准确：两份旧独立报告已经在 6c6127 加入 Git；内容未改不等于未提交。旧 evidence 目录仍未跟踪。
- “nothing required is unrun”不得暗示真实服务和安全扫描通过。70 ignored 的真实 DB/对象存储/provider 没有执行，报告没有 Secret/漏洞/许可证/镜像扫描原始证据；按适用范围明确 NOT RUN/不适用，保持离线 S1 与生产/发布接受边界。

## 5. 本轮独立运行

| 检查 | 本轮结果 |
|---|---|
| 完整 module-release-tool，专属 target | exit 0；12 suites / **98 passed / 0 failed / 0 ignored**；wave5b_catalog 20 passed |
| cargo fmt --all -- --check | exit 0 |
| check-architecture.ps1 | exit 0；现有静态规则不覆盖 N1–N4 |
| 真实 97-case driver，显式 EvidenceDir/Scratch/TargetDir | exit 0；**96 MATCH + 1 BLOCKED，0 MISMATCH** |
| repair driver 精确筛选 13 项 | 13 PASS；内部默认真实入口 entry-r4-internal-default-lifecycle 本轮 SKIP |
| 固定 history generator，新的临时 Out | exit 0；fixture 字节相同；20 entries；SHA-256 3FFA3E9633097AA85022D310E835AD9BF377B4970B7D3C17A34E117413DC80BB |
| SQL 额外反例与对照 | 4 个跨 owner 反例错误 exit 0；普通 foreign 对照 1，同 owner 对照 0 |
| embed 额外反例与对照 | raw alias/raw import/quote char 三个反例错误 exit 0；direct/普通 alias 对照 1 |
| junction replacement | 根外自建哨兵被删除 |
| unknown Only | 全部 SKIP 却 exit 0 |
| 同形 Rust 词法控制 | rustc exit 0；仅证明源码拼写/绑定形式合法 |

固定候选 generator 终端仍打印 forms: lf=19；实际 JSON 是 **12 LF / 7 CRLF SQL + 1 manifest**。终端计数修正只在未提交工作树，以 JSON 对账为准。

独立真实 driver 正常 main 与 fallback 均删除，外部 evidence 保留；这是正常路径，不是 N2 替换边界。P2-catalog-symlink 本轮同样因管理员权限要求具名 BLOCKED，没有算 PASS。

本轮 workspace check/clippy/test、完整 fast/heavy/post wrapper、内部默认真实入口、70 ignored、真实 DB/provider、发布/恢复、安全扫描：**NOT RUN**。实施者原始日志已核验的范围单列 §4，不冒充独立执行。

所有新证据位于 [final-independent-gates](2026-10-08-wave5b-s1-final-independent-gates/)。SQL 只被静态扫描，没有执行。首次反例脚本误写 CLI 为 module-release.exe，启动前断言阻断；修正为 module-release-tool.exe 后执行全部输入。该启动失败不计入 SQL/宏判据。

## 6. 后续修复与停止边界

围绕 N1–N5 补有界修复与失败回归，保留有效修复和旧证据；不再逐字符串补特判。SQL 列表须完整解析或拒绝；删除须真实归属与 reparse 安全；Rust 扫描须统一词法并拒绝半解析 import；repair driver 须证明明确执行分母。

对账当前四个 dirty 增量，固定新本地候选，复跑受影响 focused/真实入口、两个读取者及规定完整门禁，提供准确收尾报告，再独立复审。

本次只新增独立报告、证据、临时副本。没有修改实现、提交、push、merge、连接数据库、接受 ADR 或推进 S2。结论仍为 **NEEDS REPAIR / NOT INTEGRATED；R2 BLOCKED / NOT CLAIMED**。
