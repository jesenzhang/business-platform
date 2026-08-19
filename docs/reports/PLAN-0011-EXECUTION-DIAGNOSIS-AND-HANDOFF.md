# PLAN-0011 执行诊断与 Handoff

Document ID: `REPORT-PLAN-0011-EXECUTION-HANDOFF`

State verification: 2026-08-19, after final-status CI Run `32216138288` for
implementation/closeout state commit `3a42856cda587b205f8927c1d06e3aa5f532692d`
completed successfully. This report is a subsequent docs-only handoff record.

Status: `PLAN-0011 = Integrated / Archived / Closeout CI PASS / Branch Cleanup DONE`.
This report records the execution diagnosis and handoff evidence.

> 本文件曾在候选阶段保持未跟踪，避免改变 `ed870ac` 的 exact review HEAD；现已
> 随 closeout 文档提交纳入 `main`，并由最终状态 CI 验证。

## 1. 结论先行

PLAN-0011 长时间未完成的直接原因不是编译器、GitHub CI 或 PostgreSQL/MinIO
基础设施持续卡死，而是候选在自动化门禁通过后仍被独立 REVIEW-C 发现架构
级阻塞，随后进入了多轮 bounded repair。历史 reviewed HEAD `f945a53` 的 4 个
HIGH 已在 `ed870ac` 中完成 bounded repair；新的 exact-head CI 和 fresh
REVIEW-C 均已通过。候选随后已 fast-forward 集成到 main，Main CI、Closeout CI
和最终状态 CI 也已通过；最终 ancestor/clean 核验和 feature branch cleanup 均已完成。

当前有效状态快照是：

```text
implementation/closeout state = 3a42856cda587b205f8927c1d06e3aa5f532692d
main at final-status verification = 3a42856cda587b205f8927c1d06e3aa5f532692d
PLAN-0011 implementation base = 31b24c6993dbff1f3e88b2476e0c87460400ec31
latest candidate HEAD = ed870acfe165756632c0519bb181fd5dcf8a11cd
latest candidate local gates = PASS
latest candidate CI = Run 32210387950 / PASS (retry after cancelled attempt)
fresh REVIEW-C for ed870ac = PASS (read-only reviewer Rawls; no actionable findings)
previous reviewed HEAD f945a53 = historical FAIL / 4 HIGH, repaired by ed870ac
main integration = DONE (fast-forward to ed870ac; closeout commits follow)
Main CI = Run 32213985080 / PASS
PLAN archive = DONE locally and in closeout commit
Closeout CI = Run 32214911706 / PASS
feature branch cleanup = DONE (local and remote deleted)
final-status CI = Run 32216138288 / PASS (all jobs, head 3a42856c)
state verification working tree = CLEAN
```

本报告提交本身只增加诊断和 handoff 文档，不改变已接受的 PLAN-0011 实现状态。
报告不自引用其未来 commit SHA；提交后的当前 `main` 应是 `3a42856` 的 docs-only
后代，并以 Git 状态和远端 CI 作为可验证事实。

长耗时是一个控制流程放大问题，而不是单一慢步骤：

1. 一个 Goal 把架构整合、计划激活、多个实现阶段、评审、CI、main 集成、
   closeout 和分支清理串成了一个终端链路；
2. 高风险语义（生命周期规划、依赖图、序列化完整性、贡献身份）在大范围
   实现完成后才由最终评审集中发现；
3. 候选证据曾采用不可实现的“提交内记录自己的 SHA”模式，产生多轮
   evidence/alignment 提交和重复 CI；
4. dry-plan 的原始输入模型要求先得到合法 compiled package，但验收语义又
   要求它诊断“即将移除且仍被使用”的非法过渡，造成编译阶段和规划阶段顺序
   冲突；
5. worker 没有强制 heartbeat、阶段超时、红测试先行和可恢复的停止协议，
   使失败的修复尝试表现为长时间无产出；
6. 多次全量 CI 的绿色结果提高了等待时间，却没有覆盖后来由 REVIEW-C 找到
   的细粒度不变量。

