# PLAN-0014 Wave 5A Repair 1 独立审阅与 Repair 2 实施指令

日期：2026-09-30。结论：**NEEDS REPAIR**。

## 1. 审阅锚点与结论

- 分支：`codex/plan-0014-wave5a-local-release-pilot`。
- 当前 HEAD：`77a4fbd2ae89b5cec8ac2e6f482eba8722295387`；审阅开始时工作树干净。
- Repair 1 代码候选：`69d9a6b01184db7d9f3d4aa3b688fb569387b2fc`。
- 该候选到当前 HEAD 的 `tools/`、`scripts/`、`Cargo.toml`、`Cargo.lock` 差异为空。
- OCR 审阅范围：`f198454473ad6b2826ed6cd602845a0ec6bb9949..77a4fbd2ae89b5cec8ac2e6f482eba8722295387`。

F2–F5 原缺陷已修复：compare 校验两侧物理完整性；未知 schema/algorithm 与记录绑定被拒绝；消费者隔离使用解析后的依赖图与规范路径；负例使用独立副本、基线校验、有效 JSON 注入及原因断言。这里确认的是原缺陷的修复，不代表发布、生产或 R2 验收。

**F1 部分修复，仍有两个 P1 缺口。** 当前检查能证明一些输入文件与提交相同，却不能证明所有派生产物来自该提交，也不能证明提交中应包含的源码全部存在。下一步限定为 Wave 5A Repair 2。

## 2. 本轮独立证据

| 项目 | 本轮结果 |
| --- | --- |
| `cargo test -p module-release-tool --all-features` | 52 passed，0 failed，0 ignored |
| `scripts/test-plan0014-wave5a.ps1` | 9 PASS |
| `scripts/check-architecture.ps1` | PASS，含 Cargo metadata / OpenAPI 检查 |
| HEAD 生成 identity / policy | 成功；21 / 23 个成员 |
| 修改派生 Cargo.toml 并重算完整性摘要 | verify 错误地 exit 0 / verified |
| 隔离检出副本删除提交内 testing.rs | generate 错误地 exit 0 / generated，identity 仅 20 个成员 |
| 上述残缺 packed crate 的 all-features 编译 | exit 1，E0583：缺少 testing 模块 |
| 完整 workspace fmt/check/clippy/test | 本轮 NOT RUN；Repair 1 日志属于历史实施证据 |
| PostgreSQL/S3/模型真实环境与发布验收 | 本轮 NOT RUN |

复现材料位于 gitignored `target/wave5a-repair1-review-20260930/`。测试没有删除原仓库源码，没有改动原候选；残缺候选的编译会在其 scratch 副本写 Cargo.lock，复现中的 generate 成功发生在该编译之前。

## 3. F1-A：派生 Cargo.toml 未从固定提交重新计算（P1）

定位：`tools/module-release-tool/src/packaging.rs:583-618`、`:1422-1442`、`:1660-1745`。

`source-inputs-match-revision` 对当前输入文件进行提交比对；`packed-crate-bindings` 主要验证 packed crate name/version。它们没有将完整 packed Cargo.toml 与固定提交的 Cargo.toml、Cargo.lock、spec 派生结果比较。验证输出称其验证了派生来源，实际检查不足以支持该声明。

复现步骤：

1. 从当前 HEAD 生成干净 identity 候选并复制到 `derived-cargo-tamper`。
2. 把 packed Cargo.toml 的 `async-trait` 从 `=0.1.92` 改成 `=99.0.0`。
3. 根据原 v1 算法重算物理成员清单和完整性摘要，更新 artifact-manifest；保留 source_revision、release 身份以及纯声明摘要。
4. 执行 `target/debug/module-release-tool.exe verify --artifact <副本>`。
5. 实际 exit 0、status=verified，来源检查与 packed bindings 全部 PASS。

输出：`target/wave5a-repair1-review-20260930/derived-cargo-tamper-verification.txt`。变更后完整性摘要：`03cae52234b9df82016d787feafbdfaee187f70905b85612f7a7dae5a2f9217f`。

