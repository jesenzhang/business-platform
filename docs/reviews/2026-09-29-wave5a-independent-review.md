# PLAN-0014 Wave 5A 独立审阅与 zcode 修复指令

日期：2026-09-29。结论：**NEEDS REPAIR / NOT PUBLISHED**。

## 1. 固定审阅对象

- 仓库：`F:\Workspace\business-platform`
- 分支：`codex/plan-0014-wave5a-local-release-pilot`
- Base：`9b05ccddb4964b363b0c9ae001c4303c1b7393db`
- 实现候选：`8036bc65ba7c77666e48afaa6439911ba7c083d7`
- 文档 closeout HEAD：`d0b2d81ffd6a1c5efcdbdd89fec18f9ebdac8c7a`
- 审阅开始时工作树干净。本轮不修改产品代码；只新增本审阅记录，复现目录位于 gitignored `target/wave5a-independent-review/`。
- 使用 `open-code-review-delegate`：OCR 提供文件清单和规则，主审阅者顺序审阅，未调用外部 LLM。

本轮确认两个本地候选能够生成，四类摘要的区分合理，正式 migration catalog 的只读映射、共享 PostgreSQL 019 的阻塞声明也保留了边界。但验证命令和证据脚本存在以下五项缺陷，尚不能签署 Wave 5A 通过。

## 2. Findings

### F1 / P1 / bug：来源验证无条件 PASS

位置：`tools/module-release-tool/src/packaging.rs:480-492`；关联 `src/repo.rs` 的 revision 读取和 `generate` 的工作树读取。

`source-provenance` 没有核实提交存在、源文件来自该提交或工具链证据，只将读到的字段拼成字符串并写 PASS。复现：在候选副本中同时把 `release.json.source_revision` 和 `release-bundle.json.identity.source_revision` 改为 40 个 `0`；`verify` 退出 0、`status=verified`、`source-provenance: PASS`。同时，生成器从当前磁盘读取文件，却用 Git HEAD 标注来源，缺少将被打包输入绑定到该提交的检查。

影响：本地未提交的源码也可能被标记为某个已提交候选，破坏固定 SHA 审阅与后续来源追踪。本 finding 不要求生产签名或供应链认证；要求本地来源声明具有可核实依据。

### F2 / P2 / bug：compare 只比较清单，未比较实际 payload

位置：`tools/module-release-tool/src/packaging.rs:520-524`。

复现：复制正确候选，向副本 `package/contract/packages/identity/src/lib.rs` 追加 `compile_error!("review tamper");`，保持清单不变。`compare --left <正确候选> --right <副本>` 退出 0、`status=identical`，仍打印相同旧摘要。

影响：损坏或已变化的右侧工件被报告为重复构建一致。脚本仅对 run-a 做后续独立 verify，不能弥补 compare 本身不读取 run-b 实物的问题。

### F3 / P2 / bug：工件描述记录缺少跨记录一致性检查

位置：`tools/module-release-tool/src/packaging.rs:316-357`。

复现：仅将候选副本的 `artifact-manifest.json` 改为 `module_id=policy`、`module_version=9.9.9`、`artifact_integrity_algorithm=unknown`，其余不变；`verify` 仍退出 0、`status=verified`，按 release.json 输出 identity。

影响：同一个已验证目录可同时宣称不同模块/版本，未知算法也被当成支持。排除自引用哈希不意味着描述记录可以不校验。相邻的 `schema_version`、`contract_digest`、`package_digest` 和实际 packed Cargo/package 与 bundle 的映射也应一并建立明确的一致性约束。

### F4 / P2 / test：消费者路径检查不检查解析后的依赖图

位置：`scripts/plan0014-wave5a.ps1:251-284`。

脚本只搜索消费者 Cargo.toml 和 Cargo.lock 中的 Windows 反斜杠绝对路径。Cargo.lock 不记录本地 path 包的 manifest 路径，正斜杠、相对路径和传递依赖也不受此检查覆盖。

复现：在临时消费者中将依赖设置为 `F:/Workspace/business-platform/crates/identity`，运行 `cargo metadata --offline --format-version 1` 成功；解析出的 identity manifest 为原仓库 `crates/identity/Cargo.toml`，原检查字符串命中仍为 0。

影响：自动门禁可能把直接引用原工作区的消费者标记为隔离通过。此反例证明门禁漏检，**并不表示已提交的两个消费者实际存在该依赖泄漏**。

### F5 / P2 / test：错误模块 ID 负例没有隔离前一个故障

位置：`scripts/plan0014-wave5a.ps1:321-341`。

wrong-version 后未从正确候选重建副本，就继续 wrong-module-id。后者还用带空格的字符串替换处理实际紧凑 JSON，目标修改可能没有发生。已提交 `53-tamper-wrong-module-id-identity.log` 实际失败是 `bundle-identity`：release-bundle.json 与上一步已改坏的 release.json 不同。

