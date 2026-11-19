# 工作区审计与清理报告（2026-10-22）

> 文档类型：Report
> 状态：Completed
> 日期：2026-10-22
> 关联计划：PLAN-0014（Active / 等待独立复评）
> 范围：本地工作区、构建产物、C 盘用户临时目录

## 1. 结论

1. 清理构建产物与临时文件共约 **34.8 GB**：`target/` 27.53 GB，C 盘约 7.2 GB。
2. 审计发现并修复了一个 **P0 工作树缺陷**：`scripts/test-plan0014-wave5b.ps1` 的
   `Remove-Tree` 定义被删、8 处调用仍在，门禁脚本实际不可运行。已恢复到 HEAD，
   blob 与 closeout 记录的 `71d44b8554ef89b13184ec3ee06bb38e238f99bb` 一致。
3. 恢复后离线门禁回归通过：**113 例，112 matched / 1 BLOCKED / 0 mismatched，exit 0**，
   与 `6bcdd89` 的记录相同；`cargo fmt --check`、`scripts/check-architecture.ps1` 通过。
4. 项目状态未变化：分支仍位于 `origin/main` 之后 58 个提交，未推送、未合并；
   PLAN-0014 仍为 **READY FOR REVIEW / NOT INTEGRATED，R2 BLOCKED**。

## 2. 清理明细

### 2.1 工作区 `target/`

| 内容 | 大小 |
|---|---|
| `debug/`（deps 17.10 GB、incremental 6.85 GB、build/examples 0.58 GB） | 26.29 GB |
| 15 个测试轮次 scratch build 目录（`w5b-review-*`、`wave5b-*`、`wave5a-*` 等） | 约 1.24 GB |
| `target/` 根目录 20 个散落 scratch 文件（`msg-*.txt`、`run-gates.ps1`、`ignored-*.json` 等） | 约 0.15 MB |
| **合计** | **27.53 GB** |

同类目录已全部删除，包括 `w5b-review-1009-build`（1.0 GB）、
`wave5b-final-independent-20261008`（867 MB）。junction/symlink 以“只删链接”处理，
未递归进入目标。

### 2.2 C 盘临时目录

| 内容 | 大小 | 性质 |
|---|---|---|
| `%TEMP%\a0zjpnvs`、`r5nj3gom` | 3,677 MB + 2,571 MB | Visual Studio 安装器解包残留（无安装进程） |
| `workbuddy-update-x64` | 429 MB | 更新包 |
| `vscode-stable-user-x64*` | 232 MB + 205 MB | VS Code 更新包 |
| `%TEMP%\VS` | 59 MB | 安装器临时文件 |
| 本项目测试 scratch（`wave5b-*`、`w5b-*`） | 9 个空目录 | 门禁驱动的 junction/queue 测试残留 |
| 过期 DSH 工作区快照 | 54 MB | 2026-10-14/15 会话快照 |
| **合计** | **约 7.2 GB** | |

C 盘可用空间 13.70 GB → **20.76 GB**；F 盘 368.34 GB → 406.24 GB（含验证重建的 436 MB target）。

## 3. 审计发现

| 编号 | 级别 | 发现 | 处理 |
|---|---|---|---|
| F1 | **P0** | `scripts/test-plan0014-wave5b.ps1` 工作树版本删除 `Remove-Tree`（−175 行），但 8 处调用仍在；仓库内无其他定义，脚本首个清理路径即失败 | 已 `git checkout` 恢复；AST 复核定义存在、调用 8 处；blob 与 closeout 一致 |
| F2 | P1 | 干净 `target/` 下门禁无法启动：默认 scratch 父目录 `target/tmp` 不存在，驱动直接 `throw`（本次首跑即复现） | 记录为已知缺口；本次用 `New-Item target\tmp` 绕过。修复应让驱动自行创建父目录，但会改动候选 `6bcdd89`，留待复评后补丁 |
| F3 | P1 | 工作区卷 **F: 为 exFAT**，无 reparse point，`P2-catalog-symlink` 在本机永久 BLOCKED；link 用例依赖 C: NTFS 临时目录 | 无法本地修复；建议 CI（Linux/NTFS）覆盖，R2 准入时显式登记为“仅 CI 覆盖” |
| F4 | P2 | 12 个 gate 证据目录（1,307 文件、8.2 MB）按设计 untracked，无异地副本；本次清理已删除 closeout §10 指向的 `target/wave5b-q123` scratch 副本 | 已在 closeout §10 加日期脚注指向权威副本；证据归档策略待复评方决定 |
| F5 | P2 | `docs/OPERATIONAL_RUNBOOK.md` 与 `scripts/clean-target.ps1` 为游离未跟踪文件，内容把 target 写成“70+GB”（实测 26–28 GB） | 运行手册并入 `docs/operations/RUNBOOK.md` §9；清理脚本收编并加固；草稿删除 |
| F6 | P3 | `.scratch/` 仅被 `.git/info/exclude`（本机）忽略，其他克隆会看到未跟踪内容 | 记录建议：移入 `.gitignore` 或删除；本次未动（PLAN-0009 演练设计证据，0.5 MB） |

## 4. 已执行的变更