这里要求的是固定提交与生成字节之间的独立对应关系。物理完整性摘要在此负例中刻意保持正确，因此必须由来源派生检查拒绝。此发现不要求引入签名或外部信任服务。

## 4. F1-B：源码集合取自当前磁盘，遗漏提交成员仍能生成（P1）

定位：`packaging.rs:1210-1244`、`:1361-1413`、`:1772-1815`。

`collect_build_inputs` 经 `crate_source_paths` 枚举当前目录；`assert_source_members_match` 再与同一磁盘集合比较。Git blob 检查只覆盖被枚举出的文件。被删除的 tracked 源码不进入两侧集合，因而被漏检。

复现步骤：

1. 在 `incomplete-checkout` 建立所需输入的隔离副本，以 `.git` 文件只读指向原仓库 Git object database，保持相同 HEAD。
2. 删除副本中的 `crates/identity/src/testing.rs`。
3. `generate --repo <副本> --module identity --out <独立输出>` 返回 generated，identity 从 21 成员减少为 20，内部 verify 同样成功。
4. 对其 packed identity 执行 `cargo check --offline --all-features --manifest-path <packed Cargo.toml> --target-dir <独立 target>`，返回 E0583：`src/lib.rs:18` 声明的 testing 模块文件不存在。

输出：`target/wave5a-repair1-review-20260930/missing-input-generation.txt`。残缺候选摘要：`350da720a25c56f3e294a58da3d9046a91bae5143493df24936bff0d78c92dc9`。

原提交候选并未缺文件；此反例证明生成器的拒绝门禁不完整。

## 5. 交给 zcode 的精确实施指令

### 5.1 起点、范围与权威材料

在 `F:\Workspace\business-platform` 执行 **PLAN-0014 Wave 5A Repair 2**。先记录实际分支、HEAD、工作树。以本报告列出的 HEAD 为审阅起点；若代码锚点已变化，先对比新增改动再继续，保留所有无关修改。

阅读 AGENTS.md、PLAN-0014、2026-09-29-wave5a-independent-review.md、本报告、Repair 1 closure，以及架构 manifest、server baseline、数据所有权、安全、代码架构、API/Event 标准、fitness 标准和 Rust 标准中与本地发布工具相关的约束。读目标 Cargo.toml / src/lib.rs 后实施。

责任范围：

- 优先修改 `tools/module-release-tool/src/git.rs`、`repo.rs`、`provenance.rs`、`packaging.rs`；确有需要才调整 `spec.rs`、`lib.rs`、`main.rs`。
- 增补 `tools/module-release-tool/tests/provenance.rs`、`verification_guards.rs` 或专门来源回归文件；复用现有测试设施。
- 脚本只修改 `scripts/plan0014-wave5a.ps1`、`scripts/lib/plan0014-wave5a-lib.ps1`、`scripts/test-plan0014-wave5a.ps1` 中接入本轮来源负例所需部分。
- 如新增只读 Git 操作，按最小范围同步 `scripts/check-architecture.ps1` 的允许规则。
- 新建 Repair 2 证据目录与 closeout；在 PLAN-0014、ARCHITECTURE_STATUS、docs/README 更新导航和当前状态。历史 Repair 1 证据保持原样，以追加说明关联本报告。

能力归属仍是本地 module release tooling，不改变 Identity/Policy 数据所有权、业务 API、迁移执行语义或平台 contract crate 边界。

### 5.2 先建立失败回归

修实现前，加入并运行以下反例，留存修复前原始命令、退出码、失败断言：

1. packed Cargo 依赖版本改动，重算合法 inventory/integrity digest 后，物理完整性校验成功，但来源派生检查必须失败。
2. 再覆盖一个不同类别的 Cargo 改动，如 features 或 dependency path；不能只硬编码拒绝 `99.0.0`。
3. 在隔离检出目录删除固定提交的 `testing.rs`，generate 必须拒绝，且不得留下可被当作成功候选的目录。
4. 从完整候选移除该成员并重算 inventory/integrity，verify 必须因提交成员集合不符失败。
5. 在隔离输入目录增加未提交源码，generate 必须拒绝，不得把它当作固定提交的成员。

