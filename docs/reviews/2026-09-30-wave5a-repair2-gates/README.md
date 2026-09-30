# PLAN-0014 Wave 5A Repair 2 — 候选必须来自固定提交：RED/GREEN、门禁与负例证据

审阅：`docs/reviews/2026-09-30-wave5a-repair1-independent-review.md`（Wave 5A Repair 1 独立审阅，
结论 **NEEDS REPAIR**，findings F1-A / F1-B，指令 §5）。本轮按其 §5.6 交付，停在
**READY FOR REVIEW / NOT INTEGRATED**。承前结论原文照录且未扩大：「Round 3 修复通过；Wave 4
有界收口；既有 C1、R2 阻塞与未执行项继续有效」；Repair 1 轮结论为 READY FOR INDEPENDENT REVIEW /
NOT PUBLISHED，等待的正是这份审阅。

## 1. 提交链与关键 SHA

| 用途 | SHA |
|---|---|
| 修复前基线（Repair 1 代码候选 + closeout） | `77a4fbd2ae89b5cec8ac2e6f482eba8722295387` |
| 审阅起点（承载本轮审阅文档的提交） | `7180eec7fc813b33a748784ac806d5c95b4e359b` |
| 主修复提交（来源派生） | `f7e2af9ce4914ff686e23b66d674a66da3e6a2c7` |
| pilot 证据子目录选项 | `f0c80f727553395d7854135342e5dd0a29d5a852` |
| **代码候选（pilot 与全部 RED/GREEN 的 `source_revision`）** | `c22939f0a51e3c7fa2ec424f3819c6e7113a9b66` |
| 文档 closeout HEAD（本 README 所在提交） | `6c63ba1598d38e71913ad633eb8a46002b35c7d3`（123 个路径全部在 `docs/` 下） |

分支 `codex/plan-0014-wave5a-local-release-pilot`；未 push / merge / tag / 改 main。

## 2. 目录索引

| 路径 | 内容 |
|---|---|
| `00-anchors.txt` | 本轮起点 HEAD、起点 dirty 列表、工具链与 git 版本 |
| `red/` | 修复前（`77a4fbd2…`）的失败回归：9 条 `source_derivation` 用例全 RED，退出码 101 |
| `green/` | 同一命令在候选上的结果（`9 passed`、工具全套 `61 passed`） |
| `gates/` | 审阅 §5.5 每条门禁命令的原始输出与退出码（编号 30–38；39–42 是在最终 HEAD 上对受脚本改动影响的门禁的复跑，33-* 是 workspace 测试被环境阻塞的那两次尝试） |
| `script-regression/` | `scripts/test-plan0014-wave5a.ps1` 本轮报告（14 条，含新增 `R7a/R7b/R8a-c`） |
| `pilot/` | `scripts/plan0014-wave5a.ps1` 在候选 SHA 上的完整归档（生成 / compare / verify / packed crate 构建 / 消费者 / guard / 19 个负例 `negatives/*/case.md` / 候选 JSON `artifacts/`） |
| `F1-closure.md` | F1-A / F1-B 逐项关闭表 + §5.4.4/§5.4.5/§5.4.6 落点 + §5.5 兼容矩阵 + 门禁表 + NOT RUN 表 |
| `ignored-tests-classified.md` | 70 个 `#[ignore]` 的逐项分类与与 Repair 1 的差异 |
| `closeout/` | 文档 closeout 提交之后对同一固定候选的复验日志 |

## 3. 这一轮到底改变了什么（一句话版）

Repair 1 让 `source_revision` 成为一个**可解析的提交**，但候选的字节仍然是从磁盘工作树读出来的，
验证时又只把工件自带的 `module.manifest.json` 重新编译一遍——于是「与提交一致」这件事从来没有
被真正比较过。Repair 2 把候选的唯一来源改成固定提交：成员集合取自 `git ls-tree -r -z`，逐字成员
取自 `git cat-file blob`，派生成员（packed `Cargo.toml`、`module.manifest.json`、契约面、迁移映射）
由该提交**重算并整份比较**；`generate` 在创建输出目录之前先要求 checkout 与提交一致。

## 4. 两模块四类摘要（候选 `c22939f0…` 实测）

