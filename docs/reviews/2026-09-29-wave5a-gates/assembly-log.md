# Wave 5A execution log index

Every row is one command run by `scripts/plan0014-wave5a.ps1`. Temporary root:
$OutRoot = F:\Workspace\business-platform\target/wave5a (outside the repository).

| step | command | exit | log |
| --- | --- | --- | --- |
| generate identity (attempt run-a) | `cargo run -q -p module-release-tool -- generate --module identity --out F:\Workspace\business-platform\target\wave5a\run-a` | 0 | 10-generate-run-a-identity.log |
| generate policy (attempt run-a) | `cargo run -q -p module-release-tool -- generate --module policy --out F:\Workspace\business-platform\target\wave5a\run-a` | 0 | 10-generate-run-a-policy.log |
| generate identity (attempt run-b) | `cargo run -q -p module-release-tool -- generate --module identity --out F:\Workspace\business-platform\target\wave5a\run-b` | 0 | 10-generate-run-b-identity.log |
| generate policy (attempt run-b) | `cargo run -q -p module-release-tool -- generate --module policy --out F:\Workspace\business-platform\target\wave5a\run-b` | 0 | 10-generate-run-b-policy.log |
| repeat-build consistency for identity (two temporary roots) | `cargo run -q -p module-release-tool -- compare --left F:\Workspace\business-platform\target\wave5a\run-a\identity-0.1.0-rc.1 --right F:\Workspace\business-platform\target\wave5a\run-b\identity-0.1.0-rc.1` | 0 | 12-compare-identity.log |
| repeat-build consistency for policy (two temporary roots) | `cargo run -q -p module-release-tool -- compare --left F:\Workspace\business-platform\target\wave5a\run-a\policy-0.1.0-rc.1 --right F:\Workspace\business-platform\target\wave5a\run-b\policy-0.1.0-rc.1` | 0 | 12-compare-policy.log |
| independent verification of the identity candidate | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\run-a\identity-0.1.0-rc.1` | 0 | 13-verify-identity.log |
| independent verification of the policy candidate | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\run-a\policy-0.1.0-rc.1` | 0 | 13-verify-policy.log |
| build the packed identity crate as its own workspace root (--all-features) | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a\crate-build\identity\Cargo.toml --all-features --target-dir F:\Workspace\business-platform\target\wave5a\build\identity` | 0 | 40-crate-build-identity.log |
| re-verify the identity artifact after building: the build must not have touched it | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\run-a\identity-0.1.0-rc.1` | 0 | 41-verify-after-build-identity.log |
| build the packed policy crate as its own workspace root (--all-features) | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a\crate-build\policy\Cargo.toml --all-features --target-dir F:\Workspace\business-platform\target\wave5a\build\policy` | 0 | 40-crate-build-policy.log |
| re-verify the policy artifact after building: the build must not have touched it | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\run-a\policy-0.1.0-rc.1` | 0 | 41-verify-after-build-policy.log |
| build the out-of-workspace identity consumer against the released artifact | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\identity\Cargo.toml --all-features` | 0 | 20-consumer-identity-build.log |
| run the out-of-workspace F:\Workspace\business-platform\target\wave5a\consumers\identity test that calls the published contract | `cargo test --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\identity\Cargo.toml --all-features` | 0 | 21-consumer-identity-test.log |
| re-build the identity consumer with a locked dependency set, offline | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\identity\Cargo.toml --all-features --locked --offline` | 0 | 22-consumer-identity-offline-locked.log |
| resolved dependencies and features of the identity consumer | `cargo tree --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\identity\Cargo.toml -e features,normal` | 0 | 23-consumer-identity-tree.txt |
| build the out-of-workspace policy consumer against the released artifact | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\policy\Cargo.toml --all-features` | 0 | 20-consumer-policy-build.log |
| run the out-of-workspace F:\Workspace\business-platform\target\wave5a\consumers\policy test that calls the published contract | `cargo test --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\policy\Cargo.toml --all-features` | 0 | 21-consumer-policy-test.log |
| re-build the policy consumer with a locked dependency set, offline | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\policy\Cargo.toml --all-features --locked --offline` | 0 | 22-consumer-policy-offline-locked.log |
| resolved dependencies and features of the policy consumer | `cargo tree --manifest-path F:\Workspace\business-platform\target\wave5a\consumers\policy\Cargo.toml -e features,normal` | 0 | 23-consumer-policy-tree.txt |
| NEGATIVE: modifying a packed source must break artifact verification | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\identity` | 1 | 50-tamper-content-identity.log |
| NEGATIVE: a missing payload file must break artifact verification | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\identity` | 1 | 51-tamper-missing-file-identity.log |
| NEGATIVE: a wrong module_version in the release identity must be rejected | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\identity` | 1 | 52-tamper-wrong-version-identity.log |
| NEGATIVE: a payload whose module_id differs from the pinned identity must be rejected | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\identity` | 1 | 53-tamper-wrong-module-id-identity.log |
| NEGATIVE: modifying a packed source must break artifact verification | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\policy` | 1 | 50-tamper-content-policy.log |
| NEGATIVE: a missing payload file must break artifact verification | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\policy` | 1 | 51-tamper-missing-file-policy.log |
| NEGATIVE: a wrong module_version in the release identity must be rejected | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\policy` | 1 | 52-tamper-wrong-version-policy.log |
| NEGATIVE: a payload whose module_id differs from the pinned identity must be rejected | `cargo run -q -p module-release-tool -- verify --artifact F:\Workspace\business-platform\target\wave5a\tamper\policy` | 1 | 53-tamper-wrong-module-id-policy.log |

RESULT: all steps matched their expected exit codes.

Not executed by this script, and therefore NOT RUN: PostgreSQL migration execution, real
PostgreSQL installation of a single module, publish/register/install/deploy of any kind,
vulnerability and licence scanning, and CI. See the record document for the full table.