影响：该日志不能证明错误模块 ID 被相应检查拒绝。Rust 测试覆盖部分字段拒绝，并不能使这一 CLI 负例成为有效证据。

## 3. 本轮独立验证与限度

| 检查 | 本轮结果 |
| --- | --- |
| `cargo test -p module-release-tool --all-features` | 30 passed / 0 failed / 0 ignored |
| identity / policy 候选重新生成 | PASS；21 / 23 payload members |
| 本轮候选与入库的 14 个 JSON 记录比较 | 仅来源 revision 及对应验证说明存在预期差异，其余结构化内容一致 |
| F1 全零 revision 反例 | 非法来源仍 verify 成功，已复现 |
| F2 改写源码后 compare | 仍 identical，已复现 |
| F3 错误工件描述记录 | 仍 verify 成功，已复现 |
| F4 原仓库 path 依赖注入 | metadata 确认真实引用，原检查漏检，已复现 |
| F5 CLI 负例 | 源码与既有日志共同确认错因污染 |
| 全 workspace / clippy / fmt / 架构门禁 | 本轮 NOT RUN；实现记录中的历史结果不冒充本轮结果 |
| 原正向消费者完整 build/test/offline 脚本 | 本轮未全量重跑，已读实现及历史证据 |
| PostgreSQL、部署、发布、CI、漏洞/许可证扫描 | 本轮 NOT RUN |

新生成的完整候选与反例位于 `target/wave5a-independent-review/`，属于可重新生成的本地审阅辅助材料，不是正式发行包。

## 4. 可直接交给 zcode 的实施指令

### 4.1 任务与边界

执行 **PLAN-0014 Wave 5A Repair 1：修复本地候选工件验证链**。逐项关闭 F1–F5，交付固定代码 SHA、重新生成的证据和逐项映射；完成后停在 **READY FOR INDEPENDENT REVIEW / NOT PUBLISHED**。

开始先读取本文件、AGENTS.md、适用架构 Baseline、SAAS_MODULE_STANDARD、PLAN-0014、原 Wave 5A 实现记录、目标 Cargo.toml/src/lib.rs。遵循既定权威顺序。

核对当前分支与上述 HEAD 的关系。保留本审阅记录及其他人的工作；发现额外提交先检查其 diff，不能 reset、覆盖或把无关变更混入修复。延续现有分支即可。

允许修改：

- `tools/module-release-tool/src/{repo,packaging,main,lib,canonical}.rs` 中与 findings 相关的部分；必要的小型新 helper 模块。
- `tools/module-release-tool/tests/` 中的对应回归测试。
- `scripts/plan0014-wave5a.ps1`；必要时抽取其路径验证/负例验证 helper，并添加脚本回归测试。
- `scripts/check-architecture.ps1` 中为来源检查所需的窄范围规则调整。
- 确有依赖需要时的工具 Cargo.toml / 根 Cargo.lock；保持依赖最小。
- 新的修复记录、证据目录及 PLAN-0014 / ARCHITECTURE_STATUS / docs README 的进度指针。

保持业务 crate、两个纯 contract/compiler crate、公开 API/Event、历史 migration SQL/checksum 和 Accepted ADR 不变。继续保留四种 digest 的现有职责。不得通过修改纯契约 schema 把本轮工具修复升级成平台设计改造。

### 4.2 先加入失败回归，再按顺序修复

**步骤 A：F1 来源绑定**

1. 为全零或不存在的 revision、同时修改两份 identity 的 revision、被打包输入与声明提交不一致加入反例。只修改一份 identity 的测试不足以覆盖来源检查。
2. 生成时从固定提交的不可变输入快照读取，或对所有实际打包输入执行等效的提交一致性校验；覆盖源码、Cargo 配置/lock、release spec、changelog、被引用迁移和其 manifest。不能只检查一个 HEAD 字符串，也不能只记录 dirty 数量。
3. 验证时按工件记录的 source_revision 核对提交及对应输入，重新推导 packed payload/必要描述信息，并与工件比较。不要将“必须等于当前 HEAD”作为替代：文档 closeout 前的有效代码候选仍应可验证。
4. 不存在的源提交、无法取得所需输入、内容不匹配必须明确拒绝；不得返回来源 PASS。若工具链只验证了配置声明，检查名称和结果必须明确为声明检查；实际构建工具链另由本次运行日志证明，不冒充二进制构建来源认证。
5. Git 元数据读取应正确支持普通 checkout、detached HEAD 和 linked worktree。优先可靠的 Git 接口，避免继续手写不完整的 refs/packed-refs 解析。若在外层工具调用受限 Git 命令，窄化调整架构检查并说明原因；纯 crate 禁止进程/文件 I/O 的约束继续有效。命令参数使用参数数组，不执行动态 shell。
6. 工作区后续仅新增 closeout 文档时，验证旧候选仍应成功；来源内容变化未提交时，不能生成带旧提交来源 PASS 的新候选。