每个注入独立副本、先验证基线、确认注入生效；重算摘要采用现有公开算法，不能修改待测来源判定。断言非零退出及明确 check identifier，保留“完整性仍正确”的证据，防止负例退化成普通 checksum 测试。

### 5.3 建立固定提交输入视图

1. 解析并固定 source_revision 为真实 commit；只读读取该提交对象，未知提交、缺对象、缺必要输入均 fail closed。
2. 以 Git tree 为来源枚举预期源码集合，例如带 NUL 分隔的 `git ls-tree`；不要从当前目录清单构造提交预期集合。检查路径、类型和映射，避免空格或特殊字符拆分错误。
3. 对 generate，校验所需工作树输入与提交字节及成员集合一致，拒绝修改、删除、额外相关源码。无关文件变化依既有范围规则处理。
4. 对 verify，从候选宣称的固定提交构造预期输入和成员集合；当前 HEAD 前进或当前输入文件变化不应改变该提交的预期内容。可用 Git 对象缺失时明确 BLOCKED/error，不能退回磁盘信任。
5. 源码、Cargo 配置、lock、toolchain、module spec、迁移目录/manifest 等既有相关输入均由这个固定提交视图提供；复用现有解析/编译/渲染逻辑，不建立另一套弱化 verifier。

### 5.4 从固定提交重新计算派生产物

1. 重新计算完整 packed Cargo.toml，并比较完整规范化内容，包括依赖版本、path、features、edition 与已有 lints/metadata 规则；仅比较 name/version 不足够。
2. 重新计算 spec 对应的 module manifest、contract surface、migration mapping 与既有派生描述，校验其与该提交的对应关系。不能把候选内声明再次编译后自洽，视为已证明它来自提交。
3. 版本等明确允许的生成参数从经过约束的 release 输入取得；记录允许参数及其绑定，不允许候选借此任意覆盖提交中定义的依赖、能力或路径。
4. 未变换文件比对提交 blob；变换文件比对重新生成内容；成员集合比对 Git tree 所导出的预期清单。三类都通过才输出来源 PASS。
5. 保留物理 integrity 与来源派生检查的独立 check identifier 和错误原因。验证 detail 只描述实际完成的检查。
6. 保留 v1 协议、既有 pure declaration digest / artifact integrity digest 的职责分离。若规范化内容完全相同，identity/policy 当前有效候选内容与摘要应稳定；如确需兼容变更，先明确原因并更新契约说明，不可静默重定义算法。

不引入签名、联网注册、publish/install/deploy、运行时动态加载、数据库或新业务 API。不通过全盘 dirty 检查替代固定提交验证。

### 5.5 正向兼容与完整门禁

必须覆盖：identity/policy 正常生成、独立进程 verify、重复生成 compare、packed crate 与消费者构建；固定候选在后续文档 closeout HEAD 仍通过；隔离测试仓库中当前输入变动后，历史固定候选仍按原提交验证；提交对象缺失时拒绝。F2–F5 原回归保持通过。

在仓库根目录，按 RTK 要求逐条执行并保存输出、退出码与实际 revision：

```powershell
rtk proxy cargo test -p module-release-tool --all-features
rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1 -EvidenceDir target/wave5a-repair2/script-regression
rtk proxy cargo fmt --all -- --check
rtk proxy cargo check --workspace --all-targets --all-features
rtk proxy cargo clippy --workspace --all-targets --all-features -- -D warnings
rtk proxy cargo test --workspace --all-features
rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1
```

修代码完成后固定代码候选 SHA，再在全新目录运行已有 pilot 脚本：

```powershell
rtk proxy pwsh -NoProfile -File scripts/plan0014-wave5a.ps1 -OutRoot target/wave5a-repair2/pilot -EvidenceDir target/wave5a-repair2/pilot-evidence -SkipOutsideConsumer
```

该命令跳过仓库外目录 lane，必须记录该 lane 本轮 NOT RUN，不能声称全部 pilot lane 通过；若环境授权允许仓库外目录写入，再单独执行完整 lane 并留存真实证据。历史仓库外 lane 与本轮证据分列。

