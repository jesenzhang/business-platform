# PLAN-0012 Completion Audit

Document ID: REPORT-PLAN-0012-COMPLETION-AUDIT  
Status: Final (amended 2026-09-02 — Release Closure, see final section)  
Date: 2026-09-02  
Scope: PLAN-0012 release-hardening completion verification on branch
`codex/plan-0012-release-hardening` (20 commits `3d90765..f2c1e0b` over `main`).
This report records what was executed, what passed, and — explicitly — what could
NOT be run in the available environment and under what condition it must be re-run.
No item below is marked complete on partial evidence.

## Result

All v0.1 release-blocking engineering items are resolved: every automated gate
passes locally and in CI, the long-standing agent-adapter flake was eliminated at
its root cause (proxy inheritance, not a test race), and the security scanning job
(cargo-audit + gitleaks + trivy) and the backup/restore drill have now genuinely
executed end-to-end in CI for the first time.

**The `v0.1` tag is withheld and PLAN-0012 is NOT archived**, because two acceptance
items of the release mandate require a staging environment with real credentials
that does not exist in this workspace (see “NOT RUN”). The mandate is explicit:
the tag and archival happen only after every condition passes, and an honest audit
cannot convert `NOT RUN` into `PASS`.

| Identity | SHA / run |
| --- | --- |
| Branch | `codex/plan-0012-release-hardening` |
| First / final delivery SHA | `3d90765` … `f2c1e0b` |
| Branch CI (authoritative for infra-backed gates) | run `33617370509` — **success**, all 10 jobs |
| Prior branch runs (failure→fix loop) | `33611418021`, `33612868514`, `33614544430`, `33616237190` |

## Hardening delivery per mandate step

| Step | Commits | Evidence |
| --- | --- | --- |
| 1 Backup/restore drill: guarded `BACKUP_DIR`, unique drill subdir, seed marker before backup with checksum+size, restore to drill-owned DB/bucket, verify from the restore side, trap cleanup, CI-runnable | `3d90765`, `ea5629a`, `1d516ba`, `f2c1e0b` | Drill selftest + full drill executed in CI run `33617370509` (seed→`pg_dump`→restore drill DB→bucket mirror→restore lowercase bucket→marker verify→cleanup) |
| 2 OIDC: non-empty production audience, HTTPS-only issuer/JWKS in production, no redirects, JWKS fail-closed, cross-tenant test with real OIDC principals | `3b8e8cd` | Workspace tests (unit job) + CI |
| 3 AI retry: preserve `ProviderError.retry_after`, capped `Retry-After` on 429, platform backoff for timeout/5xx, authentication/invalid-request not transient, no provider types in core | `3e192ff` | End-to-end retry tests in unit job |
| 4 Observability: production JSON logs + config regression, correlation propagation, bounded HTTP method label set, worker metrics, minimal Prometheus/Grafana config, no tenant_id/body/path/model-response labels | `931a362`, `18e2542`, `e23af8f` | Unit + architecture jobs |
| 5 Flake elimination: in-process deterministic stubs (connection-refused, upstream 5xx, protocol error, normal), full workspace suite twice consecutively | `7859bc1`, `15c8823`, `f619ed6`, `e3dabd0` | Root cause was reqwest inheriting the Windows system/env proxy for the internal base URL (bearer token exposure + synthesized 5xx). Fixed with `.no_proxy()` at the client boundary plus a red→green regression test; owned-socket stubs removed port races. Full pair runs green twice more after the dependency refresh (see Local gates) |
| 6 Documentation sync, nothing marked complete without evidence | `5ab6b25`, this report | CI architecture/docs jobs |
| 7 Full gate battery | `14cad43`, `b1bf38e`, `45e3e56`, `02b2657`, `c739af3`, `9636217` + CI runs above | See gate tables |

## CI failure→fix loop (branch)

