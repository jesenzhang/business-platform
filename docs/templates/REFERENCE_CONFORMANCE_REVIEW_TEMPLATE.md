# Reference Conformance Review Template

> Review ID: REVIEW-SaaS-<module>-<date>  
> Status: Draft / PASS / PASS WITH C2-C3 / FAIL  
> Module:  
> Module revision:  
> Reviewer:  
> Review date:  
> Related plan: PLAN-0014

## 1. Scope

- module authority:
- bounded context:
- current maturity R0-R4:
- public contracts:
- adapters:
- persistence:
- excluded scope:

## 2. Fixed references

| Reference | Reviewed revision/release | License | Source/tests inspected | Why relevant |
|---|---|---|---|---|
| | | | | |

Rules:

- at least two mature references for R2 admission;
- at least one must be a real production OSS/platform, not a starter;
- record core source/tests, not README only.

## 3. Current implementation facts

### Authority and data ownership

### Domain/Application model

### Tenant/Principal/AuthZ

### Public contracts

### Persistence/migrations

### Failure/retry/idempotency

### Recovery/concurrency

### Security

### Observability/audit

### Version/release model

### Current test evidence

## 4. Capability matrix

| Dimension | business-platform | Reference evidence | Gap | Decision |
|---|---|---|---|---|
| Authority | | | | |
| Domain model | | | | |
| Tenant | | | | |
| AuthZ | | | | |
| API/Events | | | | |
| Failure semantics | | | | |
| Retry/Idempotency | | | | |
| Recovery | | | | |
| Version/Migration | | | | |
| Security | | | | |
| Observability/Audit | | | | |
| Performance | | | | |
| Release model | | | | |

Decision values: ADOPT / ADAPT / KEEP / DEFER / REJECT.

## 5. Findings

### C0 BLOCKER

### C1 R2 REQUIRED

### C2 SHOULD

### C3 DEFER

### REJECTED reference patterns

For every finding:

- evidence;
- risk;
- exact affected module/contract;
- required change or reason to keep current design;
- required test;
- acceptance criteria.

## 6. Test matrix

| Test | Level T0-T7 | Current | Required change | Result/evidence |
|---|---|---|---|---|
| | | | | |

Explicitly cover where applicable:

- cross-tenant;
- revoke/suspend;
- duplicate/replay;
- crash/restart;
- stale owner/fence;
- migration compatibility;
- provider outage;
- rollback/forward-only;
- real PostgreSQL/S3/broker/provider;
- performance evidence.

## 7. R2 readiness

| Requirement | Verdict | Evidence |
|---|---|---|
| Stable module identity | | |
| Independent version | | |
| Manifest | | |
| Public contracts | | |
| Package/contract digest | | |
| Compatibility | | |
| Migration namespace | | |
| Contract suite | | |
| Failure model | | |
| Security/tenant | | |
| Reference review | | |
| Release artifact | | |

## 8. Reviewer questions

1. Is authority unchanged and explicit?
2. Did any provider/framework type leak into Domain/public contract?
3. Does tenant/auth fail closed?
4. Are C0/C1 classifications justified?
5. Are tests proving failure semantics rather than only happy paths?
6. Are version/migration/rollback claims honest?
7. Is any abstraction speculative with no real consumer?
8. Can the implementation be replaced without changing consumer business logic?
9. Is the module honestly R0/R1/R2/R3/R4?

## 9. Verdict

**PASS / PASS WITH C2-C3 / FAIL**

### Blocking actions

### Deferred actions

### Accepted differences from references