**步骤 B：F2 实物比较**

1. compare 分别读取左右 payload，重算 inventory/digest，并确认各自与清单一致，再判断是否一致。
2. 任一侧缺文件、改内容、清单过期或未知格式，都不得退出 0 并打印 identical。
3. 保留“相同固定输入、不同输出目录”的正常通过用例；增加左右两侧各自受损的负例。
4. 来源 SHA 不参与 payload digest 的现有语义可保留，比较输出必须清楚说明比较范围。

**步骤 C：F3 描述记录一致性**

1. 拒绝不支持的 artifact schema/algorithm。
2. 检查 artifact-manifest 的 module_id/module_version/contract_digest/package_digest 与 release、bundle、编译结果一致。
3. 将 bundle.crate_versions 与实际 packed Cargo.toml 的 package name/version/成员集合核对，拒绝缺失、多余、重复和错误绑定。保持声明摘要与实体文件摘要分离。
4. 每个错误字段单独测试；错误须具名并可定位。对 payload 内字段的语义负例，应安排正确层级的测试，避免所有反例都只命中外层 checksum 而未覆盖语义门禁。
5. 自引用元数据不纳入自己的 digest；需要验证的字段通过外部输入或跨记录约束验证。

**步骤 D：F4 消费者解析图隔离**

1. 用成功返回的 `cargo metadata --format-version 1 --locked --offline` 获取与实际测试相同 feature 配置的完整解析图，保存 metadata 证据。不能使用 `--no-deps` 隐藏传递依赖。
2. 对全部本地 package 的 manifest_path 进行规范化、真实路径解析和目录边界检查。仅允许消费者目录及其指定候选 artifact 内的 package；拒绝回到原仓库 crates/apps/tools 或其他未授权本地目录的依赖。
3. 大小写、正反斜杠、相对路径、目录前缀碰撞均不能绕过检查；传递 path dependency 必须检查。支持链接时应解析最终目标，不能确定则拒绝。
4. 加入正常工件依赖通过、原仓库绝对正斜杠路径拒绝、相对路径拒绝、传递 path 泄漏拒绝的自动回归。执行真正的 guard helper，不能在测试里重写一份独立规则。
5. 清楚记录实际目录位置。`target/` 下自带 `[workspace]` 的消费者可证明 Cargo workspace 成员隔离，但不应描述为“物理上在仓库外”；至少一次正向复建应从源仓库目录外、同卷的临时消费者目录执行，记录 Cargo 配置来源，避免继承仓库 `.cargo` 配置而不自知。按环境权限执行；不可用时如实 BLOCKED。

**步骤 E：F5 有效且隔离的负例**

1. 每个负例从已验证的 pristine 候选创建全新副本；先证明基线成功，再只注入这一例的故障。
2. 通过 JSON 解析/序列化修改字段；修改后读取并断言目标值已改变，不用空格敏感的 Replace。
3. 同时断言非零退出和预期检查 ID/错误类别。进程启动、编译、JSON 解析失败等非目标错误不能计为目标负例通过。
4. wrong-version 与 wrong-module-id 不共享脏副本；补充 F1–F4 的 CLI/脚本负例。每个负例保存注入说明、实际变更、命令、退出码与拒绝原因。
5. 脚本在依赖步骤失败时停止后续依赖步骤，不继续使用旧工件凑证据。输出目录已存在时明确拒绝或使用新的 run 目录；清理只能作用于已验证归属的本次 scratch 子目录。

### 4.3 验证命令与证据

按仓库 RTK 约定执行以下命令，并检查每条退出码；实际运行使用参数数组/安全 PowerShell 调用。

```powershell
rtk proxy cargo test -p module-release-tool --all-features
rtk proxy cargo test -p business-module-contracts -p business-application-compiler --all-features
rtk proxy cargo fmt --all -- --check
rtk proxy cargo check --workspace --all-targets --all-features
rtk proxy cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk proxy cargo test --workspace --all-features
rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1
rtk git diff --check
```

此外必须执行新脚本回归入口，并将其确切命令写入交付记录。对修复后的固定代码候选 SHA，用**新目录**重跑：

```powershell
rtk proxy pwsh -NoProfile -File scripts/plan0014-wave5a.ps1 -OutRoot target/wave5a-repair1 -EvidenceDir docs/reviews/2026-09-29-wave5a-repair1-gates
```

