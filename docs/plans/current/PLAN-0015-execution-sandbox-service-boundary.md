# PLAN-0015 — Execution / Sandbox Service Boundary

- **Status:** Proposed / NOT ACTIVE
- **Date:** 2026-09-24
- **Architecture gate:** ADR-0026 must be Accepted before code implementation
- **Priority:** architecture alignment only; must not displace PLAN-0014 or the Contract Business Vertical Slice
- **Cross-project dependency:** Jarvis ADR-0023 / EXECUTION_SANDBOX_ARCHITECTURE.md semantic compatibility

## 1. Goal

Establish the Business Platform side of the shared Execution/Sandbox Service boundary without prematurely building infrastructure.

Success means Business Platform knows:

- what it owns;
- what the Execution Service owns;
- how Durable Jobs call the service;
- how tenant/policy/audit context crosses the boundary;
- how errors/reconciliation map back into existing Job semantics;
- how the contract stays compatible with Jarvis without importing Jarvis runtime code.

## 2. Non-goals

This plan does not authorize:

- Docker/Kubernetes/Firecracker/VM deployment;
- a new production microservice;
- migration/schema work;
- generated-app runtime;
- replacing existing document workers;
- blocking the Contract Business Vertical Slice;
- adopting Jarvis crates;
- snapshot/fork implementation.

## 3. Work order

### M0 — independent architecture review

Review:

- ADR-0026;
- EXECUTION_SANDBOX_SERVICE_ARCHITECTURE.md;
- ADR-0025 SaaS module boundary;
- Durable Task architecture;
- deployment/security/authorization architecture;
- Jarvis ADR-0023 compatibility.

Acceptance:

- P0/P1 = 0;
- no second durable task authority;
- no business-module ownership leak;
- no vendor/backend lock-in.

### M1 — current code boundary map

Map concrete current owners:

- business-api;
- business-worker;
- ai-worker;
- Durable Job/JobStep/Attempt;
- Artifact Store;
- Audit;
- IAM/policy;
- existing ExternalCapabilityProvider seams.

Output:

- exact port insertion point;
- fields already available for tenant/job correlation;
- missing fields, if any;
- no code changes unless separately activated.

### M2 — contract-only slice

When authorized, implement the smallest ExecutionServicePort and fake implementation.

Minimum operations:

- capabilities;
- create;
- execute;
- cancel;
- destroy;
- status/reconcile.

Minimum typed failures:

- UnsupportedCapability;
- AdmissionDenied;
- QuotaExceeded;
- ServiceUnavailable;
- OutcomeUnknown;
- CleanupUnproved.

Tests prove JobStep retry/reconciliation behavior without a real sandbox runtime.

### M3 — first real consumer decision

Choose a low-risk technical workload only after M2 proves the contract.

Selection criteria:

- clear isolation benefit;
- bounded inputs/outputs;
- no new business-domain semantics;
- existing Job ownership;
- deterministic verification.

The Contract vertical slice is not required to be this consumer.

### M4 — service extraction decision

Only create an independently deployable execution-control service when at least two real consumers or one hard security/scale requirement justify it.

A separate ADR must decide:

- persistence;
- node registration;
- placement;
- service protocol;
- production HA;
- concrete backend(s).

### M5 — backend decision

Container/microVM/VM technology selection is deferred until workload evidence exists.

Evaluate:

- startup latency;
- isolation;
- density;
- Windows/Linux requirements;
- statefulness;
- network policy;
- operational cost;
- deployment constraints.

## 4. Cross-project compatibility checks

Business Platform and Jarvis should agree semantically on:

- EnvironmentSpec;
- IsolationProfile;
- EnvironmentRef/Handle;
- ExecutionRequest;
- ExecutionReceipt;
- capability negotiation;
- OutcomeUnknown;
- cleanup proof;
- optional snapshot/restore/fork.

They must remain independent in:

- durable task/run storage;
- policy implementation;
- business schemas;
- release cadence;
- local desktop runtime.

## 5. Verification

Architecture/document stage:

- document link/index checks;
- no contradictory owner declarations;
- review against ADR-0025 and Durable Task architecture;
- review against Jarvis ADR-0023.

Future code stage:

- unit tests for request mapping;
- contract tests for fake provider;
- lease/fence stale result tests;
- ambiguous outcome reconciliation tests;
- tenant/policy fail-closed tests;
- architecture fitness rule preventing business modules from depending on backend SDKs.

## 6. Risks

### Premature platform work

Mitigation: Proposed/NOT ACTIVE, two-consumer extraction gate, no backend selection.

### Duplicate task authority

Mitigation: JobStep/Attempt remains Business Platform authority; service IDs are opaque correlations.

### Jarvis coupling

Mitigation: share vocabulary/contracts conceptually, not crates or databases.

### Security downgrade

Mitigation: requested guarantees are explicit and unsupported guarantees fail closed.

## 7. Completion definition

This plan's architecture phase is complete when:

1. ADR-0026 is independently reviewed and Accepted;
2. cross-project semantic compatibility with Jarvis ADR-0023 is recorded;
3. current code boundary map identifies a minimal insertion point;
4. PLAN-0014 and Contract vertical-slice priority remain unchanged;
5. no infrastructure backend has been selected without a separate decision.

Code phases remain NOT ACTIVE until separately authorized.
