# ADR-0017: Enterprise AI Workspace and Agent Capability Security

> Status: Accepted  
> Date: 2026-08-06

## Context

The existing architecture correctly makes the Rust Business Platform the
system of record and treats Agent as a replaceable entry point. After reviewing
Cloudflare OS and the current repository, the Agent boundary is too thin to
cover a complete employee AI product. The repository has durable business
processing, audit, integrity, and repair foundations, while `agent-integration`
and `agent-adapter` remain placeholders.

A production business assistant needs Workspace, Skill, Context, Tool,
Artifact, delegated identity, task-scoped authorization, observation lineage,
and model governance. Implementing these concerns only as prompts or generic
MCP configuration would create ambient privilege, duplicate business rules,
and lose control of derived data sharing.

## Decision

1. Add Enterprise AI Workspace as an explicit product and platform layer above
   the Rust Business Platform.
2. Keep the Business Platform as the sole authority for business facts,
   invariants, transactions, versions, workflow decisions, and formal audit.
3. Add task-scoped `CapabilityGrant` between delegated Agent identity and typed
   business tools. A grant is tenant-, principal-, task-, resource-, action-,
   field-, policy-, and expiry-bound, revocable, and never broader than the
   originating user authority.
4. Add Agent `Observation` and derived Artifact access requirements so that
   sharing an Agent-generated result requires reauthorization against its
   source resources.
5. Keep all formal writes on the existing
   `Prepare → Preview → Confirm → Execute` ActionPlan path. Agent confirmation
   cannot replace target aggregate version checks or business authorization.
6. Introduce Artifact and Blueprint as non-authoritative platform capabilities.
   They may own content, layout, versions, source references, and sharing state,
   but not Contract, Customer, Approval, Finance, Project, or Document facts.
7. Treat Generated App execution as a future independent sandbox boundary. It
   is not part of PLAN-0006 and requires a separate ADR before choosing workerd,
   WASI, containers, isolates, or microVMs.
8. Use Cloudflare OS as a reference project only. Do not introduce a required
   Cloudflare account, Workers cloud service, Durable Objects, Dynamic Workers,
   or Cloudflare AI Gateway dependency into the Business Platform.
9. Keep Agent Runtime replaceable. jarvis-rs may implement the Agent loop, but
   integration must use stable protocols and must not create source-level or
   database-level coupling.

## Consequences

### Positive

- The project gains a complete architecture for an embedded business assistant
  without weakening the existing business authority model.
- Agent access becomes narrower than normal user access and auditable per task.
- Existing Runtime Audit and durable processing can be reused rather than
  duplicated inside an Agent subsystem.
- Generated reports and applications receive explicit lineage and sharing
  constraints.
- Cloudflare OS product lessons can be absorbed without adopting its runtime
  stack.

### Costs

- New Workspace, registry, policy, observation, artifact, and UI capabilities
  must be implemented and governed.
- Capability revocation, field filtering, source reauthorization, and model
  usage accounting add persistence and test complexity.
- Integration contracts between Business Platform and a replaceable Agent
  Runtime must be maintained.

### Rejected alternatives

- **Adopt Cloudflare OS directly:** rejected because of runtime coupling,
  early-access self-hosting maturity, and mismatch with the authoritative
  business platform role.
- **Keep Agent as generic MCP only:** rejected because ambient connector access
  cannot express task-scoped resource and field constraints.
- **Put business rules in Skills:** rejected because UI, API, Worker, and Agent
  would diverge.
- **Implement arbitrary Generated Apps first:** rejected because capability,
  lineage, artifact ownership, and sandbox boundaries must exist first.