| Run | HEAD | Failures | Fix |
| --- | --- | --- | --- |
| `33611418021` | `14cad43` | trivy-action tag unresolvable; vendored `TrustedPrivateHttp` fixtures panicked on loopback-only runners; production wildcard-CORS rule rejected the dev-default `config/default.toml` and aborted the E2E API boot | `02b2657`, `45e3e56`, `b1bf38e` |
| `33612868514` | `02b2657` | Drill selftest rejected the CI checkout under `/home` as a dangerous root; 6 RUSTSEC advisories | `ea5629a`, `c739af3` |
| `33614544430` | `ea5629a` | Full drill reached restore and hit `mc mb --force` (flag removed in current mc); cargo-audit (fix not yet pushed) | `1d516ba` |
| `33616237190` | `1d516ba` | Restore bucket name contained uppercase stamp characters (S3 rejects); trivy release asset for the action-pinned `v0.65.0` is gone and the action installer failed silently | `f2c1e0b`, `9636217` |
| `33617370509` | `f2c1e0b` | — | **All jobs success** |

## Gate evidence

Final branch CI (`33617370509`, all success): Format, Check, Clippy `-D warnings`,
Unit tests (`--all-features`), Architecture Fitness, CLI and MCP contracts, Frontend
checks (lint/typecheck/test/build), Frontend Playwright smoke, PostgreSQL + MinIO +
E2E contracts (ignored PG/MinIO contract tests, multiprocess E2E, drill selftest,
real drill), Security scanning (cargo-audit `--deny warnings`, gitleaks, trivy fs
CRITICAL/HIGH `--ignore-unfixed --exit-code 1`).

Local (Windows, Git Bash):

- `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --all-features`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass
  (also after the `Cargo.lock` refresh).
- `cargo test --workspace --all-features` twice consecutively: pass both before the
  lock refresh (pair `plan0012-ws-tests3.log`) and after (`RUN1_EXIT=0`,
  `RUN2_EXIT=0`).
- `scripts/check-architecture.ps1`, `scripts/check-openapi.ps1`: pass.
- Console `npm ci / lint / typecheck / test / build / test:e2e (Playwright)`: pass.

## Security triage record (cargo-audit)

Original CI finding: 6 advisories.

| Crate | Advisory | Disposition |
| --- | --- | --- |
| h2 0.3.27 / 0.4.15 | RUSTSEC-2026-0258 | Resolved: `cargo update` (h2 → 0.4.19) and removal of the whole 0.3 line by dropping aws-sdk-s3's default `"rustls"` feature (legacy hyper-0.14 TLS connector chain) |
| rustls-webpki 0.101.7 (×3) | RUSTSEC-2026-0098/0099/0104 | Resolved: same feature trim removed rustls 0.21; modern stack is rustls 0.23.43 / rustls-webpki 0.103.15 |
| lru 0.16.4 | RUSTSEC-2026-0253 | Resolved: aws-sdk-s3 1.144 uses lru 0.18.3 |
| chacha20 0.10.1 (yanked) | — | Resolved: lock refresh → 0.10.2; also unreachable (optional quinn-proto/rand 0.10 path) |
| rsa 0.9.10 | RUSTSEC-2023-0071 (Marvin; no fixed upgrade) | Ignored with evidence in `.cargo/audit.toml`: reachable only through sqlx's optional `mysql` driver, which this workspace never enables (`cargo tree -i rsa@0.9.10 --target all` is empty for all targets); never compiled or linked into any artifact. Re-review conditions are recorded in that file |

gitleaks: pass. trivy fs (CRITICAL/HIGH, ignore-unfixed): pass (first real execution;
pinned v0.74.0 downloaded directly because the action-pinned asset had disappeared).

## NOT RUN — with reason and re-run condition

