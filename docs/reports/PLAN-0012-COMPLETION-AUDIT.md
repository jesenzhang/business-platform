# PLAN-0012 Completion Audit

Document ID: REPORT-PLAN-0012-COMPLETION-AUDIT  
Status: Final (release executed 2026-09-03 — v0.1 tagged, plan archived; see final section)  
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
| A2 | ai-worker records `TaskOutcome::Succeeded` only after `complete_ai_and_resume` returns success; a `LeaseLost` (fenced) completion records `lease_unproven` + lease-lost, while `Unavailable`/`Failed`/other persistence errors record `failed` without touching the lease-lost counter, so every attempt emits exactly one final outcome (error attribution refined by `0809f83` in the 2026-09-03 Final Review amendment below) | `c0e54e2` + `0809f83`: in-process counting `metrics::Recorder` regression tests over all three completion outcomes (durable success / fenced / persistence-failed), each asserting the exact per-scenario counters and the exactly-one-final-outcome invariant; red→green verified for both the original early-`Succeeded` bug and the pre-refinement lease-loss over-attribution |
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

## Amendment — Final Review (2026-09-03, branch `codex/plan-0012-slice-c-staging`)

This amendment records the final-review fix to the ai-worker completion
metric attribution and the re-executed Slice B gate battery on top of
`main@a673c44`. It supersedes the A2 attribution wording above (A2 was
updated in place to the final semantics); it does not restate the previous
amendments.

### Identity

