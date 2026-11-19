# PLAN-0014 Wave 5A — assembly log

run directory             : F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z
evidence directory        : F:\Workspace\business-platform\docs\reviews\2026-09-29-wave5a-repair1-gates
repository                : F:\Workspace\business-platform
source revision under test: 69d9a6b01184db7d9f3d4aa3b688fb569387b2fc
working tree dirty entries: 1
outside-repository rebuild: RAN: consumer rebuilt and tested from F:\wave5a-repair1-outside\20260929T075734Z\consumers\identity (outside F:\Workspace\business-platform, same volume).
negative-case records     : 42 file(s) archived under `negatives/`

Every row is one command this script ran, with the exit code it produced and the
exit code it had to produce. "expected 1" rows are negative cases: a refusal is
the result. A step marked blocking stops the run when it fails, so no later
step can report on state that was never produced.

## commands

| step | command | exit | expected | log |
| --- | --- | --- | --- | --- |
| build the release tool | `cargo build -p module-release-tool` | 0 | 0 | `05-build-tool.log` |
| generate identity (attempt run-a) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe generate --module identity --out F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a` | 0 | 0 | `10-generate-run-a-identity.log` |
| generate policy (attempt run-a) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe generate --module policy --out F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a` | 0 | 0 | `10-generate-run-a-policy.log` |
| generate identity (attempt run-b) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe generate --module identity --out F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-b` | 0 | 0 | `10-generate-run-b-identity.log` |
| generate policy (attempt run-b) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe generate --module policy --out F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-b` | 0 | 0 | `10-generate-run-b-policy.log` |
| repeat-build consistency for identity (payload bytes and declaration digests of both sides) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe compare --left F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1 --right F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-b\identity-0.1.0-rc.1` | 0 | 0 | `12-compare-identity.log` |
| independent verification of the identity candidate | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1` | 0 | 0 | `13-verify-identity.log` |
| repeat-build consistency for policy (payload bytes and declaration digests of both sides) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe compare --left F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\policy-0.1.0-rc.1 --right F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-b\policy-0.1.0-rc.1` | 0 | 0 | `12-compare-policy.log` |
| independent verification of the policy candidate | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\policy-0.1.0-rc.1` | 0 | 0 | `13-verify-policy.log` |
| build the packed identity crate as its own workspace root (--all-features) | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\crate-build\identity\Cargo.toml --all-features --target-dir F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\build\identity` | 0 | 0 | `40-crate-build-identity.log` |
| re-verify the identity artifact after building: the build must not have touched it | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1` | 0 | 0 | `41-verify-after-build-identity.log` |
| build the packed policy crate as its own workspace root (--all-features) | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\crate-build\policy\Cargo.toml --all-features --target-dir F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\build\policy` | 0 | 0 | `40-crate-build-policy.log` |
| re-verify the policy artifact after building: the build must not have touched it | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\policy-0.1.0-rc.1` | 0 | 0 | `41-verify-after-build-policy.log` |
| build the out-of-workspace identity consumer against the released artifact | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity\Cargo.toml --all-features` | 0 | 0 | `20-consumer-identity-build.log` |
| run the out-of-workspace identity test that calls a published contract | `cargo test --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity\Cargo.toml --all-features` | 0 | 0 | `21-consumer-identity-test.log` |
| re-build the identity consumer with a locked dependency set, offline | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity\Cargo.toml --all-features --locked --offline` | 0 | 0 | `22-consumer-identity-offline-locked.log` |
| resolved dependencies and features of the identity consumer | `cargo tree --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity\Cargo.toml -e features,normal` | 0 | 0 | `23-consumer-identity-tree.txt` |
| 26-consumer-identity : resolve the consumer dependency graph (cargo metadata, full graph) | `cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc` | 0 | 0 | `26-consumer-identity-cargo-metadata.log` |
| 26-consumer-identity : decide isolation from the resolved graph (module-release guard) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe guard --metadata F:\Workspace\business-platform\docs\reviews\2026-09-29-wave5a-repair1-gates\25-consumer-identity-cargo-metadata.json --consumer F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity --repo F:\Workspace\business-platform --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity` | 0 | 0 | `26-consumer-identity-guard.log` |
| build the out-of-workspace policy consumer against the released artifact | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\policy\Cargo.toml --all-features` | 0 | 0 | `20-consumer-policy-build.log` |
| run the out-of-workspace policy test that calls a published contract | `cargo test --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\policy\Cargo.toml --all-features` | 0 | 0 | `21-consumer-policy-test.log` |
| re-build the policy consumer with a locked dependency set, offline | `cargo build --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\policy\Cargo.toml --all-features --locked --offline` | 0 | 0 | `22-consumer-policy-offline-locked.log` |
| resolved dependencies and features of the policy consumer | `cargo tree --manifest-path F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\policy\Cargo.toml -e features,normal` | 0 | 0 | `23-consumer-policy-tree.txt` |
| 26-consumer-policy : resolve the consumer dependency graph (cargo metadata, full graph) | `cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc` | 0 | 0 | `26-consumer-policy-cargo-metadata.log` |
| 26-consumer-policy : decide isolation from the resolved graph (module-release guard) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe guard --metadata F:\Workspace\business-platform\docs\reviews\2026-09-29-wave5a-repair1-gates\25-consumer-policy-cargo-metadata.json --consumer F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\policy --repo F:\Workspace\business-platform --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\policy-0.1.0-rc.1\package\contract\packages\policy` | 0 | 0 | `26-consumer-policy-guard.log` |
| rebuild the identity consumer from a directory outside the repository | `cargo build --manifest-path F:\wave5a-repair1-outside\20260929T075734Z\consumers\identity\Cargo.toml --all-features` | 0 | 0 | `31-outside-consumer-build-identity.log` |
| test the outside-repository identity consumer | `cargo test --manifest-path F:\wave5a-repair1-outside\20260929T075734Z\consumers\identity\Cargo.toml --all-features` | 0 | 0 | `32-outside-consumer-test-identity.log` |
| 33-outside-consumer-identity : resolve the consumer dependency graph (cargo metadata, full graph) | `cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc` | 0 | 0 | `33-outside-consumer-identity-cargo-metadata.log` |
| 33-outside-consumer-identity : decide isolation from the resolved graph (module-release guard) | `F:\Workspace\business-platform\target\debug\module-release-tool.exe guard --metadata F:\Workspace\business-platform\docs\reviews\2026-09-29-wave5a-repair1-gates\34-outside-consumer-identity-cargo-metadata.json --consumer F:\wave5a-repair1-outside\20260929T075734Z\consumers\identity --repo F:\Workspace\business-platform --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity` | 0 | 0 | `33-outside-consumer-identity-guard.log` |
| n01-payload-byte : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n01-payload-byte\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| one packed source byte changed: identity must fail the integrity digest | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n01-payload-byte\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n02-missing-member : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n02-missing-member\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| a payload member removed: identity must fail the integrity digest | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n02-missing-member\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n03-unknown-algorithm : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n03-unknown-algorithm\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| artifact-manifest.json names an integrity algorithm nobody implements | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n03-unknown-algorithm\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n04-unknown-schema : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n04-unknown-schema\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| artifact-manifest.json declares a schema version this tool never wrote | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n04-unknown-schema\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n05-record-module-id : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n05-record-module-id\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| artifact-manifest.json describes a different module than release.json | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n05-record-module-id\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n06-record-package-digest : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n06-record-package-digest\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| artifact-manifest.json pins a package digest release.json does not claim | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n06-record-package-digest\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n07-bundle-crate-version : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n07-bundle-crate-version\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| release-bundle.json binds a crate version the packed Cargo.toml does not declare | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n07-bundle-crate-version\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n08-bundle-crate-name : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n08-bundle-crate-name\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| release-bundle.json binds a crate the payload does not ship | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n08-bundle-crate-name\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n09-zero-revision : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n09-zero-revision\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| both identity records claim a revision that is not a commit | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n09-zero-revision\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n11-stale-revision : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n11-stale-revision\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| both identity records claim a real commit that does not contain the packed inputs | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n11-stale-revision\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n10-toolchain-claim : the untouched copy must verify before it is damaged | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n10-toolchain-claim\identity-0.1.0-rc.1` | 0 | 0 | `00-baseline-verify.log` |
| the identity declares a toolchain rust-toolchain.toml at that commit does not | `F:\Workspace\business-platform\target\debug\module-release-tool.exe verify --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n10-toolchain-claim\identity-0.1.0-rc.1` | 1 | 1 | `10-verify.log` |
| n12 : both sides must be identical before the damage | `F:\Workspace\business-platform\target\debug\module-release-tool.exe compare --left F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1 --right F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n12-compare-right-damaged\identity-0.1.0-rc.1` | 0 | 0 | `00-compare-clean.log` |
| a damaged payload on the right side must be refused, not reported as a build difference | `F:\Workspace\business-platform\target\debug\module-release-tool.exe compare --left F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1 --right F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\n12-compare-right-damaged\identity-0.1.0-rc.1` | 1 | 1 | `10-compare-damaged.log` |
| a resolved path dependency pointing at the repository crate with forward slashes | `F:\Workspace\business-platform\target\debug\module-release-tool.exe guard --metadata F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\g01-absolute-into-repo\metadata.json --consumer F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity --repo F:\Workspace\business-platform --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity` | 1 | 1 | `10-guard.log` |
| a resolved path dependency spelled relative to the consumer ('..\..\..\..\..\crates\identity\Cargo.toml') | `F:\Workspace\business-platform\target\debug\module-release-tool.exe guard --metadata F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\g02-relative-into-repo\metadata.json --consumer F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity --repo F:\Workspace\business-platform --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity` | 1 | 1 | `10-guard.log` |
| a transitive local dependency on a repository crate the consumer never names | `F:\Workspace\business-platform\target\debug\module-release-tool.exe guard --metadata F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\negatives\g03-transitive-into-repo\metadata.json --consumer F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\consumers\identity --repo F:\Workspace\business-platform --artifact F:\Workspace\business-platform\target\wave5a-repair1\run-20260929T075734Z\run-a\identity-0.1.0-rc.1\package\contract\packages\identity` | 1 | 1 | `10-guard.log` |