| Item | Reason | Re-run condition |
| --- | --- | --- |
| T5.1 Performance smoke baseline (upload→extract→review latency under 20 concurrent jobs against production-shaped infra) | No staging cluster, real IdP, or production-shaped PostgreSQL/object storage in this workspace; CI E2E measures correctness, not capacity | Staging environment; run the documented smoke scenario and record p50/p95 and error rate before tagging |
| T5.2 Real-chain acceptance: real IdP → Console login/refresh → upload → real AI extraction → Review → crash recovery → backup restore | No real OIDC issuer credentials or real model-provider credentials exist locally; all steps are covered by fakes/stubs and CI infra tests, but that is not the acceptance the mandate names | Staging credentials (IdP client + model provider key); execute the chain end-to-end and attach evidence |
| Prometheus/Grafana stack live import/visualization | Only the minimal config files were shipped and reviewed; no live TSDB/Grafana available locally | Staging observability stack; load the shipped scrape config/dashboard and verify cardinality bounds |
| Drill run on production data | CI drill proves the procedure on CI-shaped infra with synthetic live data; production execution requires production access | Production maintenance window; run `deploy/operations/drill-backup-restore.sh` against the real backup target |

`cargo-audit`/`gitleaks`/`trivy` and the PG/MinIO contract + multiprocess E2E +
drill are NOT in this list: they were previously NOT RUN and are now genuinely RUN
and passing in CI run `33617370509`.

## Release decision

- Merge: proceed — the branch is the auditable pre-production candidate; every
  engineering gate is green and every unresolved item is environment-blocked and
  recorded above rather than silently dropped.
- `v0.1` tag: **withheld**. Per the mandate ("仅在以上条件全部满足后创建 v0.1 tag"),
  create it only after T5.1/T5.2 (and the observability-stack and production-drill
  re-runs) pass on staging and this report is amended with their evidence.
- PLAN-0012 archival: withheld with the tag; the plan stays under
  `docs/plans/current/` until the tag conditions are met.

## Amendment — Release Closure (2026-09-02, branch `codex/plan-0012-release-closure`)

This amendment records the release-closure pass on top of `main@899dd3c`
(the merged release-hardening candidate). It does not restate the audit
above; it records the review fixes, the local gate battery, and the
Slice C (preproduction acceptance) status.

### Identity

| Identity | Value |
| --- | --- |
| Base | `main@899dd3c` |
| Branch | `codex/plan-0012-release-closure` |
| Fix commits | `0f1405d` (CI pinning/checksums), `c0e54e2` (Succeeded-after-persist), `c8a54bd` (proxy env isolation), `b8e952a` (blank jwks_url rejection), docs sync `ef72c0b` (this amendment + RUNBOOK/ARCHITECTURE_STATUS/plan) |
| Branch CI (push) | run `33635752037` on `ef72c0b` — **success**, all 10 jobs (includes Security scanning with the pinned+checksum-verified trivy/mc and the real PostgreSQL/MinIO drill) |
| Branch CI (pull request) | run `33635788279` — first attempt failed in the multiprocess E2E “ai-worker crash recovery” phase (the phase poll missed the ~1s `waiting_for_ai`+`running` window on a loaded runner; the identical SHA had just passed on the push run). Re-run of the failed job on the same SHA: **success**, all jobs. No code path of this branch touches that timing; recorded as a runner-timing flake, not a regression. |

### Slice A — review fixes

