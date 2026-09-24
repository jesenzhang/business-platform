# Execution / Sandbox Service Architecture

- **Status:** Proposed architecture baseline
- **Date:** 2026-09-24
- **Authority candidate:** ADR-0026
- **Scope:** shared SaaS execution infrastructure consumed by Business Platform and other authorized products
- **Non-goal:** selecting or deploying a production container/microVM stack in this change

## 1. Position in the SaaS architecture

Execution/Sandbox is part of the Infrastructure Plane.

    Experience / API / MCP / Agent surfaces
                  |
                  v
         Business Applications
                  |
                  v
    Durable Task / AI Workspace / Adapters
                  |
                  v
         Execution Service Port
                  |
                  v
      Shared Execution/Sandbox Service
                  |
          Placement / Nodes
                  |
       Sandbox/Process Providers

It is not a Business Module and it does not own business aggregates.

## 2. Ownership boundaries

### Business Platform owns

- tenant and principal identity;
- authorization decision;
- Business Module / Workspace / Job semantics;
- Durable Job, JobStep and Attempt lifecycle;
- approval/review;
- business artifact metadata;
- business audit;
- business result validation and commit.

### Execution Service owns

- environment lifecycle;
- provider admission;
- node/resource placement;
- process/PTY lifecycle;
- filesystem/network/secret isolation;
- resource limits;
- execution cleanup;
- execution-level telemetry;
- optional snapshot/restore/fork.

### Execution Service does not own

- contract/document/workspace state;
- business workflow state;
- durable job truth;
- business approvals;
- formal AI suggestions or business results.

## 3. Core service contract

The port should be protocol-neutral and small.

### Capabilities

Returns provider/service guarantees, versions and limits.

### CreateEnvironment

Input:

- tenant/principal correlation;
- owning job/step/attempt correlation;
- policy revision;
- EnvironmentSpec;
- requested isolation/capabilities;
- resource limits;
- artifact/workspace inputs.

Output:

- opaque EnvironmentRef;
- admitted capabilities;
- lease/generation facts;
- expiry/retention facts.

### Execute

Input:

- EnvironmentRef;
- exact command/tool operation;
- cwd;
- stdin/PTY policy;
- timeout/resource bounds;
- expected output/artifact policy.

Output:

- ExecutionReceipt;
- bounded output or artifact references;
- exit/outcome classification;
- usage/resource facts.

### Cancel / Destroy

Returns explicit receipt. "Requested" is different from "proved stopped".

### Status / Reconcile

Allows the Business Platform worker to resolve timeout, lost response or uncertain cleanup without blind re-execution.

### Optional Snapshot / Restore / Fork

Capability-negotiated only.

## 4. Identity model

Keep these identities distinct:

    Tenant/User
        |
    Business Process
        |
    Job / Step / Attempt
        |
    EnvironmentRef
        |
    ExecutionRef
        |
    Provider-local process/container/vm identity

Only the first three are Business Platform durable authority.

EnvironmentRef and ExecutionRef are opaque external correlations.

## 5. Error model

The client contract should distinguish at least:

- UnsupportedCapability;
- AdmissionDenied;
- QuotaExceeded;
- ProvisionFailed;
- ExecutionFailed;
- TimedOut;
- Cancelled;
- LeaseLostOrStaleGeneration;
- OutcomeUnknown;
- CleanupUnproved;
- ServiceUnavailable.

Rules:

- authorization/policy failures are terminal unless policy changes;
- ServiceUnavailable may be retried according to Durable Job policy;
- OutcomeUnknown is reconciled before retry when duplicate side effects are possible;
- stale generations never commit success.

## 6. Multi-tenant controls

Admission policy includes:

- tenant execution concurrency;
- per-user/service concurrency;
- CPU/memory/disk limits;
- wall-clock limits;
- process count;
- artifact size;
- network egress allowlist;
- secret scopes;
- environment retention;
- provider/backend eligibility;
- optional cost budget.

Execution Service enforces these limits but does not decide business permission.

## 7. Security model

### Filesystem

Inputs should be projected from immutable or checksum-verified artifacts/workspaces where practical. Writable scratch is separate from authoritative business storage.

### Network

Profiles should support:

- none;
- explicit allowlist;
- controlled platform egress;
- unrestricted only when specifically approved.

No implicit production network fallback.

### Secrets

Secrets are injected only for the exact execution scope and should not be persisted into snapshots/artifacts unless explicitly designed.

