# ADR-0026: Execution Sandbox as Shared Infrastructure Service

- **Status:** Proposed
- **Date:** 2026-09-24
- **Decision scope:** SaaS execution/sandbox service boundary, business-platform ownership, tenant/policy/audit integration, local development compatibility
- **Related authority:** ADR-0025, SAAS_PLATFORM_ARCHITECTURE.md, WORKFLOW_AND_LONG_RUNNING_TASK_ARCHITECTURE.md, DEPLOYMENT_ARCHITECTURE.md
- **Implementation status:** documentation only; no container, VM, microVM, scheduler, service or production dependency is activated by this ADR

## Context

Business Platform is becoming an enterprise AI SaaS platform composed of independently releasable modules. AI and automation workloads increasingly need controlled execution environments for code, document tooling, converters, external agents, evaluation and future generated applications.

This capability is infrastructure rather than a business bounded context. It must be reusable by Business Platform, Jarvis and other SaaS consumers without moving business state, durable task authority or tenant authorization into a sandbox engine.

The platform therefore needs a stable service boundary before choosing Docker, Kubernetes, microVM or other backend products.

## Decision

### D1. Execution/Sandbox is an Infrastructure Plane service

The target SaaS architecture contains an independently deployable Execution/Sandbox Service.

    Business Modules / AI Workspace / Durable Jobs
                      |
                      v
             Execution Service Port
                      |
                      v
        Shared Execution/Sandbox Service
                      |
             +--------+--------+
             |                 |
        Worker Node        Worker Node
             |                 |
        provider(s)        provider(s)

The service is reusable infrastructure and is not part of Contract, Document, Workspace or another business domain.

### D2. Business Platform remains owner of business and durable-job state

Business Platform keeps authority for:

- Tenant and user identity;
- authorization/capability grants;
- business process state;
- Durable Job / JobStep / Attempt state;
- approval and review semantics;
- business audit lineage;
- formal business results.

The Execution Service owns only environment and execution-resource mechanics.

Execution success does not directly transition business state. Results return to the owning application/job flow for validation and commit.

### D3. Integrate through an explicit ExecutionServicePort

Business Platform consumes a protocol-neutral port with operations conceptually covering:

- capabilities;
- create environment;
- execute;
- cancel;
- destroy;
- status/reconcile;
- artifact import/export;
- optional snapshot/restore/fork.

The port uses opaque environment/execution identifiers. Business Platform must not depend on container IDs, pod names, VM IDs or service database rows.

### D4. Multi-tenant policy is enforced at both admission and execution boundaries

Every request is bound to:

- tenant;
- principal/service identity;
- owning job/step correlation;
- policy revision;
- resource quota;
- isolation profile;
- allowed workspace/artifacts;
- network egress policy;
- secret exposure policy;
- retention/cleanup policy.

Business authorization decides whether the operation is allowed. Execution infrastructure independently enforces the admitted resource/isolation contract.

No sandbox service may infer business authorization from possession of an environment ID.

### D5. No shared database with Business Platform

The Execution Service has independent persistence when needed.

Business Platform stores only:

- opaque environment/execution references;
- request/receipt digests;
- relevant attempt outcome;
- artifact references;
- reconciliation/audit evidence.

The two systems coordinate by API/event contracts, not cross-database queries or shared transactions.

### D6. Durable Task Execution and Sandbox Execution stay separate

The existing Durable Job model remains the reliable process authority.

    Business Process
        -> Durable Job / Step / Attempt
            -> Execution Service request
                -> Environment / Process
            <- Execution Receipt
        -> validate / persist / advance

A sandbox lease is not a Job lease. A node worker claim is not a Business Platform fencing token.

### D7. Production SaaS favors remote managed execution; development may use local/fake providers

Production deployments should use the shared service for workloads that need controlled execution.

Local development and tests may use:

- fake/in-memory provider;
- loopback test service;
- explicitly bounded local provider.

The semantic contract must remain the same. Development convenience must not create a production bypass.

### D8. Backend technology remains replaceable

This ADR does not select Docker, containerd, Kubernetes, Firecracker, Hyper-V or another runtime.

Selection happens behind providers according to isolation, density, startup time, statefulness, operating-system needs and operational cost.

### D9. Snapshot/restore/fork are reserved optional capabilities

The service contract may expose snapshot, restore and fork, but Business Platform cannot require them until a concrete workload and backend prove the semantics.

Filesystem copy, object-store copy or Job retry must not be mislabeled as environment snapshot.

### D10. Jarvis and Business Platform share semantics, not implementation coupling

Jarvis may use the same remote Execution/Sandbox Service through its own adapter. Business Platform does not import Jarvis Runtime crates or depend on Jarvis Desktop.

Cross-project compatibility is defined by:

- execution capability vocabulary;
- lifecycle/error semantics;
- artifact/audit ownership;
- opaque IDs and reconciliation behavior.

## Service responsibility matrix

| Concern | Business Platform | Execution/Sandbox Service |
|---|---|---|
| business state | owns | no |
| job/step state | owns | no |
| tenant authorization | owns decision | enforces admitted execution policy |
| environment lifecycle | correlation only | owns |
| node placement | no | owns |
| process/PTY mechanics | no | owns |
| resource quotas | defines tenant/product limits | enforces execution allocation |
| artifacts | owns business artifact metadata | staging/transfer only |
| audit | owns business audit | emits execution evidence |
| snapshot/fork | consumes when admitted | optional provider capability |

## Consequences

### Positive

- Sandbox becomes reusable SaaS infrastructure instead of another monolith feature.
- Business modules stay free of container/VM concerns.
- Jarvis and Business Platform can converge on one execution semantic model.
- Durable Job recovery remains intact.
- Future scale, microVM or RL use cases do not force an early business-domain redesign.

### Costs / risks

- Cross-service outcome/reconciliation semantics must be explicit.
- Tenant quotas and sandbox quotas require coordinated policy.
- Artifact/secret/network boundaries need separate security review.
- Distributed execution adds operational dependencies when production activation eventually occurs.

## Rejected alternatives

1. **Implement sandbox directly inside ai-worker.** Rejected because it couples resource isolation to one process and prevents reuse.
2. **Make Jarvis the required Business Platform sandbox runtime.** Rejected because the SaaS platform must remain independently deployable.
3. **Let Execution Service own Durable Job state.** Rejected because it duplicates execution authority and business recovery semantics.
4. **Select Kubernetes/microVM before contract proof.** Rejected as premature infrastructure commitment.
5. **Treat generated-app sandbox as the only sandbox use case.** Rejected; the service is a general execution resource.

## Acceptance criteria

Independent architecture review should confirm:

1. Business Platform business/job authority is unchanged.
2. the service can be independently released and replaced;
3. multi-tenant policy is fail-closed;
4. no shared database/cross-service transaction is required;
5. Jarvis compatibility does not create crate/runtime coupling;
6. backend products stay below the contract;
7. near-term business vertical slice is not blocked by implementing the service.

## Implementation

See:

- docs/architecture/EXECUTION_SANDBOX_SERVICE_ARCHITECTURE.md
- docs/plans/current/PLAN-0015-execution-sandbox-service-boundary.md

This ADR does not activate code implementation.