| # | Fix | Evidence |
| --- | --- | --- |
| A1 | CI pins trivy `0.74.0` and MinIO `mc RELEASE.2025-08-13T08-35-41Z` to immutable GitHub release assets; SHA-256 checksums explicitly maintained in the workflow with a documented bump policy; verification runs after download and before execution, and any mismatch aborts the step (`sha256sum --check` + explicit `exit 1`) | `0f1405d` (`.github/workflows/ci.yml`); checksums cross-checked against upstream `checksums.txt`/`.sha256sum` release assets |
| A2 | ai-worker records `TaskOutcome::Succeeded` only after `complete_ai_and_resume` returns success; a fenced or persistence-failed completion records `lease_unproven` + lease-lost instead, so every attempt emits exactly one final outcome | `c0e54e2`: in-process counting `metrics::Recorder` regression tests over all three completion outcomes (durable success / fenced / persistence-failed); red→green verified by temporarily re-introducing the early-`Succeeded` bug (the fenced and persistence-failure tests failed, then passed again after restoring the fix) |
| A3 | The proxy test saves and restores the original `HTTP_PROXY`/`http_proxy`/`ALL_PROXY`/`all_proxy` values (RAII guard) and serializes environment mutation behind a process-wide mutex, eliminating global env pollution and races with other env-var tests | `c8a54bd`: restore-exactness test (`proxy_environment_is_restored_after_the_client_construction_window`) + original no-proxy behaviour test under the guard |
| A4 | Production config rejects blank/whitespace-only `auth.jwks_url` (a blank override would suppress OIDC discovery and fail every JWKS fetch — fail-closed at startup instead) | `b8e952a` (`apps/business-api/src/config.rs`): `production_rejects_blank_jwks_url` covers `Some("")` and `Some("   ")`; existing https/no-override tests retained |
| A5 | RUNBOOK, `ARCHITECTURE_STATUS.md`, PLAN-0012 and this audit now reflect actual status (production audience mandatory, blank `jwks_url` rejected, CI-executed drill and pinned/verified CI tools, Slice C blocked state) | docs sync commit on this branch |

### Slice B — local gate battery (Windows, Git Bash, this workspace)

All executed on this branch after the fixes; all PASS.

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace --all-features` | PASS — all 131 test-target result lines `ok`, 0 failures (chain exit 0) |
| `scripts/check-architecture.ps1` | PASS (cargo-metadata architecture fitness + OpenAPI contract + architecture fitness) |
| `scripts/check-openapi.ps1` | PASS |
| `DRILL_SELFTEST=1 bash deploy/operations/drill-backup-restore.sh` | PASS (`[drill selftest] PASS`, exit 0) |

### Slice C — preproduction acceptance: BLOCKED / NOT RUN

The mandate is explicit: 如缺少 staging 或真实凭据，必须将 Slice C 标记为
BLOCKED/NOT RUN，不得用 fake、stub 或本地测试替代验收证据。

Environment probe on this workspace found no staging deployment and no real
credentials: no docker/psql/pg_dump/mc binaries available locally, no
`deploy/staging/` configuration, and no real IdP or model-provider endpoints
or keys (`AI_SMOKE_BASE_URL` and friends unset). Accordingly:

| C item | Status | Reason / re-run condition |
| --- | --- | --- |
| Real IdP + real model-provider + PostgreSQL + object storage + Prometheus/Grafana | BLOCKED | No staging stack or credentials here; requires provisioning access to the preproduction environment |
| 20-concurrent performance smoke (p50/p95/throughput/error rate/resources) | BLOCKED | Same; run on staging and record numbers here before tagging |
| Full chain login/refresh → upload → real AI extraction → Review → worker crash/recovery → backup/restore | BLOCKED | Same; the chain is covered locally only by fakes/stubs and CI infra tests, which the mandate does not accept as this evidence |
| Prometheus scrape / Grafana dashboard / label-cardinality live verification | BLOCKED | Only shipped config reviewed; needs the staging observability stack |
| Record commands/env/SHA/time/metrics into this audit | PENDING | To be appended by whoever executes the staging run; no evidence may be simulated |

### Release decision (unchanged)

The branch was merged to `main` as `eb62451` (PR #9, merge commit; branch CI
`33635752037`/`33637294711` and the PR run re-run all green, Main CI
`33637882962` — success), so two of the three merge-side preconditions are
met. Because Slice C cannot pass in this environment, the conditional final
actions (“仅当 Slice A/B/C 全部 PASS、变更合入 main 且 Main CI 全绿后”)
remain unmet on Slice C alone: the `v0.1` tag is **still withheld**, PLAN-0012
is **not archived**, and `docs/plans/README.md` is **not** updated for
archival. Merging did not discharge the Slice C evidence requirement; the
tag/archive trigger is a staging run of the Slice C items with this audit
amended by their real evidence.