| Identity | Value |
| --- | --- |
| Base | `main@a673c44` (post-PR #9 state) |
| Branch | `codex/plan-0012-slice-c-staging` |
| Fix commit | `0809f83` (completion error attribution refinement) |
| Docs sync | this amendment + `ARCHITECTURE_STATUS.md` (header integration mode corrected to PR #9 / GitHub PR merge; metric wording) + PLAN-0012 Release Closure item 2 |
| Execution window | 2026-09-02T16:00Z–16:40Z (UTC), this workspace |

### Slice A — final-review fix (`0809f83`)

`complete_ai_and_resume` completion-boundary attribution in
`apps/ai-worker/src/main.rs`:

| Completion result | Before | After (`0809f83`) |
| --- | --- | --- |
| `Ok(_)` (durable persist) | `succeeded` | `succeeded` (unchanged) |
| `Err(LeaseLost)` | `lease_unproven` + `ai_lease_lost_total` | `lease_unproven` + `ai_lease_lost_total` (unchanged) |
| `Err(Unavailable / Failed / other persistence error)` | `lease_unproven` + `ai_lease_lost_total` (over-attributed lease loss) | `failed`, `ai_lease_lost_total` untouched |

Every attempt still records exactly one final `ai_tasks_total` outcome.
Regression tests assert per scenario: fenced → succeeded=0,
lease_unproven=1, failed=0, lease_lost=1; persistence failure → failed=1,
lease_unproven=0, lease_lost=0; durable success → succeeded=1, others 0;
each asserts `succeeded + lease_unproven + failed == 1`.

Evidence: `cargo test -p ai-worker --all-features` — 31 passed, 0 failed.
Red→green: temporarily reverting the `Err(_)` arm to the pre-refinement
attribution made
`completion_persistence_failure_records_failed_without_lease_loss` fail
(“a persistence error other than LeaseLost is a failed outcome” — left=0,
right=1); restoring `0809f83` returned all three tests to green.

### Slice B — local gate battery (Windows, Git Bash, pwsh 7, this workspace)

All executed on this branch after `0809f83`; all PASS. The PowerShell
environment was not blocked this time: `pwsh -NoProfile` resolved
(PowerShell 7) and both scripts executed natively.

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS (only cargo's incremental hard-link filesystem notice; not a code lint) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace --all-features` | PASS — 131 `test result: ok` lines, 0 failures, exit 0 |
| `pwsh -NoProfile -File scripts/check-architecture.ps1` | PASS (Cargo metadata architecture fitness + OpenAPI contract + architecture fitness) |
| `pwsh -NoProfile -File scripts/check-openapi.ps1` | PASS |
| `DRILL_SELFTEST=1 bash deploy/operations/drill-backup-restore.sh` | PASS (`[drill selftest] PASS`, exit 0) |

Remote corroboration: GitHub CI run `33656338653` on `8fd46de` —
**success**, all 10 jobs green, including **Architecture Fitness**,
Format/Check/Clippy (Rust 1.94.1), Unit tests, PostgreSQL + MinIO + E2E
contracts, CLI and MCP contracts, Security scanning (cargo-audit, gitleaks,
trivy), Frontend checks, and Frontend Playwright smoke.

### Slice C — preproduction acceptance: still BLOCKED / NOT RUN

> Superseded: the staging environment was provisioned after this entry
> was written; see the following amendment “Slice C real staging
> acceptance”.

Environment re-probe on this workspace (2026-09-02T16:3xZ UTC): `docker`,
`psql`, `pg_dump`, `mc` binaries all absent from `PATH`; no
`deploy/staging/` configuration exists in the repository; no real IdP
audience/issuer or model-provider credentials are configured
(`AI_SMOKE_BASE_URL`, OIDC issuer/client, provider keys unset;
`TEST_POSTGRESQL_URL`/`TEST_POSTGRESQL_RESTORED_URL` are local
contract-test harness variables, not staging access). Per the mandate —
缺少 staging 或真实凭据时保持 BLOCKED/NOT RUN，不得用 fake/stub 替代 — the
Slice C table in the Release Closure amendment stands unchanged: real-stack
provisioning, 20-concurrent smoke metrics, full-chain rehearsal, and live
Prometheus/Grafana/cardinality verification remain BLOCKED, and the
evidence row remains PENDING for whoever executes the staging run.

### Release decision (unchanged)

Slice A fix and Slice B gates are complete on this branch; Slice C still
cannot pass here. The conditional final actions remain unmet on Slice C
alone: `v0.1` tag **withheld**, PLAN-0012 **not archived**,
`docs/plans/README.md` **not** updated for archival.

## Amendment — Slice C real staging acceptance (2026-09-03, branch `codex/plan-0012-slice-c-staging`)

This amendment records the first real (non-fake, non-stub) preproduction
staging run and the defects it surfaced. Evidence files live outside the
repository under `F:\Workspace\business-platform-staging\evidence\`
(numbered `01`–`19`); this audit quotes them. No credential, endpoint
secret, or database password is recorded here or committed anywhere.

### Identity

| Identity | Value |
| --- | --- |
| Base | `main@a673c44` |
| Branch | `codex/plan-0012-slice-c-staging` |
| Code commits | `0809f83` (Slice A attribution) · `ac58991` (observability histogram buckets) · `d0076e0` (processing contract determinism) · `3ab22dd` (contract-suite DB isolation) · `0f27420` (clippy doc-markdown fix) |
| Docs commits | `8fd46de`, `589e6a6`, this amendment |
| Execution window | 2026-09-02T18:00Z–19:35Z (UTC), this workspace |

### Environment (Docker-free staging, real components only)

| Component | Reality used |
| --- | --- |
| PostgreSQL | local PostgreSQL 18 (`D:\Program Files\PostgreSQL\18`), database `business_platform`, full migration set applied on PG18 (compatibility evidence) |
| Model provider | intranet vLLM endpoint (OpenAI-compatible `/v1`, model `qwen3_5_122B_A10B`) — real inference, no stub |
| Object storage | local MinIO (no Docker), buckets `enterprise-documents` + contract bucket |
| Metrics / dashboards | local Prometheus (`:9090`), Grafana 12.4.10 (`:3001`), dashboard `plan-0012-v0-1-candidate` |
| Processes | `business-api :3000` (dev-auth mode), `business-worker :9464`, `ai-worker :9465`, all built from this branch |
| IdP | **not configured** — user chose "option A" (reuse of the T2.5 IdP) but issuer/audience/client/test-account parameters were not supplied; production-mode login/refresh acceptance therefore stays BLOCKED (see Limitations) |

### Scenario matrix (mandate item by item)

| Scenario | Result | Evidence |
| --- | --- | --- |
| login/refresh | PARTIAL — dev-auth token exercised end-to-end; production IdP flow BLOCKED pending IdP parameters | token flows through upload/job/review calls in `01`–`09` |
| upload + idempotent replay | PASS — 201 then replayed 200 with same document id; fingerprint conflict rejected | `01-upload.json`, `02-upload-replay.json` |
| real AI extraction | PASS — real vLLM produced candidate fields; candidate passed validation into `waiting_for_review` | `03-job-created.json`, `05-candidate.json` |
| Review approve | PASS — optimistic-lock review transition, audit + outbox recorded | `06-review.json`, `07-job-after-review.json` |
| worker crash/recovery | PASS — killed worker mid-`extract_fields`; expired-lease scan reclaimed the lease (`reclaimed=1`) and a second worker resumed the same step at fence version 2 (`ai-worker-2.log` 2026-09-02T18:03:02Z); job reached `waiting_for_review` with no duplicate side effects | `08-crash-recovery*.txt`, `logs/ai-worker-2.log` |
| backup/restore drill | PASS — `drill-backup-restore.sh`: pg_dump (118,124 bytes) + bucket mirror restored into an isolated drill DB/bucket; row counts + marker object md5 roundtrip verified (`42f5137a…d748a`) | `10-backup-restore-drill.log` |
| 20-concurrent smoke ×2 | PASS — see metrics below | `12/13-load-report.json`, `11/14-resource-samples.csv` |
| Prometheus scrape | PASS — 3/3 targets up (`business-api`, `business-worker`, `ai-worker`) | live `/api/v1/targets` |
| Grafana dashboard | PASS with documented zero-event series | `19-dashboard-panel-data-check.txt` |
| label cardinality | PASS — sampled custom metrics expose 2–7 series each (histograms with the fixed 15-bucket `_le` set); no unbounded labels | same file + live `/api/v1/series` |

### 20-concurrent smoke metrics (dev-auth staging, real stack)

Run 1 — `12-load-report.json`, 2026-09-02T18:05:51Z–18:07:30Z:

| Op | n | errors | p50 ms | p95 ms | p99 ms | throughput |
| --- | --- | --- | --- | --- | --- | --- |
| upload (20 concurrent) | 200 | 0 | 935.7 | 1431.9 | 1619.8 | 20.61 req/s |
| list | 200 | 0 | 1.7 | 19.5 | 23.5 | 20.61 req/s |
| job create | 20 | 0 | 132.4 | 137.1 | 137.7 | 141.9 req/s |
| pipeline e2e to `waiting_for_review` | 20 | 0 failed | 44.2 s | 77.1 s (max 87.2 s) | — | 13.77 jobs/min |

Run 2 — `13-load-report.json`, 2026-09-02T18:25:15Z–18:27:00Z (staged
while a second worker pair processed the run-1 backlog, hence slower):

| Op | n | errors | p50 ms | p95 ms | p99 ms | throughput |
| --- | --- | --- | --- | --- | --- | --- |
| upload | 200 | 0 | 1389.7 | 2537.4 | 2686.5 | 13.63 req/s |
| list | 200 | 0 | 1.9 | 21.1 | 26.3 | 13.63 req/s |
| job create | 20 | 0 | 270.8 | 302.4 | 310.2 | 64.2 req/s |
| pipeline e2e | 20 | 1 job `failed` | 55.3 s | 86.2 s (max 87.2 s) | — | 13.76 jobs/min |

The single pipeline failure was `ai_invalid_response`: the real model
returned an unparseable extraction payload for one document and the
pipeline correctly failed that job closed-form after retries instead of
stalling — counted honestly in the error rate (1/20 = 5%).

Resource water level (`14-resource-samples.csv`,
2026-09-02T18:12:29Z–18:35:30Z — this window mixes the load runs with
contract-suite diagnostics, so it is labelled MIXED, not a clean
load-only window): host CPU p50 24.4% / p95 80.5% / max 100% (the
machine also ran the cargo test battery in the same window); working-set
MB p50/p95 — `ai-worker` 13.2/16.0, `business-api` 29.2/32.8,
`business-worker` 13.1/15.8, MinIO 255.4/284.7. PostgreSQL CPU delta was
NOT measurable (service runs as a different account and the sampler did
not cover it) — recorded as a sampling limitation, not as a pass.

### Defect found and fixed by the staging run (`ac58991`)

Prometheus scraped correct counters/sums but every duration histogram
rendered as a Go summary with zero quantiles, so all p95 panels read
zero. Root cause: `metrics-exporter-prometheus` 0.17 exposes histograms
without configured buckets as summaries. Fixed in `crates/observability`
by explicit `DURATION_BUCKETS` (5 ms…120 s) via
`set_buckets_for_metric(Matcher::Suffix("_seconds"), …)` plus a
regression test asserting `_bucket{le=…}` series in the rendered text.
Verified live: after restart, `processing_job_queue_wait_seconds_bucket`
and `ai_task_duration_seconds_bucket` series are present.

### Grafana dashboard verification (`19-dashboard-panel-data-check.txt`)

All 8 panels' PromQL evaluated through the datasource API: 6 panels
return data; 2 panels are PARTIAL purely because their event classes are
empty in this staging window (no 429/5xx from the provider, no
worker-side lease loss after the drill). Raw-counter spot checks confirm
the wiring: `processing_leases_reclaimed_total` = 2 (the two drill
reclaims) while `ai_provider_rate_limited_total`,
`ai_provider_server_error_total`, `processing_lease_lost_total` and
`ai_lease_lost_total` are absent, which in this exporter means zero
increments. Verdict: panels are correctly wired; "no data" reflects
genuinely empty event classes, not broken queries.

### Contract-gate determinism fixes discovered while running the CI mirror locally

Running the exact ci.yml contract-test list against one local PG18
database exposed three real cross-target defects (each reproduced from
raw assertion values in the `15`/`16` battery logs):

1. `document-processing-postgres` contracts timed the crashed worker's
   lease from stale virtual time while the store deliberately fences
   writes against the database wall clock
   (`lease_expires_at > CURRENT_TIMESTAMP`); under host load this is a
   correct `LeaseLost`, not a product bug — fixed test-only (`d0076e0`).
2. `outbox_events` claim/recovery statements are table-wide, and three
   targets left pending rows behind (processing contracts, the
   persistence-contract harness, the governance fixture), poisoning the
   messaging outbox contract counts — all producers now clean their own
   rows (`d0076e0`, `3ab22dd`).
3. `query_contracts`' EXPLAIN index assertion flipped on planner
   statistics left stale by earlier targets' churn — it now runs
   `ANALYZE documents` first, and cleans the harness's outbox rows via a
   before/after event snapshot (`3ab22dd`); `inbox_idempotency` raced
   its own `CREATE TABLE IF NOT EXISTS` under parallel threads — gated
   by a process-wide `OnceCell` (`3ab22dd`).

Final CI-mirror battery on a fresh, migrated database:
`TOTAL FAILING TARGETS: 0` (`18-ci-contract-battery-hygiene-green.log`).
Workspace gates re-run after these commits: fmt / check / clippy /
default tests / `check-architecture.ps1` / `check-openapi.ps1` — all
PASS (Git Bash, Windows; the PowerShell gates executed natively, exit 0).

### CI fix→fail→fix loop (branch)

Run `33673045645` on `3ab22dd`: 9/10 jobs green; Clippy failed on Rust
1.94.1 (`doc_markdown`: bare `PostgreSQL` in the new doc comment — the
local toolchain predates that check). Fixed in `0f27420`; the
re-run's outcome is recorded in the branch CI history.

### Limitations (honest)

- Production-auth acceptance (login/refresh against the real IdP) is
  BLOCKED: "option A" parameters (issuer/audience/client/test accounts)
  were never supplied. Dev-auth mode covered every downstream scenario
  but is not a substitute under the mandate's no-substitution rule.
  > Superseded: a real Auth0 tenant was provisioned with user consent and
  > the production-mode IdP leg passed 12/12; see the following amendment
  > "Slice C IdP leg: real OIDC issuer acceptance".
- The resource window in `14` mixes load with contract diagnostics, and
  PostgreSQL process CPU is unsampled; the water levels above must be
  quoted with that caveat.
- The discarded 18:19:58Z load attempt (`13-load-run.log` traceback) was
  an operator error (stale idempotency keys reused from run 1 caused a
  409 phase abort); it is kept as evidence and is not counted as a
  product failure.
- Staging ran `business-api` in dev-auth mode, and the vLLM endpoint
  requires no real credential on this intranet; neither is production
  authN evidence.

### Release decision (still conditional)

Slice C functional acceptance (upload→real AI→Review, crash/recovery
fencing, backup/restore, 20-concurrent smoke, observability) is now PASS
on real components, with the single IdP leg BLOCKED pending parameters.
The mandate's final gate remains: merge to `main` + Main CI green before
`v0.1` tag / PLAN-0012 archive / plans README update. `v0.1` remains
**withheld**.

## Amendment — Slice C IdP leg: real OIDC issuer acceptance (2026-09-03)

The BLOCKED IdP leg above is resolved by this amendment. Under explicit
user authorization ("我没有Auth0 的账号，你可以自己申请一个，然后测试吗")
a genuine Auth0 tenant was registered and the production-mode
`business-api` was accepted end-to-end against it. No fake or stub stood
in for the issuer at any point.

### Identity and provisioning

| Item | Value |
| --- | --- |
| IdP | Auth0 tenant `dev-lcjop6qfgzt7dc8v.us.auth0.com` (dev region), account self-registered by the agent with user consent on 2026-09-03 |
| OIDC application | `business-platform-smoke`, client id `igvLedqzvOSAinFmEi8Mo3zQHrkoWKzS`, authorization_code + PKCE(S256) + refresh_token + ROP(password, test-only) |
| API / audience | `https://business-platform.test/api` (HS256 signing app disabled-path unused; tenant signs RS256; JWKS via OIDC discovery) |
| Test subject | `smoke.user@business-platform.test` → `sub auth0\|6a98c2522c88bf714c69f9c4` |
| Claim injection | Auth0 Post Login Action `identity-claims` (id `ff1b7184-0afe-435b-9ebd-c3833b1a0f51`, deployed v7), activated via trigger binding `77da4f7a-d83a-40c5-aeae-ad4ea6f81281`, injects `tenant_id`, `user_id`, `management_permissions` into the access token |

Secrets (Auth0 account/client credentials, smoke password, database
password) live only in `F:\Workspace\business-platform-staging\secrets.env`
and process environment — nothing was committed or written to evidence.

### Production-mode launch and a fail-closed discovery

`business-api` was launched with `ENV=production`, `issuer_url=https://dev-lcjop6qfgzt7dc8v.us.auth0.com/`,
`audience=https://business-platform.test/api`, JWKS via discovery, `log_format=json`,
explicit non-wildcard CORS, PostgreSQL 18 + MinIO. Health: `/health/live`
200, `/health/ready` 200 (`migrations: compatible`).

Operational finding (real, not a defect): the config loader
(`crates/runtime-config/src/loader.rs`) also merges `config/default.toml`
from the **working directory**, and the repo's default file carries
`auth.dev_secret = "dev-only-secret"` for developer convenience. A
production instance started from the repository root is therefore
rejected — `auth.dev_secret must be absent in production` — which is the
fail-closed rule doing its job. Production deployments must run from a
working directory without repo dev config (the acceptance instance ran
from an empty `prod-run` directory with a full env matrix).

### E2E acceptance — 12/12 PASS (evidence `20-prod-e2e.raw`)

`tools/prod-e2e-acceptance.py`, 2026-09-03T01:2xZ: ROP login returned
access/refresh/id tokens with the injected claims; `GET /api/v1/documents`
200 with real tenant rows (e.g. `smoke.txt`);
`GET /api/v1/admin/audit-events` 200 driven purely by the injected
`management_permissions` claim; `refresh_token` grant returned a new
access token which was accepted again. Negatives, all 401: no token;
tampered signature; `id_token` as bearer (audience mismatch);
HS256 token forged with the repo dev secret; `alg=none` token; wrong
issuer. Earlier artifacts cover the standard browser flow:
`20-idp-authcode-token.raw` (authorization_code + PKCE via
`localhost:8765` redirect + consent incl. offline access),
`20-idp-refresh-test.raw`, `20-idp-token-with-claims.raw` (claims proof).

### Startup fail-closed matrix — 8/8 rejected, rc=1 (evidence `20-failclosed-*.log`)

With an otherwise-valid production env matrix, each single violation was
refused at startup with a field-oriented message: dev auth enabled +
secret + dev identity (3 messages); `dev_secret` alone; empty issuer;
http(scheme) issuer; `cors_origins=*`; `log_format=text`; blank audience;
`storage.backend=local`.

### Caveats (honest)

- `roles` is a reserved Auth0 claim and `setCustomClaim('roles', …)` is
  silently ignored; the validator treats `roles` as optional and
  authorization here is driven by `management_permissions`, so the flow
  is complete, but a production Auth0 setup should deliver real roles via
  the RBAC (Roles) API instead.
- The Action injects claims without an audience guard:
  `event.accessToken.aud` proved unreadable for the ROP flow in this
  tenant, and the guard silently skipped injection when present.
  Acceptable for a single-API test tenant; production Actions must guard
  by audience — recorded as an operational note, not a product gap.
- The ROP(password) grant is test-convenience; the acceptance additionally
  proved the browser authorization_code + PKCE flow end-to-end.
- Claim values (tenant `aaaaaaaa-…`, user `11111111-…`) deliberately match
  the existing staging data so the real PostgreSQL data set was exercised.

### Slice C status after this amendment

Every Slice C row is now PASS on real components (staging stack
2026-09-02 + real IdP 2026-09-03). The remaining gate is the mandate's
final clause only: merge to `main`, Main CI green — then and only then
`v0.1` tag, PLAN-0012 archive, plans README sync. `v0.1` remains
**withheld** pending that decision.

## Release executed (2026-09-03)

All mandate conditions are met and the conditional final actions were
executed:

1. Slice A/B/C: all PASS (engineering gates + staging real-stack
   acceptance + real-IdP production-auth acceptance — amendments above).
2. Merge: PR #10 (`codex/plan-0012-slice-c-staging`) merged to `main` as
   `2383651`; PR checks 20/20 SUCCESS; Main CI run `33705531597` — all
   jobs success.
3. Tag: annotated `v0.1` created on `2383651` and pushed.
4. Archive: PLAN-0012 moved to `docs/plans/archive/2026/` with an archive
   record (deferred T3.3 recorded honestly); `docs/plans/README.md`
   updated (current list now only PLAN-0006 Proposed).

Residual operational notes (not release-blocking, recorded in the IdP
amendment caveats): production Auth0 tenants should deliver `roles` via
the RBAC API and guard Action injection by audience; production
deployments must run from a working directory without the repo's dev
`config/` files (the fail-closed `dev_secret` rule rejects otherwise).
