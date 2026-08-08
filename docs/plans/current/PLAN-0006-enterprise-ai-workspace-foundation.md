# PLAN-0006: Enterprise AI Workspace Foundation

> Status: Proposed  
> Revision: 0  
> Date: 2026-08-06  
> Owner: Platform Foundation / Agent Integration  
> Base SHA: `a3f78a7d6e1a745d30cd0e6cf257a870fc95aa58`
> Integration Mode: local solo fast-forward  
> Pull Request: not required  
> Stop Policy: blockers-only  
> Architecture Decision: `ADR-0018-enterprise-ai-workspace-and-capability-security.md`

## 1. Goal

Build the minimum durable Enterprise AI Workspace foundation and prove one
read-only business assistant vertical slice against the existing Document
Management, Document Intelligence, Runtime Audit, Policy, PostgreSQL, and
MinIO boundaries.

The completed slice must allow an authenticated tenant user to open a
Workspace, send a Turn, have a replaceable Agent Runtime select an allow-listed
Skill and typed business Tool, receive a task-scoped Capability Grant, query a
document processing status, record bounded Tool/Observation evidence, stream
the answer, and recover from a process interruption without granting database,
Shell, filesystem, or arbitrary HTTP access.

## 2. Why this plan is next

PLAN-0001 through PLAN-0005 established service integrity, persistence/query
architecture, durable document processing, and runtime governance. The current
`agent-integration` crate and `agent-adapter` application remain placeholders.

Cloudflare OS demonstrates that an enterprise Agent product requires a
Workspace, shared Skill and Context assets, task-scoped capabilities, derived
data protection, and persistent artifacts. PLAN-0006 begins that product layer
without adopting Cloudflare OS or prematurely implementing arbitrary generated
applications.

## 3. Non-goals

PLAN-0006 does not implement:

- arbitrary Generated App or Gadget execution;
- workerd, WASI, container, isolate, or microVM sandbox selection;
- generic Workflow DAG or scheduler;
- general-purpose MCP marketplace;
- Shell, SQL, filesystem, browser, or arbitrary HTTP tools;
- Contract/Customer/Approval business domains that do not yet have an accepted
  vertical slice;
- high-risk business writes;
- ActionPlan confirmation UI beyond stable interfaces and fixtures;
- Artifact sharing or Blueprint execution;
- realtime multiplayer collaboration;
- full RAG/knowledge graph/search platform;
- model training or fine-tuning;
- replacement of Runtime Audit, Durable Processing, or business Application
  Services.

## 4. Architecture preflight

### 4.1 Capability boundaries

- **Workspace Management:** Workspace, Conversation, Thread, Turn and attached
  resource references.
- **Agent Integration:** AgentDefinition, SkillDefinition, ContextDefinition,
  ToolDefinition, AgentRun, ToolInvocation and Observation references.
- **Identity and Access:** Principal and Delegation Grant.
- **Policy:** Capability authorization, issue, expiry, revocation and validation.
- **Document Management / Intelligence:** authoritative document and processing
  data queried through public Application ports.
- **Audit:** formal runtime audit evidence.
- **Durable Task Execution / processing adapters:** execution recovery; Workspace
  does not duplicate Job state.

### 4.2 Data owners

| Data | Owner |
|---|---|
| Workspace/Conversation/Turn | Workspace Management |
| Skill/Context/Tool definitions | Agent Integration |
| AgentRun/ToolInvocation | Agent Integration |
| Principal/Delegation | Identity and Access |
| Capability Grant validity | Policy |
| Document identity and version | Document Management |
| Processing Job/Step/Candidate | Document Intelligence |
| Audit evidence | Audit |
| Model usage | AI Application / Model Gateway seam |

### 4.3 Invariants

1. Workspace and Agent data are tenant scoped.
2. An Agent Run cannot invoke a Tool without an unexpired, unrevoked,
   tenant-matching Capability Grant.
3. A Grant cannot exceed the current originating principal authority.
4. Tool input cannot broaden the Grant resource, action, or field scope.
5. Agent Adapter never accesses business persistence directly.
6. The read-only slice calls public Application query ports.
7. Tool and Observation records contain identifiers, versions, hashes,
   classifications, bounded summaries and trace references, not unrestricted
   document content.
8. Duplicate Turn or Tool requests converge through idempotency keys.
9. Agent Runtime failure cannot corrupt Workspace state or business state.
10. Business Platform continues to operate when Agent components are stopped.

### 4.4 Consistency

- Workspace Turn acceptance and Outbox event commit in one local transaction.
- Agent Run and ToolInvocation transitions use optimistic versions.
- Capability issue/revoke and use records are tenant scoped and auditable.
- Business query is read-only; no distributed transaction is introduced.
- Runtime Audit is correlated through trace and invocation identifiers.
- PostgreSQL is production authority; SQLite is local single-process only if a
  complete adapter is implemented and explicitly validated.

