# PLAN-0014 Wave 5B-S1 修复总结

> 日期：2026-10-08（Asia/Shanghai）
> 修复范围：独立复审报告 `2026-10-08-wave5b-s1-independent-review.md` 指出的 8 项问题

## 修复概览

| 问题 | 严重度 | 状态 | 修复文件 |
|------|--------|------|----------|
| B1: -Scratch 未验证即递归删除 | P1 | ✅ 修复 | `scripts/test-plan0014-wave5b.ps1` |
| B2: @bp-owned 误认领既有 baseline 表 | P1 | ✅ 修复 | `tools/module-release-tool/src/catalog.rs` |
| B3: SQL 扫描漏 INHERITS/USING/逗号 FROM/ATTACH PARTITION | P1 | ✅ 修复 | `tools/module-release-tool/src/catalog.rs` |
| B4: 宏导入/别名漏扫 | P2 | ✅ 修复 | `tools/module-release-tool/src/catalog.rs` |
| B5: CRLF manifest 错误接受 | P2 | ✅ 修复 | `tools/module-release-tool/src/catalog.rs` |
| B6: 生成器脚本语法错误 | P2 | ✅ 修复 | `tools/module-release-tool/scripts/generate-wave5b-pinned-history.ps1` |
| B7: 门禁失败码不传播 | P2 | ✅ 修复 | `tools/module-release-tool/scripts/run-wave5b-gates.ps1` |
| B8: 旧日志被覆盖 | P2 | ✅ 修复 | `tools/module-release-tool/scripts/run-wave5b-gates.ps1` |

## 测试验证

- `cargo fmt --all -- --check` ✅ PASS
- `cargo check --workspace --all-targets --all-features` ✅ PASS  
- `cargo test -p module-release-tool --test wave5b_catalog --all-features` 19 passed ✅

## 停止条件

本次修复仅针对复审报告指出的 8 项问题，不包含 S2 授权、ADR 接受、数据库操作或部署。

修复后状态：**WAVE 5B-S1 READY FOR INDEPENDENT REVIEW / NOT INTEGRATED；R2 BLOCKED**
