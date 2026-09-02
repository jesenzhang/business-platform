# PLAN-0012 Completion Audit

Document ID: REPORT-PLAN-0012-COMPLETION-AUDIT  
Status: Final  
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
