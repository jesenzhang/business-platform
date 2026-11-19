# PLAN-0014 Wave 4 — gate and lane evidence index

> Branch `codex/plan-0014-wave4-reference-conformance`. Base commit
> `a4c2f74b5f0fbff5a9b0abde6319be33831e9890`. Code commit under review
> `6fb9487d045108c70efd41c88ccc95f0861ba603`. Reviewer verdict column is
> deliberately empty: this wave records self-verification only
> (`READY FOR INDEPENDENT REVIEW / NOT INTEGRATED`).

## 1. Workspace gates (code commit `6fb9487`)

| # | Gate | Command | Exit | Result |
|---|---|---|---|---|
| 1 | Format | `cargo fmt --all -- --check` | 0 | pass (`01-fmt-check.log`) |
| 2 | Check | `cargo check --workspace --all-targets --all-features` | 0 | pass (`01b-check-workspace.log`) |
| 3 | Lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | pass (`02-clippy.log`) |
| 4 | Tests | `cargo test --workspace --all-features` | 0 | 759 passed / 0 failed / 59 ignored, 175 suites (`03-test-workspace.log`) |
| 5 | Architecture | `pwsh -NoProfile -File scripts/check-architecture.ps1` | 0 | Cargo metadata fitness PASS, OpenAPI contract PASS, architecture fitness PASS (`04-check-architecture.log`) |
| 6 | Diff hygiene | `git diff --check` | 0 | no whitespace errors; only CRLF-normalisation warnings (`05-diff-check.log`) |

`superseded-intermediate-run2/` keeps the last superseded gate pair: clippy
flagged `clippy::too_many_lines` in `governance-worker::main` and
`clippy::panic` in the new worker test, and `cargo test` failed the new
`business-worker` missing-object case because its own assertion was wrong (it
forbade the substring `source`, which the legitimate message
"source object was not found" contains). Both were fixed before the recorded
green run. Two earlier intermediate runs were overwritten by the next gate
invocation rather than archived; their findings are described in
`../2026-09-28-wave4-storage-lane/README.md` (the capability-probe bugs) and in
the commit messages. No test was deleted, no assertion weakened and no
exemption widened to reach green.

## 2. Real infrastructure lanes

| Lane | Environment | Evidence | Result |
|---|---|---|---|
| PostgreSQL adapter + tenant fence | local PostgreSQL 18.4, dedicated database `plan0014_wave4` created empty for this round and dropped afterwards; `DATABASE_URL` pointed at that database only | `06-postgres-lane.log` | `document_postgres` unit 1/1, `query_contracts` contract 1/1, `tenant_fence_pg` 1/1 — 3 passed / 0 failed |
| Real S3-compatible provider | RustFS `1.0.1-preview.11` (tag SHA `0913686b12845eefc69a2f4ed4ee6a8161e25742`, zip SHA-256 `cae2628159a8759118d8a68cd043c389a05ee9a6a13ceb9e188e9414578453d5`), dedicated bucket `plan0014-wave4-bucket`, prefix `plan0014-wave4/run-20260928-110319/` | `../2026-09-28-wave4-storage-lane/` | 7 passed / 0 failed, capability matrix recorded, 0 objects left, bucket deleted, server stopped |
| OTLP exporter / Collector | no Collector available in this environment | — | **NOT RUN / BLOCKED** (recorded as gap `OBS-W4-06`, not as a pass) |
| Hosted provider (real AWS S3, MinIO) | MinIO open-source binaries are discontinued (`dl.min.io` returns HTTP 410; no Windows asset on the last release); no AWS credentials in scope | — | **NOT RUN**; RustFS stands in as the S3-compatible implementation actually chosen |

> **Round 2 correction (2026-09-28, same wave).** Two claims above did not survive
> re-checking: the S3 row's "0 objects left" verdict came from a single
> `ListObjectsV2` page and never inspected incomplete multipart uploads, and the
> observability provider-suppression fix was bypassable through `RUST_LOG`. Both
> are re-closed in Round 2 — see
> [`../2026-09-28-wave4-round2-gates/`](../2026-09-28-wave4-round2-gates/) and
> [`../2026-09-28-wave4-round2-storage-lane/`](../2026-09-28-wave4-round2-storage-lane/).
> The rows above are left as Round 1 recorded them.

## 3. Key file digests at the code commit (SHA-256)

```
8fd0a282516222e7ece0b840caa817b37cfd2f6d79e41a1355f08d9244c5b0b0  crates/document-sqlite/src/unit_of_work.rs
2c7168331fccfefbda4bd9fbc58ebffeb19b882e75491f5dce8dd9de1fa41931  crates/document-postgres/src/unit_of_work.rs
75997e121d21841b7ef30ff3fdb4ed1dd8ff9dcdcec29ff00f7da8ef62dc11e2  crates/object-storage/src/client.rs
1e84c3e8ba37bb1aa041cb79ffbbbe5b42e90758b2d9be954dfcfa34ec5c36d9  crates/observability/src/lib.rs
8f2dcfb0e862ca45b8b735288e6ead5482406d57e0454fea0dce8d757d350e50  apps/business-api/src/metrics.rs
087306b4fafe26f59af3a7e47d7d42b705c8632a12ee1a26ee4c6632b43cbc77  apps/business-worker/src/main.rs
0987beb88ff8d902503cd5956e0c5f9685fff201f371b5fb68bea0e5843c96ff  apps/governance-worker/src/main.rs
e5ba6df69fa8a3e0909a845953bde427eec9ff3a3a409a4cb75da47a94087a74  crates/document-persistence-contracts/src/lib.rs
34f1921a9d04bad9804dfa6d4b2c63165204d5e0f33f6d4aca3dd065c161d353  crates/notification/src/lib.rs
ba9a21133669598ab6be198b6ebc093e29bf79bf8bfdfc8e2743d51ec69e96d3  crates/document-sqlite/tests/tenant_fence.rs
df28e1117fa74f29b844a73d471f6cefdc51be4892b4a8d2e362741784fcbb19  crates/document-postgres/tests/tenant_fence_pg.rs
9230919b476494eec3c05692bce64555f700fd3780257aa5c6f0071cf1b6ce67  crates/document-sqlite/tests/content_reference.rs
b49cbb3bcccd3edc12ddc12a910d2cd94e2341d7abc13ed33fe9605d37b2bdc7  crates/object-storage/tests/log_hygiene.rs
dd38fb4928728fead893dbfc1eecbbf76bece470b25d9e60e41e611ba6a9c5ae  crates/object-storage/tests/s3_contract_lane.rs
```

## 4. Preserved working-tree modification

`crates/document-sqlite/tests/operational.rs` was already modified when this
wave started (end-of-line only; blob identical to the base commit). It was not
staged, not committed and not reformatted: `git hash-object` still returns
`8dc7c7ac49e08ef8eec8ea79a53a352aec48b5c9`, equal to
`git rev-parse a4c2f74b:crates/document-sqlite/tests/operational.rs`.

## 5. Closeout binding

The docs closeout commit is the commit that introduced this file; locate it with
`git log --oneline -- docs/reviews/2026-09-28-wave4-gates/README.md`. It changes
documentation only, so the gate results above bind byte-for-byte to the code
commit `6fb9487d045108c70efd41c88ccc95f0861ba603` (the tree of every `crates/`
and `apps/` path is identical between the two commits; verified with
`git diff --name-only 6fb9487 HEAD -- crates apps` returning no files).