## 5. Target user flow

```text
1. User opens the Document detail page.
2. UI creates/opens a tenant-scoped Workspace.
3. UI attaches only document id/version as page context.
4. User asks: “这份文档的处理进度和最近失败原因是什么？”
5. Workspace creates a durable Turn and Agent Run.
6. Agent selects `document.processing_status.explain` Skill.
7. Agent requests the document status Tool Capability.
8. Policy verifies user authority and issues a short-lived resource-scoped Grant.
9. Agent Adapter validates Tool schema and Grant.
10. Document Intelligence query port returns a bounded Agent Read DTO.
11. ToolInvocation, Observation and Audit references are recorded.
12. Agent produces an answer with document/job references.
13. SSE streams the result; reconnect resumes from the durable event cursor.
```

## 6. Public contracts

### 6.1 Workspace API

Minimum endpoints or equivalent versioned contracts:

```text
POST /api/v1/ai/workspaces
GET  /api/v1/ai/workspaces/{workspace_id}
POST /api/v1/ai/workspaces/{workspace_id}/turns
GET  /api/v1/ai/workspaces/{workspace_id}/turns
GET  /api/v1/ai/workspaces/{workspace_id}/events
POST /api/v1/ai/runs/{run_id}/cancel
```

Rules:

- authentication and tenant are server trusted;
- create/submit commands require `Idempotency-Key`;
- lists use opaque keyset cursors;
- SSE uses durable sequence/cursor semantics;
- errors use stable codes and trace ids;
- resource attachments are typed references, not arbitrary URLs.

### 6.2 Registry contracts

```text
RegisterSkill / PublishSkillVersion / DisableSkillVersion
RegisterContext / PublishContextVersion / DisableContextVersion
RegisterTool / PublishToolVersion / DisableToolVersion
ResolveApplicableSkills
ResolveContextProjection
```

PLAN-0006 may seed built-in definitions in code, but persistence and versioned
read contracts must exist so they are not permanent hard-coded prompts.

### 6.3 Capability contracts

```text
RequestCapability
IssueCapability
ValidateCapabilityUse
RevokeCapability
GetCapabilityDecision
```

The Tool invocation carries only a Grant reference and normalized arguments.
It cannot carry client-asserted permissions.

### 6.4 Read-only business Tool

Initial Tool:

```text
document.processing_status.get
```

Input:

```text
tenant inferred from trusted context
document_id
optional expected_document_version
```

Output is a bounded Agent Read DTO containing:

- document reference and version;
- current processing job reference;
- processing state and current step;
- bounded progress counters;
- last safe error classification and timestamp;
- review/candidate availability indicators;
- updated timestamp;
- trace/resource links.

It must not return object-store keys, raw extracted text, prompts, credentials,
full provider errors, database details, or unrestricted audit payloads.

### Document revision precondition

PLAN-0006 depends on PLAN-0008's Document Revision contract. A workspace page
context and `document.processing_status.get` must prefer the stable
`document_revision_id` when a revision is selected. During the compatibility
window, a request containing only `document_id` resolves the current revision
through the Document Management query port; clients must not infer a revision
from a storage key or provider version. Workspace/Agent state remains
non-authoritative and PLAN-0006 remains Proposed until its own activation
gate is satisfied.

## 7. Work packages

| ID | Scope | Required evidence |
|---|---|---|
| WP-00 | Adopt ADR-0018 and synchronize architecture/reference/plan docs | Documentation review |
| WP-01 | Workspace domain and application contracts | Pure domain + Fake port tests |
| WP-02 | Skill, Context and Tool registries with version/status semantics | Registry contract tests |
| WP-03 | Delegated Agent principal and task-scoped Capability Grant | Policy/domain/security tests |
| WP-04 | AgentRun, Turn, ToolInvocation and Observation lifecycle | Domain/application tests |
| WP-05 | PostgreSQL persistence, migrations, keyset queries and local transactions | PostgreSQL adapter tests |
| WP-06 | `agent-adapter` typed HTTP/MCP boundary with no DB access | API/architecture tests |
| WP-07 | Document processing read-only Tool through public query ports | Application/tool contract tests |
| WP-08 | Replaceable Agent Runtime port plus deterministic Fake Runtime | Runtime contract tests |
| WP-09 | Workspace API, SSE durable cursor and reconnect | API/process recovery E2E |
| WP-10 | Runtime Audit correlation and bounded Observation evidence | Audit/security tests |
| WP-11 | Model Provider seam, usage record and hard request budget | Mock/provider contract tests |
| WP-12 | Threat model, metrics, dashboards and runbook draft | Docs + telemetry tests |
| WP-13 | Full SQLite/local and PostgreSQL/MinIO CI evidence | E2E + Architecture Fitness |

