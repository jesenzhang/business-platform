# Wave 5B-S1 生成与门禁脚本

这四个脚本是 Wave 5B-S1 离线 catalog 门禁的**可复现入口**。它们入库的原因：本轮生成器一度只存在于临时目录，而评审无法复现的证据不算证据。

| 脚本 | 作用 | 之后必须做什么 |
|---|---|---|
| `generate-wave5b-fixtures.ps1` | 重建共享 fixture 树 `tools/module-release-tool/tests/fixtures/wave5b/catalogs/**` 与其 `EXPECTATIONS.json`（50 个 case 目录，期望表 50 行） | 两个读取者都要重跑：`pwsh scripts/test-plan0014-wave5b.ps1` 与 `cargo test -p module-release-tool --test wave5b_catalog` |
| `generate-wave5b-pinned-history.ps1` | 从 git blob 字节重建钉住历史表 `…/fixtures/wave5b/history/pinned-baseline-<sha>.json`（SHA-256 of `git cat-file blob <sha>:<path>`；**git 对象 id 不是摘要**） | 换一个 SHA 是**设计级动作**（改变"历史"钉在何处），必须经独立审阅，不能自行换 |
| `run-wave5b-gates.ps1` | 按审阅 §3.9 分 `fast` / `heavy` / `post` 三相跑门禁，每条命令一份日志（命令行、起止时间、退出码）写入 `docs/reviews/2026-09-30-wave5b-s1-gates/gates/` | 日志是证据：**不得删改**；重跑写新文件名并保留旧日志 |
| `build-wave5b-closeout.ps1` | 从 gate 日志与 `wave5b-cases/*.csv` 就地填充 closeout 的证据段落（门禁表、逐 case 对账表、工作区对账、固定历史证明、catalog 输出） | 只读证据不改代码；无占位符时是 no-op |

## 用法

```powershell
pwsh tools/module-release-tool/scripts/generate-wave5b-fixtures.ps1 -Repo .
pwsh tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1 -Repo .
pwsh tools/module-release-tool/scripts/run-wave5b-gates.ps1 -Phase fast -Repo .
pwsh tools/module-release-tool/scripts/build-wave5b-closeout.ps1 -Repo . -DocsSha <文档提交 SHA>
```

## 跑之前要知道的三件事

1. **不要并发跑 cargo**：同一 target 目录里并发调用会让 validator 无输出，驱动把它判成"无法计分"直接抛错（而不是当成 fail）。门禁必须串行；`cargo test --workspace` 用隔离 `-TargetDir`（设计包 §9.4 对 V14 的规定）。
2. **fixture 树按 LF 入库**：`.gitattributes` 只对 `tools/module-release-tool/tests/fixtures/wave5b/**` 与 `migrations/modules/**` 设了 `text eol=lf`。历史 `migrations/` 故意不设——C1 的 CRLF 兼容由 blob 摘要承担，不改写历史。因此脚本按 LF 解析克隆出来的历史 manifest 会解析不到行（克隆出来是 CRLF）。
3. **本机限制**：仓库卷是 exFAT（无 reparse point），创建符号链接还需管理员权限，因此 `P2-catalog-symlink` 在 Windows 上如实记 BLOCKED；同族的 junction 变体可以真实创建并已实测拒绝。