1. 恢复 `scripts/test-plan0014-wave5b.ps1` 到 HEAD（无 diff，不产生提交）。
2. 加固 `scripts/clean-target.ps1`：修正 `$matches` 自动变量误用；构建中拒绝执行；
   junction/symlink 只删链接；默认清空 `target/`，`-KeepRelease` 保留 release；
   `-DryRun` 预览；校验目标确为仓库 `target/`。
3. `docs/operations/RUNBOOK.md`：新增 §9「构建产物与临时文件清理」，并在 §8 记录
   F2 已知缺口；更新“最后更新”日期。
4. 删除 `docs/OPERATIONAL_RUNBOOK.md`（内容已合并）。
5. `docs/reviews/2026-10-09-wave5b-s1-rereview-repair-closeout.md` §10：加 2026-10-22
   脚注，说明 `target/wave5b-q123` 已随清理删除、权威副本位置不变。

## 5. 验证

| 检查 | 结果 |
|---|---|
| `git hash-object scripts/test-plan0014-wave5b.ps1` | `71d44b8554ef89b13184ec3ee06bb38e238f99bb`，与 closeout 记录一致 |
| PowerShell AST | `Remove-Tree` 已定义；调用点 8 处（200/548/553/875/903/918/989/990） |
| `scripts/test-plan0014-wave5b.ps1` 离线门禁 | exit 0；113 例，112 matched，1 BLOCKED（`P2-catalog-symlink`），0 mismatched；证据 `docs/reviews/2026-10-22-restore-verification/` |
| `cargo fmt --all -- --check` | exit 0 |
| `scripts/check-architecture.ps1` | `Architecture fitness: PASS`（Cargo metadata + OpenAPI contract） |
| `git diff --check` | exit 0 |

**NOT RUN（明确记录，不视为通过）**：`cargo check/clippy/test --workspace`、
`scripts/check-openapi.ps1`、数据库/对象存储/Provider 契约测试、secret/漏洞/许可证/镜像扫描。
原因：本次被跟踪变更只涉及文档与 PowerShell 运维脚本，Rust 源码与 `Cargo.lock` 与
`6bcdd89` 相同；完整工作区门禁已在 closeout §8 于该候选记录，且清理后首次全量构建
代价高。独立复评仍应在复评时重新执行。

## 6. 保留项（未删除）

- `docs/reviews/**/`：1,307 个证据文件、8.2 MB，复评需要。
- `~/.cargo`（2.16 GB）与 `~/.rustup`（10.26 GB，12 个 toolchain）：共享缓存，
  非本项目临时产物；`rust-toolchain.toml` 固定 1.94.1。
- `%LOCALAPPDATA%\pnpm`（4.16 GB）与 `npm-cache`（0.82 GB）：前端共享缓存。
- `%TEMP%` 根下 8×141.7 MB 随机后缀文件：某常驻程序的活动文件，非本项目产物。
- `.scratch/`（0.5 MB）：PLAN-0009 C 迁移演练设计证据，仅本机忽略。
- `target/` 现有 436 MB：本次验证产生的 `module-release-tool` 构建，保留以缩短复评构建时间。

## 7. 剩余风险

1. gate 证据只在本地、无备份；工作树清理类操作可能再次误删被文档引用的 scratch。
2. F: 为 exFAT，符号链接类门禁在本机不可执行，R2 证据完整性依赖 CI。
3. 分支 58 个提交未推送；本地磁盘事故会丢失全部候选历史（含 `6bcdd89`）。
4. 门禁驱动对空 `target/` 不健壮（F2），新环境首次运行会失败。

## 8. 下一步

| 优先级 | 动作 | 完成定义 |
|---|---|---|
| P0 | 已完成：恢复脚本并回归离线门禁 | 113 例 0 mismatched，证据已落盘 |
| P1 | 独立复评候选 `6bcdd89`（含本次恢复后的工作树只读核对） | 复评报告给出 APPROVED / CHANGES REQUESTED，恢复项被单独记录 |
| P1 | 推送分支到远端并开 PR（强烈建议，消除单点磁盘风险） | `origin` 可见 `codex/plan-0014-wave5a-local-release-pilot` |
| P2 | R2 admission 决策（PLAN-0014 §10） | 明确 P2-catalog-symlink 的 CI 覆盖豁免或改为 NTFS 环境执行 |
| P2 | 关闭并归档 PLAN-0014（目标 2026-10-23） | 归档记录最终 SHA、门禁数字、BLOCKED 项、未完成项 |
| P3 | 创建并激活 PLAN-0016 Contract Business Vertical Slice（10/24 起） | 计划含 Bounded Context/数据所有权、API/Event、幂等与并发、安全、质量属性、Fitness Functions、文档同步、回滚与 DoD |
| P3 | 修补 F2：驱动在 scratch 父目录缺失时自行创建 | 干净 `target/` 下门禁可直接运行，并有回归用例 |
| P3 | 环境治理（可选）：`pnpm store prune`、卸载旧 toolchain、或迁移 `CARGO_HOME`/`RUSTUP_HOME` 到 F: | C 盘再释放 4–5 GB，且不影响固定工具链 |