## negative cases

Each case starts from its own copy of a candidate that verified moments before,
injects one fault through parse/serialise (never a blind string replacement),
proves the injection landed by re-reading the file, and then has to be refused
by the asserted check. WRONG REASON means it failed, for a different reason.

| case | expected check | observed check | verdict |
| --- | --- | --- | --- |
| n01-payload-byte | `artifact-integrity-digest` | `artifact-integrity-digest` | OK |
| n02-missing-member | `artifact-integrity-digest` | `artifact-integrity-digest` | OK |
| n03-unknown-algorithm | `artifact-record-format` | `artifact-record-format` | OK |
| n04-unknown-schema | `artifact-record-format` | `artifact-record-format` | OK |
| n05-record-module-id | `record-consistency` | `record-consistency` | OK |
| n06-record-package-digest | `record-consistency` | `record-consistency` | OK |
| n07-bundle-crate-version | `packed-crate-bindings` | `packed-crate-bindings` | OK |
| n08-bundle-crate-name | `packed-crate-bindings` | `packed-crate-bindings` | OK |
| n09-zero-revision | `source-revision-exists` | `source-revision-exists` | OK |
| n11-stale-revision | `source-inputs-match-revision` | `source-inputs-match-revision` | OK |
| n10-toolchain-claim | `toolchain-declaration` | `toolchain-declaration` | OK |
| n12-compare-right-damaged | `compare-payload-integrity` | `compare-payload-integrity` | OK |
| g01-absolute-into-repo | `consumer-graph-isolation` | `consumer-graph-isolation` | OK |
| g02-relative-into-repo | `consumer-graph-isolation` | `consumer-graph-isolation` | OK |
| g03-transitive-into-repo | `consumer-graph-isolation` | `consumer-graph-isolation` | OK |

## result

RESULT: every step matched its expected exit code and every negative case was refused for the asserted reason.

## not executed here, and therefore NOT RUN / BLOCKED

* PostgreSQL migration execution and per-module PostgreSQL installation:
  BLOCKED (G-02 / G-203, no database authorisation in this environment).
* publish / register / install / deploy of a module: does not exist in this
  script, and is not part of Wave 5A.
* vulnerability, licence, secret and image scanning: NOT RUN.
* CI: NOT RUN.
* which toolchain actually produced the packed bytes: NOT PROVEN by the tool.
  `toolchain-declaration` compares release.json with the toolchain *declared* at
  the pinned commit; the `rustc -vV` line in 00-toolchain.txt is the separate,
  external evidence for the run that built the candidate.