### Artifacts

Exported artifacts pass size/type/checksum validation before Business Platform accepts them.

### Isolation

A backend name is not a guarantee. The service advertises and proves effective guarantees.

## 8. Durable Job integration

The recommended sequence is:

    JobStep claim
      -> validate tenant/business permission
      -> build deterministic execution request
      -> create/reuse admitted environment
      -> execute
      -> persist external correlation
      -> validate receipt/artifacts
      -> atomically commit JobStep transition
      -> enqueue next step / business application action

If the process dies after dispatch but before result commit, the replacement worker uses persisted correlation and Status/Reconcile before resending work when duplication would be unsafe.

A sandbox lease never substitutes for Business Platform's JobStep claim/fence.

## 9. Artifact ownership

Business Platform's Artifact Store remains the owner of business artifacts.

Execution Service may own temporary staging/cache objects. Promotion requires:

- checksum;
- media/schema validation where applicable;
- tenant binding;
- provenance;
- retention policy;
- explicit transfer into the authoritative Artifact Store.

## 10. Audit and observability

Execution evidence should expose:

- tenant/principal correlation;
- job/step/attempt correlation;
- environment/execution references;
- policy revision;
- provider/backend class;
- admitted capability profile;
- timestamps/outcome;
- resource usage;
- network policy class;
- artifact digests;
- cleanup result.

It must not log raw secrets or unnecessary model/business content.

Business Platform links this evidence into its audit model rather than delegating business audit ownership.

## 11. Deployment model

### Local/dev

    Business Platform
      -> fake or loopback ExecutionServicePort
      -> bounded local test provider

No production guarantee may be inferred from this profile.

### SaaS production

    business-api / business-worker / ai-worker
                  |
          ExecutionServicePort
                  |
         execution-control service
                  |
        scheduler / placement
          |             |
       node A         node B
          |             |
       provider       provider

The service is independently deployable and horizontally scalable.

## 12. Backend selection criteria

A later ADR should choose concrete backends using:

- isolation strength;
- startup latency;
- density;
- OS compatibility;
- nested toolchain needs;
- stateful snapshot support;
- network policy;
- operational maturity;
- cost;
- enterprise deployment constraints.

No backend is selected here.

## 13. Relationship to existing platform modules

### AI Worker

AI Worker may request controlled execution for resource/tool steps but must not embed a private sandbox scheduler.

### Enterprise AI Workspace

Workspace may request environment-backed tools through capability grants. Workspace permissions are business/application policy; the service enforces the admitted execution subset.

### Document processing

Existing fixed document pipelines continue to run without this service unless a concrete step needs stronger isolated execution. This proposal does not block current document/contract work.

### Generated App Sandbox

Future generated applications become one consumer of this shared service rather than a separate infrastructure stack.

### Jarvis

Jarvis may consume the same service through an independent adapter. Shared semantics do not mean shared durable state or Rust crate coupling.

## 14. Compatibility vocabulary with Jarvis

Both projects should align on these concepts:

- EnvironmentSpec;
- IsolationProfile;
- ExecutionTarget;
- EnvironmentRef/Handle;
- ExecutionRequest;
- ExecutionReceipt;
- provider capabilities;
- OutcomeUnknown;
- cleanup proof;
- optional snapshot/restore/fork.

Wire schemas may differ initially. Semantic compatibility is the first requirement.

## 15. Implementation phases

### Phase 0 — architecture only

Accept/review ADR-0026 and the shared vocabulary. No code.

### Phase 1 — client contract

Add a minimal ExecutionServicePort plus fake implementation and contract tests. No remote service required.

### Phase 2 — one real low-risk consumer

Use an existing technical workload where isolation is useful and business semantics are simple. Do not make the Contract vertical slice depend on this phase.

### Phase 3 — independently deployable service

Only after two real consumers justify it, extract service deployment, persistence, placement and operational contracts.

### Phase 4 — stronger backends

Container/microVM/VM selection after workload and security evidence.

## 16. Acceptance criteria

- no business domain depends on a backend product;
- durable jobs remain authoritative;
- service can be replaced independently;
- tenant/policy context is explicit;
- network/secret/artifact boundaries are explicit;
- retry/reconciliation is compatible with existing Job semantics;
- Jarvis compatibility requires no Jarvis dependency;
- current Contract vertical-slice schedule remains unblocked.