因此，当前不是“再等 Main CI 就会完成”：`ed870ac` 的 exact CI、fresh REVIEW-C、
Main CI、Closeout CI 和最终状态 CI 均已 PASS。PLAN-0011 已通过最终完成审计，
没有需要恢复的实现或评审工作。

## 2. 权威当前状态

| 项目 | 当前事实 | 结论 |
| --- | --- | --- |
| Repository | `F:\Workspace\business-platform` | 正确工作区 |
| Branch | `main`（PLAN-0011 feature branch 已删除） | 当前集成分支 |
| Implementation state | `3a42856cda587b205f8927c1d06e3aa5f532692d` | closeout 状态已验证 |
| Remote feature HEAD | 不存在 | local/remote cleanup 已完成 |
| `origin/main` at state verification | `3a42856cda587b205f8927c1d06e3aa5f532692d` | 最终状态 CI 已通过 |
| Merge base at state verification | `3a42856cda587b205f8927c1d06e3aa5f532692d` | closeout history 已在 main |
| Ahead / behind at state verification | `0 ahead / 0 behind`（main） | 已集成 |
| Implementation base | `31b24c6993dbff1f3e88b2476e0c87460400ec31` | exact review base |
| PLAN status | `Integrated / Archived` | 集成、归档、Closeout CI 和 cleanup 均完成 |
| Latest exact-head CI | Run `32210387950`, HEAD `ed870ac`, `PASS` | 自动化门禁通过 |
| Previous candidate REVIEW-C | HEAD `7867123`, `FAIL` | 历史候选已失效 |
| REVIEW-C for `ed870ac` | `PASS` | Rawls 精确审查无 actionable findings |
| Main CI | Run `32213985080`, `PASS` | main exact HEAD 已通过 |
| Closeout CI | Run `32214911706`, `PASS` | closeout commit 已通过 |
| Final-status CI | Run `32216138288`, HEAD `3a42856`, `PASS` | 最终状态提交全部 jobs 通过 |
| Branch cleanup | `DONE` | local `-d` 与 remote `--delete` 均成功 |
| Working tree at state verification | clean | `main` 与 `origin/main` 一致 |

implementation base 到 `ed870ac` 的已提交候选范围为 39 个提交、19 个文件，约
6,126 行新增和 18 行删除；feature branch 在集成前相对 main 为 40 个提交。该
范围仍只包含 PLAN-0011 的纯 Rust contract/compiler、测试、架构 fitness 和
必要的 ADR/PLAN 澄清；没有数据库迁移、runtime、worker、具体业务模块、
Marketplace、动态插件、PLAN-0006 实现或 C migration。

`ed870ac` 中已提交的 bounded repair 严格对应 fresh REVIEW-C 的 4 个 HIGH：

- `business-module-contracts`：typed target classification、UI/Agent target
  闭合约束和 published capability target；
- `business-application-compiler`：结构化 planning conflict、显式 platform
  capability evidence/resolution、canonical digest/反序列化覆盖；
- `scripts/check-architecture.ps1`：扩大 generic production scan 到三个
  contract/compiler crate，并排除 test fixture；
- 对应的 typed contribution、compiler/planning、架构/ADR/PLAN/fitness 文档测试
  与契约同步。

它们已经属于远端 `ed870ac`，并有 exact CI、fresh REVIEW-C 和 Main CI 证据；handoff
及 closeout 状态又已在 `fd4b754`、`9757669`、`3a42856` 中正式记录并通过对应 CI。

### 2.1 自动化验证事实

本轮诊断实际重新执行了以下 focused loop：

```text
cargo test -p business-module-contracts --test typed_contributions    PASS (12)
cargo test -p business-application-compiler --test compiler           PASS (18)
cargo test -p business-application-compiler --test planning           PASS (26)
cargo test -p business-application-compiler --test synthetic_fixtures PASS (6)
pwsh ./scripts/check-architecture.ps1                                PASS
git diff --check                                                      PASS
```

`ed870ac` checkpoint 的完整本地门禁记录为 PASS：