## 8. Detailed implementation requirements

### WP-01 Workspace

Add a focused capability crate or modules without forcing a new service:

```text
workspace/domain
workspace/application
workspace/infrastructure
workspace/api
```

Required lifecycle:

```text
Active → Archived
```

Conversation/Turn records are append-oriented. Editing a user message creates a
new branch/revision rather than silently replacing historical evidence.

### WP-02 Registries

Definitions must include stable ids, monotonic versions, publish status, input
and output schema hashes, risk class and compatibility metadata.

A published version is immutable. Changes create a new version. Disabled
versions cannot be selected for new Runs but remain readable for historical
replay and audit.

### WP-03 Capability Grant

Implement strong value objects for:

- ResourceScope;
- AllowedAction;
- FieldPolicy;
- CapabilityConstraint;
- GrantExpiry;
- PolicyVersion.

Required failure cases:

- forged tenant;
- forged principal;
- wrong Agent Run;
- resource outside scope;
- action outside scope;
- disallowed field request;
- expired/revoked Grant;
- policy version invalidation;
- origin user permission revoked after issue;
- replay after terminal use when single-use is required.

### WP-04 Agent evidence

AgentRun lifecycle should be explicit and fail closed, for example:

```text
Queued → Running → WaitingTool → Running → Completed
                     ├→ Failed
                     ├→ Cancelled
                     └→ Interrupted/Recoverable
```

Exact states may change during design, but process interruption cannot be
reported as business success. ToolInvocation has its own lifecycle and
idempotency fingerprint.

### WP-05 Persistence

PostgreSQL tables should remain owner-oriented, not a generic event dump.
Likely tables include:

```text
ai_workspaces
ai_workspace_members
ai_conversations
ai_turns
agent_definitions
skill_definitions / skill_versions
context_definitions / context_versions
tool_definitions / tool_versions
agent_runs
tool_invocations
capability_grants
agent_observations
model_usage_records
```

Names are provisional; migrations require manifest/checksum updates and
PostgreSQL/SQLite semantic parity where SQLite support is claimed.

### WP-06 Agent Adapter

`agent-adapter` is a composition root and protocol adapter. It may depend on
Agent Integration application contracts and typed clients/ports, but must not
receive `PgPool` or write business tables.

No generic endpoint such as `execute_tool(name, arbitrary_json)` is accepted
unless Tool resolution, schema, Capability and version are server controlled.

### WP-07 Document Tool

Reuse existing Document Intelligence query contracts. If the required query
port is missing, add a narrow public Application query; do not query processing
tables from Agent Adapter.

### WP-08 Runtime portability

Define a port such as:

```text
AgentRuntime
- start_run
- continue_run
- cancel_run
- recover_run
```

The exact Rust API is implementation-owned. A deterministic Fake Runtime must
prove the Workspace/Tool/Capability flow without model network access. A real
runtime adapter may target jarvis-rs or another service, but is not required to
be embedded in the same repository.

### WP-09 Streaming and recovery

SSE events must have durable tenant/workspace/run-local sequence values.
Reconnect with `Last-Event-ID` or equivalent resumes without dropping or
reordering committed events. A process crash after Turn commit but before Agent
start must be recoverable.

### WP-10 Audit and Observation

Extend Runtime Audit by correlation, not by storing model transcripts as audit
truth. Observation stores bounded metadata and classifications. Tests must prove
that raw document text, object keys, provider credentials and full prompts do
not enter Audit/Observation by default.

### WP-11 Model usage

PLAN-0006 needs only a minimum provider-independent request and usage record:

- provider/model ids;
- prompt/context schema versions;
- input/output token counts when available;
- latency;
- outcome classification;
- cost fields when available;
- tenant/workspace/run attribution;
- hard timeout and token/context budget.

Full routing, fallback, budget policy and evaluation governance are deferred.

## 9. Security threat model

Mandatory threats and controls:

| Threat | Minimum control |
|---|---|
| Forged tenant/page context | Trusted server identity and resource reload |
| Agent privilege amplification | Task-scoped Grant ≤ user authority |
| Tool argument scope expansion | Normalize and validate against Grant |
| Prompt injection requests new tools | Capability request + allow-list, never automatic |
| Cross-tenant Workspace access | Tenant keys at API/Application/Repository levels |
| Sensitive data in logs | Redaction, bounded DTO and forbidden-field tests |
| Duplicate Tool side effects | Read-only slice + idempotency foundation |
| Stale/revoked permission | Revalidation policy and fail-closed use |
| Runtime crash | Durable Turn/Run state and recovery |
| Agent Adapter direct persistence | Architecture fitness rule |
| Arbitrary network/tool use | No generic HTTP/Shell/SQL/File tools |

## 10. Quality attributes

### Reliability

