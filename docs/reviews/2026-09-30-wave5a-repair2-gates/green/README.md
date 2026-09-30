# GREEN — 同一批反例在修复后必须被拒

本目录是 `../red/` 那批失败回归的对照面：**命令逐字相同**，只是代码已修复。
RED 与 GREEN 使用同一组用例、同一断言，因此两条日志可以直接对读。

| 文件 | 命令 | 结果 |
|---|---|---|
| `20-green-source-derivation.log` | `rtk proxy cargo test -p module-release-tool --test source_derivation --all-features -- --test-threads=1` | 与 `../red/10-red-unit-regressions.log` 同一条命令：修复前 `0 passed; 9 failed`，修复后 `9 passed; 0 failed`，退出码 0 |
| `21-green-release-tool-suite.log` | `rtk proxy cargo test -p module-release-tool --all-features` | 工具全套（含 Repair 1 的 F2–F5 回归）在修复后全绿 |

对应的脚本层反例（同一注入的编排版本）不在本目录，而在 pilot 的
`negatives/`（`n13`–`n16`）与 `script-regression/`（`R7a`/`R7b`/`R8a`–`R8c`）里；
`../gates/36-script-regression.log` 保存该套件的退出码。

修复前的数字一并留档，便于核对「同一用例、只是换了代码」：

- F1-A 注入后的重算完整性摘要 `03cae52234b9df82016d787feafbdfaee187f70905b85612f7a7dae5a2f9217f`；
- 依赖版本抬升注入后的摘要 `88f5c3bcf1e2f034d2e932caea4ec31c01ba18e7acb2b2ff1d4ea5fc97bff1fc`；
- 备选 spec 候选的完整性摘要 `f1ee4ce2a3e396de91c991fc639584dce74ceab6398281e88909838f9064c816`；
- F1-B 从缺文件的 checkout 生成出的工件摘要 `350da720a25c56f3e294a58da3d9046a91bae5143493df24936bff0d78c92dc9`（`member_count` 20）。

四个值在修复后都不再可能出现在任何候选里：派生成员现在与固定提交重算出的完整内容逐字节比较，
成员集合来自 `git ls-tree -r -z`。