```text
cargo fmt --all -- --check                                        PASS
cargo check --workspace --all-targets --all-features               PASS
cargo clippy --workspace --all-targets --all-features -- -D warnings PASS
cargo test --workspace --all-features                              PASS
  (all executed tests passed; existing external-infrastructure tests remain ignored)
pwsh ./scripts/check-architecture.ps1                              PASS
pwsh ./scripts/check-openapi.ps1                                   PASS
git diff --check                                                    PASS
```

远端 [Run `32210387950`](https://github.com/jesenzhang/business-platform/actions/runs/32210387950)
已对已提交 exact HEAD `ed870ac` 通过 Format、Check、Clippy、
CLI/MCP、Architecture Fitness、Unit tests、Frontend checks 和 PostgreSQL + MinIO
+ E2E contracts 和 Playwright smoke，Run 总体为 `PASS`。这只能证明自动化验证
通过，不能替代独立 REVIEW-C。Node.js 20 deprecation annotations 是 workflow
环境提示，不是本次失败。

外部扫描 `cargo-audit`、`cargo-deny`、`gitleaks`、`trivy`、`syft`、`grype`、
`osv-scanner` 未运行；当前环境没有仓库正式入口或已安装 executable。不得把
它们写成 PASS。

## 3. 诊断反馈闭环与假设判定

### 3.1 反馈闭环

本次诊断使用的最小确定性反馈闭环是：

```text
Git status / exact SHA
        +
focused compiler/planning/synthetic tests
        +
GitHub run headSha/status/conclusion/jobs
        +
review range and findings
        -> classify: code / process / infrastructure / evidence
```

这个闭环能够把“测试失败”“CI 失败”“评审拒绝”“没有开始下一关”区分开；
它显示历史失败类别主要是 review-gate failure，而不是 infrastructure failure；
当前候选已通过 review，剩余是 integration/closeout gate 未执行。

### 3.2 排名假设

| 假设 | 判定 | 可核对证据 |
| --- | --- | --- |
| H1：高风险语义在实现规模变大后才被检查，导致一次小缺陷触发整轮返工 | 已证实/强支持 | 最终评审一次性检查 stable identity、typed contributions、extension point、SemVer、canonicalization、dry-plan、fitness；`40c4078` 和 `7867123` 均在 CI 绿后被拒绝。 |
| H2：候选证据与 reviewed immutable HEAD 脱节 | 已证实，已修复流程 | 旧 evidence/alignment 对出现 `b3ab20e -> 40c4078` 等循环；提交无法包含自身尚未知晓的 SHA。 |
| H3：dry-plan 输入契约与 BlockedRemoval 验收语义有阶段顺序冲突 | 历史阻塞已证实；已修复并通过复审 | standalone compilation 会在 `UnknownExtension` 处先失败，规划器无法看到 removal intent；`f64e767` 引入 declaration transition seam，`880b9a6` 又修正 incoming graph/consumer 边界。 |
| H4：worker 缺少 heartbeat 和硬性停止条件，放大了等待和重启成本 | 支持，但精确等待时长不可证实 | 两个 repair worker 无 commit 返回，也没有完成 focused-test 阶段；执行记录没有可审计的 worker heartbeat/阶段耗时。 |
| H5：当前主要原因是 GitHub CI 或基础设施卡死 | 已排除 | Run `32107765513` 在约 4 分钟内对 exact `880b9a6` 全 job PASS；历史 `32099631658` 也 PASS。 |
| H6：CI PASS 即代表候选被接受 | 已排除 | `7867123` 的 exact-head CI `32099631658` PASS，但 fresh REVIEW-C 仍给出 2 个 CRITICAL、3 个 HIGH。 |

H4 的结论需要保守理解：仓库没有完整 worker telemetry，不能把日历间隔
全部归因于某个 worker、模型 API 或人工等待。可以确认的是流程没有提供足够
的可见性来快速区分这些情况。

## 4. 阻塞点、返工点和处理结果

### 4.1 历史阻塞、已完成修复与当前阻塞

当前没有未解决的 code/review、Main CI、Closeout CI 或仓库状态阻塞。`f945a53` 的
fresh REVIEW-C FAIL 是历史事实，四个 HIGH 已在 `ed870ac` 中修复，并已通过
exact-head CI 和新的 fresh REVIEW-C。候选已 fast-forward 到 main，且 Main
CI/Closeout CI/最终状态 CI 已通过；ancestor、clean 和 feature branch cleanup
核验也全部完成。

历史候选 `7867123` 的 REVIEW-C 发现了以下五个 blocking findings：

1. **CRITICAL — dependency transition graph**：dry-plan 从 current package
   读取依赖，而不是从 incoming retained package 读取；新增依赖可能错误地
   放行 provider 移除，删除依赖可能错误地阻塞移除。
2. **CRITICAL — public target consumers**：移除 query/command/resource 时没有
   检查保留的 UI/Agent/extension consumers，可能把仍被使用的 endpoint 当成
   可移除。
3. **HIGH — contribution identity domain**：legacy UI/Agent ID 没有统一
   owner namespace，extension ID 又使用独立 collision set，可能出现跨类型或
   外部 owner 冲突。
4. **HIGH — architecture fitness bypass**：fitness script 只拒绝显式 path
   dependency，`workspace = true` 依赖可能绕过 generic/business isolation。
5. **HIGH — exact-head evidence**：evidence 没有明确绑定 `7867123` 与
   Run `32099631658` 的 immutable CI 事实。

`880b9a6` 是针对这五项的 bounded repair：

- dry-plan 改用 incoming dependency graph；
- public target removal 检查保留 UI/Agent/extension consumer；
- legacy contribution IDs、extension IDs 和 manifest/typed IDs 进入共享
  collision domain，并强制 owner namespace；
- architecture fitness 使用 Cargo metadata 和 generic crate dependency
  allowlist，避免 `workspace = true` 绕过；
- evidence 明确记录 `7867123` / Run `32099631658` 的历史 PASS，避免 SHA
  自引用。

`f945a53` 是在上述修复基础上完成的 bounded repair：dry-plan 不再用整个
package digest 作为每个 component 的 fingerprint，而是对归一化后的每个
command/query/event/resource/semantic/legacy UI/agent、typed contribution、
policy/capability requirement 和 extension contribution 计算自身 digest。
新增回归测试证明 package metadata 变化不会误更新未改变的 contribution，且
requirement 变化会产生精确 update。该 HEAD 已通过本地完整门禁并已推送，仍未
得到 reviewer 认可。

随后 fresh REVIEW-C 对 `31b24c6..f945a53` 返回 `FAIL`，新增 4 个 HIGH：

1. dry-plan 对未知/不兼容依赖和 ownership collision 返回普通
   `PlanError::PlanningCompilation`，没有 Stage 7 要求的结构化 `Conflict`；
2. Typed UI/Agent 缺少 classification，Agent 复用过宽的 `PublicTargetKind`，未限制
   到 Query/Command/approved Capability；
3. required platform capability 只解析版本，没有 against explicit platform evidence
   做 fail-closed resolution；
4. architecture fitness 只扫描 compiler source，`business-module-contracts` 仍可
   绕过 generic neutrality 和 semantic single-authority 检查。

这些是 reviewer 对 `f945a53` 的阻塞结论，不是当前 HEAD 的结论。`ed870ac`
逐项实现了 bounded repair，形成新的 immutable HEAD，并通过 focused/full
local gates、远端 exact-head CI 和 fresh REVIEW-C；这些阻塞已关闭。

当前四项修复状态：

| REVIEW-C finding | `ed870ac` 中的 bounded repair | 当前证据 |
| --- | --- | --- |
| 结构化 planning conflict | 已将未知/不兼容依赖、ownership collision、cycle、unknown extension 等映射为 `Conflict` | planning focused tests、Run `32210387950`、REVIEW-C PASS |
| Typed UI/Agent boundary | 已加入 `DataClassification`、闭合 target kind 和 published capability catalog 校验 | typed/compiler tests、Run `32210387950`、REVIEW-C PASS |
| Platform capability evidence | 已加入显式 host evidence、版本匹配、fail-closed resolution，并持久化 resolved evidence | compiler serialization/resolution tests、Run `32210387950`、REVIEW-C PASS |
| Architecture fitness scope | 已扫描三个 generic production crate，并排除 test fixture | Architecture Fitness、Run `32210387950`、REVIEW-C PASS |

### 4.2 已发生的主要返工

| 轮次/提交 | 暴露的问题 | 影响 |
| --- | --- | --- |
| `21c3420` | compiler/dry-plan canonical reconstruction、disabled state、owner-consumer removal 和 ID collision 不完整 | 首轮实现无法作为候选 |
| `f08ac7d` | compiled manifest 反序列化后的 digest 校验不充分 | 需要补强完整性边界 |
| `4742857` | 未知 top-level field 和重新计算 digest 的 non-canonical payload 可被接受 | 需要拒绝非规范 manifest |
| `014d3a5` | `PublicCapability` 合法依赖没有从 provider agent-tool 发布到 dependency catalog | 合法输入被错误判为 unknown |
| `40c4078` REVIEW-C | live-consumer predicate 错误；candidate evidence 过期 | 两个 HIGH，候选失效 |
| `f64e767` | 引入 declaration transition seam，保持 standalone compilation fail-closed | 修复历史 dry-plan/编译阶段冲突 |
| `2639c3e`、`7867123` | transition/evidence checkpoint | 分别触发重复 CI，但 `786` 仍在下一次 review 被拒绝 |
| `880b9a6` | 修复 `786` review 的两 CRITICAL、三 HIGH | 新 HEAD 已 CI PASS，待 fresh review |
| `f945a53` | 修复 contribution-level whole-package fingerprint 误报，并覆盖 policy/capability requirement | 本地门禁和 CI `32117575001` PASS，但 Cicero fresh review FAIL（4 HIGH） |
| `ed870ac` | 针对 `f945a53` 的 4 个 HIGH 完成 bounded repair | 本地门禁 PASS；Run `32210387950` PASS；Rawls fresh REVIEW-C PASS |

这些返工中有两类必须区分：

- compiler/dry-plan、canonicalization、capability catalog 和 identity 的返工是
  实质正确性修复；不能简单删除；
- evidence/alignment 的多次往返主要由流程设计缺陷产生，增加了提交和 CI，
  没有相称地增加语义覆盖。

### 4.3 worker 层面的返工/中断

至少两个 fresh repair worker 在此前循环中超时或被中断，未产生 commit，且
没有完成 focused-test 阶段（因此没有可用的 focused-test 通过/失败结果）。它们的
partial diff 已由主流程核对后形成 `880b9a6`，不应
恢复旧 worker 的工作树或让旧 worker 自证其修改正确。后续 repair 必须只接受：

```text
exact Base
+ rejected HEAD
+ exact reviewer findings
+ relevant ADR/PLAN constraints
```

若未来出现新的 review FAIL，不得在 repair 中顺便扩大范围或重构无关代码。

本轮对 `f945a53` 的 fresh repair worker `Noether` 也在约 6 分钟内没有 heartbeat
或 partial diff，已安全中断；工作树没有新增代码修改。该事件支持 H4，但不能
归因于代码或基础设施失败。

随后启动的第二个 fresh repair worker `Linnaeus` 在收到进度检查时仍处于
source/design inspection，未开始具体 patch，已按停止协议结束；同样没有文件
修改、测试、commit 或 push。下一接手者不应恢复这两个 worker 的隐式状态，而应
从本 handoff、exact base 和四个 findings 重新建立一个有阶段 heartbeat 的 repair。

## 5. 为什么这么长时间仍未完成

### 5.1 时间线事实

可由 Git/CI 直接观察的时间窗口：

- activation `31b24c6`：2026-08-13 14:55:34 (+08:00)；
- 首个大候选 `40c4078`：2026-08-13 22:58:14 (+08:00)；
- 初始 staged implementation、修复、证据和候选准备约耗时 8 小时 2 分 40 秒；
- `f64e767`：2026-08-18 12:11:04 (+08:00)；
- `2639c3e`：2026-08-18 12:18:04 (+08:00)；
- `7867123`：2026-08-18 12:33:57 (+08:00)，其 exact CI 约 5 分钟且 PASS；
- `880b9a6`：2026-08-18 14:36:51 (+08:00)；
- `f945a53`：2026-08-18 16:39:33 (+08:00)，本地门禁通过后推送；CI Run `32117575001` PASS；
- Run `32117575001`：约 16:40 至 16:50 (+08:00)，约 10 分 17 秒；主要等待 Playwright 浏览器安装与 smoke，非业务失败；
- Run `32107765513`：约 14:38 至 14:42 (+08:00)，约 4 分钟，PASS。
- `ed870ac`：2026-08-19 10:56:57 (+08:00)，完成 4 个 HIGH 的 bounded repair；Run
  `32210387950` 于 10:57 至 11:23 (+08:00) PASS，随后 fresh REVIEW-C PASS。
- `fd4b754` / `9757669` / `3a42856`：2026-08-19 closeout、verification 和
  branch-cleanup 状态提交；Closeout CI Runs `32214911706`、`32215330940` 和最终
  状态 CI Run `32216138288` 均 PASS；该状态快照中 `HEAD == origin/main == 3a42856`。

从 40c 到后续修复之间的完整 worker 等待、模型调用和人工处理 telemetry
没有持久化，因此不能把这段日历间隔精确分摊给某一原因。能确定的是：CI
本身不是主要耗时项，主要耗时来自评审发现后的语义重建、证据修正和等待下一
个可接受的 immutable checkpoint。

### 5.2 流程放大图

```text
大范围 staged implementation
        ↓
一次性 full gates（主要验证“能构建”）
        ↓
晚到的 adversarial REVIEW-C
        ↓
候选失效
        ↓
fresh repair worker + focused tests + full CI
        ↓
evidence/alignment commit 再触发 full CI
        ↓
fresh REVIEW-C
        ↓
accepted feature candidate
        ↓
main integration / Main CI / closeout
```

这个环路是“长时间未完成”的主要解释。它不是单纯因为代码行数多，而是因为
每个后期发现都使此前的 candidate、CI 和 reviewer verdict 不能复用。

## 6. 当前流程设计的问题与改进建议

### 6.1 Goal 粒度过大，durable stage 没有真正成为可恢复任务

附件要求 Coordinator → fresh worker → focused validation → immutable checkpoint
→ stage gate，但实际运行仍把多阶段目标维持在一个长链路中。Stage 的名称和
SHA 虽被记录，worker heartbeat、阶段 start/end、阻塞原因、unfinished
completion conditions 没有以统一 durable state 保存。

改进：每个 major stage 建立可独立恢复的 handoff，至少记录 `stage`、`base`
、`checkpoint`、`owner`、`focused gate`、`full gate`、`review verdict`、
`next action` 和 `stop reason`；模型/API 中断后从最后一个 valid checkpoint
恢复，不重新扫描整个 Goal。

### 6.2 最终评审承担了过多风险，缺少早期 focused review

REVIEW-C 同时审查 identity、typed contributions、extension point、SemVer、
canonical bytes、digest、dry-plan、synthetic fixtures 和 fitness。任何一个
局部遗漏都会让整个候选失效。

改进：在实现完下列高风险 seam 后立即做轻量 read-only review 或 adversarial
contract test：

- compiled-manifest deserialize/canonicalization；
- dependency catalog positive/negative resolution；
- current-vs-incoming lifecycle truth table；
- contribution/extension identity collision domain；
- candidate evidence/head binding。

### 6.3 Acceptance matrix 不够可执行

早期 gate 覆盖了“主要路径能通过”，但没有把每个公开 dependency variant、
每个 removal transition 和每个 deserialization adversarial case 变成一一对应
的测试。后来 reviewers 找到的缺口都可以用很小的 focused test 更早暴露。

改进：先建立 truth table，再编码。例如 dependency、public target、extension
point、consumer contribution 分别列出 current/desired 状态和预期
`Allowed`、`Conflict` 或 `BlockedRemoval`；正向和反向测试必须成对存在。

### 6.4 Evidence contract 曾经不可满足

“把 candidate HEAD 写进包含该文档的 commit”是 Git 上不可满足的自引用约束。
每次 alignment 都改变 commit SHA，不能靠再提交一次解决。

改进：candidate identity 由 review request、CI run 的 `headSha`、tag 或 post-
review closeout record 提供；候选提交只记录 predecessor/implementation
facts，不声明自己包含自身 SHA。

### 6.5 CI 没有分层，文档 churn 也触发全量工作流

多次 docs-only evidence/alignment commit 触发完整 CI，等待成本高，但不验证
新的语义。CI PASS 也没有替代 reviewer 对架构不变量的攻击性检查。

改进：区分 focused local tests、纯 contract/compiler CI、完整仓库 CI、真正
需要基础设施的 E2E。docs-only 只运行链接/格式/evidence schema check；候选
immutable code HEAD 再运行一次完整 CI。

### 6.6 Worker 缺少可观测性和停止条件

长时间无 heartbeat 时，Coordinator 无法判断 worker 是在编译、等待模型、
网络重试还是已失去进展。

改进：要求固定 heartbeat（例如 5–10 分钟）、首个红测试立即报告、focused
loop 不通过不得启动 full gates、阶段 hard timeout、停止时输出 partial state
和恢复命令。基础设施失败必须记录为 `INFRASTRUCTURE_BLOCKED`，不能与业务
失败混在一起。

## 7. 有效 handoff：若后续需要复核/恢复的最短路径

以下 Gate 1–4 是已经完成的证据链，不是当前待执行队列。若未来需要复核，必须
从下列 immutable checkpoint 读取事实；不得把已归档 PLAN-0011 重新当作活动实现分支。

### Gate 0：保留当前事实

1. 不 amend、force-push 或 rewrite `40c4078`、`7867123`、`f64e767`、
   `2639c3e`、`880b9a6`、`f945a53`、`ed870ac`、`fd4b754`、`9757669` 或
   `3a42856`。
2. 当前必须保持 `main`、工作树 clean，且 PLAN-0011 feature branch 保持已删除；
   实现状态必须仍以 `3a42856` 为基线或其 docs-only 后代，不得改写该状态。
3. 不恢复两个已中断 worker 的 partial state。
4. 不把新的实现混入本 handoff 或归档 PLAN；若有新增需求，创建独立 Plan、
   fresh branch 和新的 review/evidence 链。

### Gate 1：固化 bounded repair（已完成）

`f945a53` 已完成 fresh REVIEW-C 且为 FAIL；不得重复使用该 verdict，也不应
重复审查同一 SHA。该失败已经由 `ed870ac` 关闭。接手者不得再从 `f945a53`
重复创建 repair；只需保留 `ed870ac` 作为当前候选，并核对以下已完成条件：

1. 只修复 blocking findings，不扩大 scope；
2. focused tests 和 full local gates PASS；
3. 创建新的 immutable `ed870ac`，未 amend/force-push；
4. push `ed870ac`，并取得绑定该 SHA 的 exact-head CI PASS；
5. 历史 repair 的 heartbeat/partial-state 记录不完整，这属于已记录的流程
   缺陷，不是当前候选的验收阻塞。

### Gate 2：对 `ed870ac` 发起 fresh REVIEW-C（已完成）

使用只读 Sol reviewer，精确范围为：

```text
31b24c6993dbff1f3e88b2476e0c87460400ec31..ed870acfe165756632c0519bb181fd5dcf8a11cd
```

Rawls 已对上述精确范围重新验证 Platform Core neutrality、ownership、identity/collision、
typed UI/Policy/Agent boundary、published extension point、SemVer、dependency
cycle、canonical bytes/digest、dry-plan determinism、BlockedRemoval、data
retention、semantic single authority、scope 和 architecture fitness。没有
actionable findings，verdict 为 `PASS`，因此进入 Gate 3。不得把该 PASS 复用到
其他 SHA，也不得把旧的 `7867123` / `f945a53` FAIL 当作当前 HEAD 结论。

### Gate 3：main integration（已完成）

1. `git fetch origin --prune`，重新读取 `origin/main`；候选集成时结果为
   `ed870acfe165756632c0519bb181fd5dcf8a11cd`，最终 closeout 后为
   `3a42856cda587b205f8927c1d06e3aa5f532692d`；
2. 复核 main 是否改动 `business-module-contracts`、
   `business-application-compiler`、相关 ADR/fitness 或 PLAN 假设；
3. 只有在 base 为 ancestor、无冲突且 scope 仍成立时，才允许 solo
   fast-forward；否则从真实最新 main 重建 candidate 并重新 full gates/review；
4. 已无冲突 fast-forward 到 main 并 push；
5. Main CI Run `32213985080` 已 PASS，进入 Gate 4。

### Gate 4：Main CI PASS 后 closeout（已完成）

1. 已将 PLAN-0011 从 `docs/plans/current/` 移到 `docs/plans/archive/2026/`；
2. 已更新 `docs/plans/README.md`、`docs/architecture/ARCHITECTURE_STATUS.md`
   和 Business Application Platform 文档状态；
3. 已记录 implementation、candidate、review verdict、integration SHA、Main CI；
4. closeout commit `fd4b754` 已 push，Closeout CI Run `32214911706` 已 PASS；
5. closeout verification commit `9757669` 已 push，并有对应 Closeout CI
   Run `32215330940` PASS；
6. 已证明最终 closeout history 是 `main` ancestor 且工作树 clean，随后使用
   `git branch -d` 和 `git push origin --delete` 清理 feature branch；未使用无证据
   的 `-D`；
7. branch cleanup record commit `3a42856` 已 push，最终状态 CI Run `32216138288`
   已 PASS。

## 8. 完成定义与当前状态

PLAN-0011 在以下条件全部满足前都不能通过最终完成审计：

- 当前 bounded repair 形成的 `ed870ac` 通过 exact-head CI；
- fresh REVIEW-C 对 exact Base..HEAD 返回 `PASS`；
- 候选 fast-forward 集成当前 main；
- Main CI PASS；
- PLAN-0011 标记 `Integrated` 并归档；
- closeout CI PASS；
- feature branch 本地和远端安全删除；
- 最终工作树 clean。

当前完成状态：

```text
Fresh REVIEW-C for ed870ac = PASS (Rawls, exact range 31b24c6..ed870ac)
PLAN implementation state = 3a42856, closeout history integrated
Main CI = PASS (Run 32213985080)
PLAN archive = DONE
Closeout commit/CI = DONE (fd4b754 / Run 32214911706)
Closeout verification = DONE (9757669 / Run 32215330940)
Closeout CI = PASS (Run 32214911706)
Feature branch cleanup = DONE (local and remote deleted)
Final-status CI = PASS (Run 32216138288; HEAD 3a42856)
State-verification working tree = CLEAN; post-report HEAD must be a clean docs-only descendant
```

当前明确禁止：PLAN-0012、PLAN-0006、Contract/C production migration、
Finance/Legal/HR/CRM、dynamic plugin、Marketplace、WASM/Node/Python runtime、
WrenAI/Analytics runtime 或任何超出 PLAN-0011 的扩展。

## 9. 交接摘要

接手者只需记住三件事：

1. `f945a53` 是已 push、本地和远端 CI 通过、但被独立评审拒绝的历史 HEAD；
2. `ed870ac` 已关闭这 4 个 HIGH，并取得 exact CI 与 fresh REVIEW-C PASS；不要
   重复 repair 或复用其他 SHA 的 verdict；
3. main ancestor、Closeout CI、工作树 clean 和 feature branch cleanup 均已完成；
   当前可按完成审计关闭 PLAN-0011。
