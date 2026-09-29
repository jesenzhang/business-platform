| `rtk proxy cargo test -p module-release-tool --all-features` | 0 | `01-test-module-release-tool.log` |
| `rtk proxy cargo test -p business-module-contracts -p business-application-compiler --all-features` | 0 | `02-test-pure-crates.log` |
| `rtk proxy cargo fmt --all -- --check` | 0 | `03-fmt-check.log` |
| `rtk proxy cargo check --workspace --all-targets --all-features` | 0 | `04-check-workspace.log` |
| `rtk proxy cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | `05-clippy-workspace.log` |
| `rtk proxy cargo test --workspace --all-features` | 0 | `06-test-workspace.log` |
| `rtk proxy pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 | `07-check-architecture.log` |
| `rtk git diff --check` | 0 | `08-git-diff-check.log` |
| `rtk proxy pwsh -NoProfile -File scripts/test-plan0014-wave5a.ps1 -EvidenceDir docs\reviews\2026-09-29-wave5a-repair1-gates\script-regression` | 0 | `09-script-regression.log` |