- committed Turn survives process restart;
- duplicate idempotency key converges;
- Agent Runtime interruption is recoverable or terminally classified;
- stale Tool completion cannot overwrite a newer Run version;
- SSE reconnect has no committed-event loss.

### Security

- all tenant/authorization negative tests pass;
- Grant expiry and revocation fail closed;
- Tool outputs are field-filtered;
- no unrestricted sensitive content in telemetry.

### Performance

Initial budgets to validate, not silently assume:

- Workspace/Turn command P95 excluding model call;
- Capability validation P95;
- read-only Tool P95;
- SSE reconnect and first recovered event;
- bounded context size and Tool response size.

Exact thresholds are set during implementation after baseline measurement and
recorded in `QUALITY_ATTRIBUTE_SCENARIOS.md` if they become long-term policy.

### Replaceability

The full E2E must pass with Fake Runtime and Mock Model Provider. Business
queries cannot depend on a provider SDK.

## 11. Observability

Required dimensions:

- tenant-safe workspace/run identifiers;
- skill/tool/version;
- capability decision outcome and reason class;
- Tool latency/outcome;
- model/provider usage;
- SSE reconnect count;
- recovery attempts/outcome;
- authorization and schema rejection counters.

Never label metrics with user input or unbounded resource ids.

## 12. Fitness functions

Extend `scripts/check-architecture.ps1` or equivalent checks to prove:

1. `apps/agent-adapter` does not depend on SQLx or business persistence adapters;
2. Agent Integration core does not depend on Axum, SQLx, Reqwest or provider SDKs;
3. no generic SQL/Shell/filesystem/arbitrary HTTP Tool is introduced;
4. Tool implementations depend on public Application query/command ports;
5. Workspace/Agent tables are not imported into Contract/Document domain code;
6. Generated App runtime dependencies are absent from PLAN-0006;
7. migrations and manifests include all new tables;
8. API/Event schemas and compatibility tests exist.

## 13. Test matrix

### Domain/Application

- Workspace lifecycle and membership;
- immutable published registry versions;
- AgentRun/ToolInvocation transitions;
- Capability scope, expiry and revocation;
- Observation redaction;
- idempotency conflicts.

### API

- authentication and tenant enforcement;
- keyset pagination;
- Idempotency-Key replay/conflict;
- SSE reconnect;
- cancellation;
- stable error responses.

### Adapter

- PostgreSQL transactions and optimistic concurrency;
- SQLite semantics if supported;
- Agent Adapter schema/Capability validation;
- Document query Tool contract;
- Fake Runtime recovery.

### Process E2E

1. create Workspace;
2. submit Turn;
3. crash after commit before Run starts;
4. restart and recover;
5. issue scoped Capability;
6. invoke document status Tool;
7. record Observation/Audit;
8. stream final Turn;
9. reconnect and verify sequence;
10. prove no business state mutation.

### CI

- fmt;
- check all targets/features;
- Clippy `-D warnings`;
- workspace tests;
- Architecture Fitness;
- migration manifests;
- real PostgreSQL and MinIO E2E;
- security negative tests;
- diff/governance checks.

## 14. Documentation synchronization

Implementation must update:

- `docs/architecture/ARCHITECTURE_STATUS.md`;
- `docs/architecture/SECURITY_ARCHITECTURE.md` if detailed controls change;
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`;
- `docs/standards/API_AND_EVENT_CONTRACT_STANDARD.md`;
- `docs/standards/ARCHITECTURE_FITNESS_FUNCTIONS.md`;
- relevant migration manifests;
- local/CI/runbook documentation;
- this plan with candidate evidence.

## 15. Completion definition

PLAN-0006 becomes `Accepted Candidate` only when:

- every work package is complete;
- the documented read-only vertical slice runs end to end;
- Agent Adapter has no direct business persistence access;
- Capability negative tests are green;
- process crash/restart and SSE reconnect evidence are green;
- Audit/Observation redaction tests are green;
- Fake Runtime and Mock Provider E2E are green;
- real PostgreSQL/MinIO CI is green;
- all Architecture Fitness checks pass;
- documentation and ADR remain synchronized;
- no Generated App, arbitrary tool or business write scope was added.

The implementation task must stop at `Accepted Candidate`. It must not merge to
`main`, archive the plan, delete the branch, or start PLAN-0007 without a
separate integration instruction.

## 16. Accepted risks and deferred work

At proposal time the following are intentionally deferred:

- production Agent Runtime selection;
- jarvis-rs protocol details;
- high-risk write operations;
- Artifact/Blueprint sharing;
- Generated App sandbox;
- realtime collaboration;
- full Model Gateway routing and cost policy;
- broad enterprise knowledge retrieval.

Deferral is not permission to create hidden shortcuts. Interfaces must preserve
replaceability and the security boundaries in ADR-0018.
