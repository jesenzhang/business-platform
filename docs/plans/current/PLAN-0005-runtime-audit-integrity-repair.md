# PLAN-0005: Runtime Audit, Data Integrity and Controlled Repair Foundation

> Status: Accepted Candidate
> Date: 2026-08-04
> Owner: Platform Foundation / Runtime Governance
> Base SHA: `6fd065c33471665e828a3da3a2cc3fae8d6d2afc`
> Candidate SHA: `21f8c603263bde0dab4c6944c7655c5c71394554`
> Feature CI: `30877000524` (all jobs green)
> Integration Mode: local solo fast-forward
> Pull Request: not used
> Stop Policy: blockers-only

## Goal

Build a bounded Runtime Governance capability that makes audit evidence,
integrity findings, and controlled repair durable without creating a generic
workflow/DAG engine or an arbitrary SQL repair surface. Document Management
continues to own document revisions and storage references; Document
Intelligence owns processing jobs, steps, AI tasks, candidates, and reviews.
Governance may inspect those owners through typed query ports and may request
only allow-listed owner operations through typed repair handlers.

## Architecture preflight

- **Bounded Contexts:** Audit is the Audit supporting context. Integrity scans
  and Repair Runs are Runtime Governance platform capabilities. Processing
  rules and handlers remain implemented by Document Intelligence adapters.
- **Data owners:** `audit_events` and the append-only audit chain belong to
  Audit; scan runs, findings, repair plans/runs/steps/ledger belong to Runtime
  Governance; processing state remains owned by Document Intelligence.
- **Business invariants:** audit records are validated and append-only;
  findings are stable-rule/version/resource identities with optimistic
  lifecycle transitions; repairs are typed, revalidated before execution,
  fenced, idempotent, approval-gated, and verified after execution.
- **Commands/queries:** RecordAudit, StartIntegrityScan, CreateRepairPlan,
  DryRunRepair, ApproveRepair, ExecuteRepair, CancelRepair, ResumeRepair;
  AuditQuery, FindingQuery, ScanRunQuery, RepairRunQuery, ProcessingJobQuery,
  CandidateQuery, and ProcessingStepQuery.
- **Transactions:** owner business state, Audit, and Outbox commit in the
  Document Intelligence owner transaction; the Governance transaction commits
  Finding, Repair Step/Run, and Repair Ledger transitions. Cross-context reads
  are typed and read-only; repair handlers call owner application ports and
  never write foreign tables. The boundary is explicit and recoverable through
  fenced leases and idempotent repair commands.
- **Security:** all records are tenant scoped; management permissions are
  required; medium/high risk repairs require creator/approver separation;
  details are redacted and never contain secrets, object locations, raw text,
  prompts, URLs, or arbitrary SQL.
- **Quality attributes:** keyset audit pagination, bounded scans/batches,
  PostgreSQL multi-worker claim/lease/fence, SQLite single-process
  `BEGIN IMMEDIATE`, crash recovery, stale-worker rejection, and actionable
  audit/integrity metrics.

## Work packages

| ID | Scope | Evidence |
|---|---|---|
| WP-00 | Restrict processing writes to the atomic UoW; expose typed candidate/step queries | Fitness + compile tests |
| WP-01 | Unified Audit domain, validation, actor/result model and redaction | Domain tests |
| WP-02 | PostgreSQL/SQLite audit persistence, queries, hash chain, atomic mapper | Adapter contracts |
| WP-03 | Integrity rule descriptor, registry, scan contracts | Domain/application tests |
| WP-04 | Scan Run and Finding persistence, dedupe and lifecycle | SQLite/PG contracts |
| WP-05 | Typed controlled-repair command, plan, approval and handler contracts | Domain tests |
| WP-06 | Durable Repair Run/Step/Lease/Fence/Retry/Cancel/Resume and ledger | Recovery contracts |
| WP-07 | PROC-INT-001..008 processing integrity rules | Rule fixtures |
| WP-08 | Five deterministic processing repair handlers | Owner-port contracts |
| WP-09 | Governance worker and management API with authorization | API/worker tests |
| WP-10 | Observability, keyset audit query and retention policy | Docs + query tests |
| WP-11 | SQLite/PG/MinIO contracts, atomic rollback and crash recovery | E2E evidence |
| WP-12 | Candidate evidence, CI and architecture gates | Green feature CI |

## Public contracts and events

Management endpoints are versioned below `/api/v1/admin/` and expose only
safe Read DTOs. Commands use `Idempotency-Key`; list APIs use opaque v1
keyset cursors. Audit and repair execution events use versioned envelopes and
contain identifiers, hashes, versions, and bounded counters only.

## Consistency and recovery

PostgreSQL is the production authority and supports multiple Governance
Workers with atomic claim and fencing. SQLite is local, single-process only;
production, parallel workers, and separate AI configuration are rejected.
Every Repair Step records a checkpoint and lease fence. A crashed worker's
lease expires; a new worker resumes the step, while stale writes fail closed.
Each resource is repaired in its own transaction; a run records aggregate
progress and may be cancelled or resumed.

## Fitness functions and documentation

The plan requires the full workspace fmt/check/clippy/test gates,
`scripts/check-architecture.ps1`, migration manifest/checksum validation,
Audit/Integrity/Repair domain tests, real PostgreSQL/MinIO contracts, SQLite
process E2E, management API security tests, and repair crash-recovery tests.
The implementation is synchronized with the runtime audit, integrity/repair,
retention, standards, and ADR-0013..0016 documents.

## Completion definition

The plan becomes `Accepted Candidate` only after all work packages, the five
allow-listed processing repair handlers, both database E2E suites, stale-fence
and crash-recovery evidence, documentation, and a green feature CI run pass.
This plan does not merge to main, archive itself, delete its branch, or start
PLAN-0006.

## Candidate evidence

- Local `cargo fmt --all -- --check`, `cargo check --workspace
  --all-targets --all-features`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, and `cargo test --workspace
  --all-features`: PASS.
- Local `scripts/check-architecture.ps1` and
  `scripts/test-local-governance-repair.ps1`: PASS, including SQLite scan,
  repair, audit, outbox, ledger, and finding lifecycle assertions.
- GitHub Feature CI `30877000524`: PASS for format, check, clippy, workspace
  tests, architecture fitness, and PostgreSQL/MinIO contracts. The real
  governance E2E `postgres_scan_and_requeue_repair_are_durable` passed with
  migrations, unified audit, outbox, repaired Finding, and Repair Ledger
  assertions.
- Windows PostgreSQL/MinIO execution: NOT RUN; the local environment has no
  PostgreSQL service.

## Accepted boundaries and risks

No generic scheduler, DAG, arbitrary SQL, PDF/OCR/Office/real model provider,
or automatic repair outside the explicit allow-list is introduced. Windows
PostgreSQL/MinIO evidence may remain `NOT RUN` when GitHub Linux evidence is
green. A full WORM archive is deferred; the database hash chain is tamper
evidence, not an absolute immutability guarantee. The PROC-INT-008 text
artifact check is metadata/checkpoint based and deliberately reports Unknown
when the durable artifact reference is not trustworthy; a bounded object-store
probe with timeout/rate-limit policy is deferred. Owner business state and
Governance ledger/finding state use separate local transactions across the
bounded-context adapter boundary; leases, idempotency, and reconciliation keep
the boundary recoverable, but a distributed transaction is intentionally not
introduced.