若该目录已被占用，使用新的明确编号目录，不覆盖旧证据。实现脚本应支持源仓库目录外的 OutRoot；另一次外部路径正向复建按实际允许的同卷位置执行并记录命令。

证据要求：

- 先固定修复代码提交，再生成最终工件/门禁证据，最后做独立文档 closeout 提交；记录二者 SHA。
- 保留原 `wave5a-gates` 历史目录；新增 repair1 记录，明确替代哪些结论，不悄悄覆盖旧日志。
- 两模块分别记录 source revision、四类摘要、成员数量、两次生成比较、独立 verify、packed build、consumer build/test/offline、metadata 隔离、全部负例。
- 现有“70 ignored 全部 PostgreSQL”表述需要按测试清单重新核对并分类；ignored 不计入通过。
- 用一个表逐项列 F1–F5：修复位置、回归名称、命令、日志、实际结果。
- 未执行的门禁写 NOT RUN；环境阻塞写 BLOCKED 和原因。不得复用历史通过计数充作新 SHA 的结果。

### 4.4 完成定义与停止点

F1–F5 的原反例均被正确检查拒绝；正常两模块候选可重新生成、验证、比较、由隔离消费者调用；定向、脚本、workspace 和架构门禁有对应固定 SHA 的结果。

交付：代码 SHA、文档 SHA、状态/dirty 清单、文件范围、逐项闭环表、精确测试计数、NOT RUN/BLOCKED、证据路径。

完成状态仅为 **Wave 5A Repair 1 READY FOR INDEPENDENT REVIEW / NOT PUBLISHED**。保持 **R1 / R2 BLOCKED / NOT CLAIMED**、G-01/G-02/G-201/G-202/G-203 及既有 C1 的原边界；不得自填独立审阅 PASS。停止，不 push/merge/tag/publish/deploy，不启动 Wave 5B 或 PLAN-0006，不改 ADR-0027/0028 的 Proposed 状态。

## 5. 审阅覆盖

OCR：total_files=90，reviewable_files=39，reviewed_files=39，skipped_files=0，coverage_rate=100%（分母为 OCR 可审阅文件；不表示全部运行时场景已经测试）。OCR 另排除 51 个 Markdown/日志/lock 等 unsupported_ext 文件；本轮额外读取相关计划、标准、状态变更、实现记录和关键日志，未逐条独立重跑全部历史日志。

以下每项均为 reviewed；新增 JSON 证据使用当前独立重生成结果做结构化比对，显式处理预期 source_revision 差异。

| path | status | disposition |
| --- | --- | --- |
| Cargo.toml | modified | reviewed |
| scripts/check-architecture.ps1 | modified | reviewed |
| scripts/plan0014-wave5a.ps1 | added | reviewed |
| tools/module-release-tool/Cargo.toml | added | reviewed |
| tools/module-release-tool/consumers/identity/Cargo.toml | added | reviewed |
| tools/module-release-tool/consumers/identity/src/lib.rs | added | reviewed |
| tools/module-release-tool/consumers/identity/tests/public_contract.rs | added | reviewed |
| tools/module-release-tool/consumers/policy/Cargo.toml | added | reviewed |
| tools/module-release-tool/consumers/policy/src/lib.rs | added | reviewed |
| tools/module-release-tool/consumers/policy/tests/public_contract.rs | added | reviewed |
| tools/module-release-tool/specs/identity.json | added | reviewed |
| tools/module-release-tool/specs/policy.json | added | reviewed |
| tools/module-release-tool/src/canonical.rs | added | reviewed |
| tools/module-release-tool/src/lib.rs | added | reviewed |
| tools/module-release-tool/src/main.rs | added | reviewed |
| tools/module-release-tool/src/migrations.rs | added | reviewed |
| tools/module-release-tool/src/packaging.rs | added | reviewed |
| tools/module-release-tool/src/repo.rs | added | reviewed |
| tools/module-release-tool/src/spec.rs | added | reviewed |
| tools/module-release-tool/src/surface.rs | added | reviewed |
| tools/module-release-tool/src/tempdir.rs | added | reviewed |
| tools/module-release-tool/tests/canonical.rs | added | reviewed |
| tools/module-release-tool/tests/compatibility.rs | added | reviewed |
| tools/module-release-tool/tests/packaging.rs | added | reviewed |
| tools/module-release-tool/tests/pilot_candidates.rs | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/identity/artifact-manifest.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/identity/contract-surface.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/identity/migration-mapping.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/identity/module.manifest.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/identity/release-bundle.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/identity/release.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/identity/verification.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/policy/artifact-manifest.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/policy/contract-surface.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/policy/migration-mapping.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/policy/module.manifest.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/policy/release-bundle.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/policy/release.json | added | reviewed |
| docs/reviews/2026-09-29-wave5a-gates/artifacts/policy/verification.json | added | reviewed |