任何失败先修复或标明 BLOCKED；不能用历史绿色日志替代当前门禁。ignored 测试逐项分类，保持 PostgreSQL、PostgreSQL/S3、S3 和模型依赖分母，不能将 70 项全部称为 PostgreSQL。真实环境、漏洞/许可证/Secret 等未执行门禁逐项 NOT RUN，不能写总验收通过。

### 5.6 交付与停止条件

- 新建 `docs/reviews/2026-09-30-wave5a-repair2-gates/`，归档 RED/GREEN 回归、当前门禁、候选 verify/compare、消费者 metadata 和新来源负例。不得覆盖 Repair 1 归档。
- 新建 Repair 2 closeout，记录审阅起点、代码候选 SHA、文档 HEAD、F1-A/F1-B 对应代码与证据、成员数量/摘要、门禁与未运行项。
- 文档 closeout 后再验证同一固定候选，证明验证不绑定最新工作树输入。
- 完成时报告 `READY FOR REVIEW / NOT INTEGRATED`，附实际 dirty 状态与提交列表；独立审阅尚未执行，不能自行写 accepted。
- 停在 Wave 5A Repair 2。R2 与原计划开放门禁保持未关闭；不启动 Wave 5B、不改 main、不 push/merge/tag/publish/install/deploy。

## 6. OCR 覆盖说明

OCR total_files=147，reviewable_files=37，excluded_files=110（unsupported_ext）。37 个 reviewable 条目中 reviewed=35，skipped=2，coverage_rate=94.59%。另按任务需要阅读了权威计划、closeout 和证据说明；110 个排除条目不等于全部逐文件执行或全面审阅。

跳过两项历史脚本：`red-run/pre-fix-orchestration-script.ps1` 是旧实现的归档快照，本轮审阅当前执行脚本；`red-run/reproduce-red-counterexamples.ps1` 是历史 RED 辅助脚本，本轮未重跑历史 RED，采用当前代码独立反例。路径前缀均为 `docs/reviews/2026-09-29-wave5a-repair1-gates/`。

逐文件清单见下表。

| 路径 | Git 状态 | 审阅状态 |
| --- | --- | --- |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/25-consumer-identity-cargo-metadata.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/25-consumer-policy-cargo-metadata.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/34-outside-consumer-identity-cargo-metadata.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/identity/artifact-manifest.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/identity/contract-surface.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/identity/migration-mapping.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/identity/module.manifest.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/identity/release-bundle.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/identity/release.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/identity/verification.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/policy/artifact-manifest.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/policy/contract-surface.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/policy/migration-mapping.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/policy/module.manifest.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/policy/release-bundle.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/policy/release.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/artifacts/policy/verification.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/ignored-tests-inventory.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/red-run/pre-fix-orchestration-script.ps1` | added | skipped，原因见上文 |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/red-run/red-run-summary.json` | added | reviewed |
| `docs/reviews/2026-09-29-wave5a-repair1-gates/red-run/reproduce-red-counterexamples.ps1` | added | skipped，原因见上文 |
| `scripts/check-architecture.ps1` | modified | reviewed |
| `scripts/lib/plan0014-wave5a-lib.ps1` | added | reviewed |
| `scripts/plan0014-wave5a.ps1` | modified | reviewed |
| `scripts/test-plan0014-wave5a.ps1` | added | reviewed |
| `tools/module-release-tool/src/git.rs` | added | reviewed |
| `tools/module-release-tool/src/isolation.rs` | added | reviewed |
| `tools/module-release-tool/src/lib.rs` | modified | reviewed |
| `tools/module-release-tool/src/main.rs` | modified | reviewed |
| `tools/module-release-tool/src/packaging.rs` | modified | reviewed |
| `tools/module-release-tool/src/provenance.rs` | added | reviewed |
| `tools/module-release-tool/src/repo.rs` | modified | reviewed |
| `tools/module-release-tool/src/spec.rs` | modified | reviewed |
| `tools/module-release-tool/tests/isolation.rs` | added | reviewed |
| `tools/module-release-tool/tests/pilot_candidates.rs` | modified | reviewed |
| `tools/module-release-tool/tests/provenance.rs` | added | reviewed |
| `tools/module-release-tool/tests/verification_guards.rs` | added | reviewed |
