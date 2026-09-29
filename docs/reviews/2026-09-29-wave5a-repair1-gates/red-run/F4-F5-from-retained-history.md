# F4 / F5 的修复前证据（来自不可改的既有记录）

F1–F3 的修复前行为在 `red-run-table.md` 里可复现。F4 与 F5 不需要复现：修复前的脚本
本身和它当时产出的日志都还在版本库里，读一遍就能确认这两个 finding 属实。

## F4 — 所谓"隔离检查"只是两次文本扫描

修复前脚本（pre-fix-orchestration-script.ps1 第 253-277 行，逐字摘录）：

```powershell
        # Guard: nothing in the consumer's resolved graph may point back into the
        # source workspace. This is an assertion, not a review note.
        $guardLog = Join-Path $EvidenceDir "25-consumer-$module-path-guard.txt"
        $needle = Join-Path $repo 'crates'
        $needleApps = Join-Path $repo 'apps'
        $lockText = Get-Content -LiteralPath (Join-Path $consumer 'Cargo.lock') -Raw
        $manifestText = Get-Content -LiteralPath $manifestPath -Raw
        $hits = @()
        foreach ($text in @($lockText, $manifestText)) {
            foreach ($line in ($text -split "`n")) {
                if ($line -like "*$needle*" -or $line -like "*$needleApps*") {
                    $hits += $line.Trim()
                }
            }
        }
        $guardLines = @(
            "guard : the consumer dependency graph must contain no path into the source workspace",
            "needle: $needle",
            "needle: $needleApps",
            "scanned: Cargo.lock + Cargo.toml of $consumer",
            "hits   : $($hits.Count)"
        )
        $guardLines += $hits
        if ($hits.Count -eq 0) {
            $guardLines += 'result: PASS'
```

它扫描的输入只有消费者自己的 `Cargo.toml` 与 `Cargo.lock`；needle 是
<repo>\crates / <repo>\apps 这类**反斜杠绝对路径字面量**。因此以下形状一律扫不到：以正
斜杠书写的同一仓库路径、以 `..` 相对消费者目录书写再规范化回仓库的路径、由依赖解析引入而
消费者文件里根本不出现的传递性本地 path 依赖，以及任何不在 `crates` / `apps` 下的仓库路径。

该步在旧证据目录里的输出（`docs/reviews/2026-09-29-wave5a-gates/25-consumer-identity-path-guard.txt`）：

```text
guard : the consumer dependency graph must contain no path into the source workspace
needle: F:\Workspace\business-platform\crates
needle: F:\Workspace\business-platform\apps
scanned: Cargo.lock + Cargo.toml of F:\Workspace\business-platform\target\wave5a\consumers\identity
hits   : 0
result: PASS
```

## F5 — 第 3 例的损坏被第 4 例继承，且两例的拒绝原因逐字相同

修复前脚本（同文件第 318-343 行，逐字摘录）。第 318-319 行在第 3 例之前重新复制了副本，
而第 321-341 行的第 3、4 例之间**没有**重新复制：

```powershell
        Remove-Item -LiteralPath $tampered -Recurse -Force
        Copy-Item -Recurse -Force -LiteralPath (Join-Path $runDirs['run-a'] "$module-$ModuleVersion") -Destination $tampered

        $releasePath = Join-Path $tampered 'release.json'
        (Get-Content -LiteralPath $releasePath -Raw).Replace("`"module_version`": `"$ModuleVersion`"", '"module_version": "9.9.9"') |
            Set-Content -LiteralPath $releasePath -Encoding utf8
        $null = Invoke-Step `
            -Name "tamper-version-$module" `
            -Title "NEGATIVE: a wrong module_version in the release identity must be rejected" `
            -Exe 'cargo' `
            -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', $tampered) `
            -LogFile (Join-Path $EvidenceDir "52-tamper-wrong-version-$module.log") `
            -ExpectExit 1

        $wrongId = Join-Path $tampered 'package\module.manifest.json'
        (Get-Content -LiteralPath $wrongId -Raw).Replace("`"module_id`": `"$module`"", '"module_id": "someone-else"') |
            Set-Content -LiteralPath $wrongId -Encoding utf8
        $null = Invoke-Step `
            -Name "tamper-module-id-$module" `
            -Title "NEGATIVE: a payload whose module_id differs from the pinned identity must be rejected" `
            -Exe 'cargo' `
            -Arguments @('run', '-q', '-p', 'module-release-tool', '--', 'verify', '--artifact', $tampered) `
            -LogFile (Join-Path $EvidenceDir "53-tamper-wrong-module-id-$module.log") `
            -ExpectExit 1

        Remove-Item -LiteralPath $tampered -Recurse -Force -ErrorAction SilentlyContinue
```

第 322 与 333 行各做一次 `.Replace()` 字符串替换来"改 JSON"，而 `Invoke-Step` 只
断言 `-ExpectExit 1`，不看拒绝原因。后果写在旧日志里，两条负例的拒绝行完全一样：

```text
52-tamper-wrong-version-identity.log :
module-release: artifact verification failed [bundle-identity]: release-bundle.json pins a different identity than release.json
53-tamper-wrong-module-id-identity.log :
module-release: artifact verification failed [bundle-identity]: release-bundle.json pins a different identity than release.json
```

即"错误 module_id 被相应检查拒绝"这一结论当时并不成立：第 4 例跑的是仍带着第 3 例版本改动
的副本，先在 bundle 与 release 记录的交叉一致性上被拒，`module.manifest.json` 里那个
`module_id` 从未被任何检查判定过。修复后：每个负例各自复制一份刚验证通过的候选、注入后回
读证明改动落地、并同时断言退出码与被点名的检查 ID（见 `../F1-F5-closure.md` §5 与
`../negatives/<case>/case.md`）。