| 模块 | contract_digest | package_digest | artifact_integrity_digest | migration_set_digest | 成员 |
|---|---|---|---|---|---|
| `identity` | `3fed976b0ac5e040366f696a1c4a7fe6140bd5bc956996fc8b51a141eba31107` | `b9dfa42ccd09190020c39ee8bf412507de2107810fc2414efdc3415761053c70` | `68deba81c942aaf624f599cd191ef6b09855438b99035cca7010ae4b7d9cc700` | `ef43cfa9a90a367bb9b774834f5b9c150ca67cd27bce65e4016b1c82a062ae03` | 21 |
| `policy` | `52e788ff3187846c4484cc42a3891aa7833f89ff65d666c7b3c7e1af831568c8` | `37e95a19b45c3dcc00e2f4099d91cc72ed63af71f19d769882412af8b2ddd4b6` | `f8add9e0ce13f0c1e33a52eb2f28a85f726c649534e8d10584cdaf4ff6ecb243` | `1a5a43f8b269b3e3624b44a613fa9e498775ad7ccac0cf768018b891168ea597` | 23 |

四类摘要与 Repair 1（`69d9a6b0…`）以及 Wave 5A 首轮（`8036bc65…`）**逐字相同**：本轮改的是候选
怎样被证明，不是打包字节；随提交变化的只有 `source_revision` 一个字段。§5.4.6 要求「规范化内容
相同则摘要不变」在这里是可核对的：`red/README.md` 记录了 RED 阶段各注入的完整性摘要，修复后这些
值不再可能出现在任何候选里。

一处必要的口径修正（已记录理由，未改协议）：`migration-mapping.json` 的 `digest_form` 过去先匹配
原始字节摘要，导致该字段取决于检查者如何 checkout（`core.autocrlf=true` → `raw`），映射摘要随主机
漂移；现在先匹配规范 LF 折叠形式，本轮两份映射全部为 `lf-normalized`，摘要因此回到上表值。
`plan-0014.artifact-integrity.v1`、`plan-0014.migration-set.v1`、`normalize_text_bytes` 的构造一字未改。

## 5. 负例（19 例，全 OK）

`pilot/negatives/<case>/case.md` 每例记录：注入说明、注入实际改动的文件与前后 sha256、两条命令与
退出码、被点名的检查 ID、判定（`OK` / `NOT REFUSED` / `WRONG REASON` / `ARTIFACT CREATED` / `BROKEN`）。
本轮新增：

| 用例 | 断言 |
|---|---|
| `n13-packed-manifest-not-from-the-commit` | packed `Cargo.toml` 改写并**重算完整性记录**后仍被 `derived-members-match-revision` 拒绝 |
| `n14-isolated-checkout-control` | 只含发布输入（26 文件）的隔离 checkout 能生成同一候选（`--repo` 指向它） |
| `n15-checkout-dropped-a-committed-source` | 缺一个已提交源文件 ⇒ `source-member-set-matches-revision`，且不创建工件目录 |
| `n16-checkout-added-an-uncommitted-source` | 多一个未提交源文件 ⇒ 同上 |

## 6. 与 Repair 1 归档的关系

`docs/reviews/2026-09-29-wave5a-repair1-gates/` 一字未动（本轮不覆盖上一轮证据）。Repair 1 的
结论没有被替代：它修好的是 F2–F5 与「`source_revision` 是一个真提交」，本轮在它之上补的是
「候选的字节必须来自那个提交」。Repair 1 的仓库外消费者 lane 仍然只在它的目录里（`3*-outside-*`），
本轮按审阅指令跳过该 lane 并在 §7 记 NOT RUN，不与本轮混计。

## 7. NOT RUN / BLOCKED

见 `F1-closure.md` §7 的逐项表（环境阻塞的 workspace 测试、仓库外 lane、70 个 ignored 的四个分母、
迁移执行、publish/install、扫描、CI、Wave 5B、G-01/G-02/G-201/G-202/G-203）。任何一项都不计入通过。

## 8. 独立审阅栏

| 项 | 值 |
|---|---|
| 审阅结论 | `<pending>` |
| 文档 closeout HEAD | `6c63ba1598d38e71913ad633eb8a46002b35c7d3` |
| closeout 后复验 | 在同一 closeout 提交之后（HEAD 即上一行），对钉住 `c22939f0…` 的同一份候选重新 `verify`：两模块各 `exit 0`，12 项检查全 PASS。日志 `closeout/40-verify-after-closeout-identity.log`、`closeout/40-verify-after-closeout-policy.log`（含两份头部记录 HEAD、候选钉住的 SHA，以及该提交只含 `docs/` 路径这一点）。 |

工具不要求 `source_revision` 等于当前 HEAD：它要求那个提交存在、且工件的每个输入都等于**那个提交**里的
内容。因此「closeout 只加文档」不会让旧候选失效，而「改了来源却不提交」会立刻失效——后者正是本轮
`n15` / `n16` 与单元回归证明的方向。

实现者不自行填写 PASS / accepted。状态：**WAVE 5A REPAIR 2 READY FOR REVIEW / NOT INTEGRATED**。