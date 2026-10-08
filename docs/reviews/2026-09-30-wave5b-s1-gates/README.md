# Wave 5B-S1 门禁证据

同一轮门禁的原始输出。任何结论以这里的日志为准；closeout 与 handoff 只是对账与导航，不替代日志。

```text
00-anchors.txt      起止 SHA、钉住的 baseline 提交、宿主版本、影响门禁结果的宿主事实、日志→退出码索引
gates/              每条命令一份日志：头部是 # gate / # command / # started / # finished /
                    # duration-seconds / # exit-code，正文是合并的 stdout+stderr
wave5b-cases/       scripts/test-plan0014-wave5b.ps1 的机器可读证据：
                    <family>-cases.csv（Case/Expect/Actual/Verdict/Detail）+ catalog-reconciliation.csv + 原始 gate 输出
```

## 怎么读

- **退出码不是"越大越坏"**：`25-catalog-check-unknown-check.log` 的 exit 1 是负例的预期结果（未知 check 必须被拒）。每条日志头部写了它跑的到底是哪条命令。
- **失败与阻塞的尝试保留**：`37-test-workspace.log` / `37-test-workspace-attempt-2.log`（默认 target 目录被运行中的开发进程占住二进制）、`37b-test-workspace-isolated-target.log`（隔离 target 目录重跑）、`41-wave5b-offline-catalog-gate.log`（首次跑出 2 条驱动 fixture 缺陷）都没有被删除或改写。
- **BLOCKED 不是 MATCH**：`path-cases.csv` 里 `P2-catalog-symlink` 记 `BLOCKED`，驱动末行也如实播报"能跑的都对上，N 条 BLOCKED"，不会把它算成通过。

## 复跑

```powershell
pwsh tools/module-release-tool/scripts/run-wave5b-gates.ps1 -Phase fast  -Repo .
pwsh tools/module-release-tool/scripts/run-wave5b-gates.ps1 -Phase heavy -Repo .
pwsh tools/module-release-tool/scripts/run-wave5b-gates.ps1 -Phase post  -Repo .
pwsh scripts/test-plan0014-wave5b.ps1 -Repo . -EvidenceDir <新目录>   # 已存在的证据目录需 -Force 才会覆盖
```

三相的分工：`fast` 是 validator 自身的五种调用 + `fmt` + `git diff --check`；`heavy` 是 workspace `check` / `clippy -D warnings` / 相关 crate 测试 / 两条 PG `--no-run`；`post` 是 workspace 全量测试、`check-architecture.ps1`、离线 case 驱动，以及**提交后**才能成立的三条（`git diff --check`、`git status --short`、`-p module-release-tool --all-features`，其中 packaging 测试比较的是提交态输入）。
