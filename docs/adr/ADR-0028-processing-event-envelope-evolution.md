# ADR-0028: Processing event envelope evolution

> Status: Proposed / NOT ACCEPTED
> Date: 2026-09-27
> Owner: Document Intelligence / Messaging / Audit
> Related: PLAN-0014 Wave 3 D-G06/A-G07; API_AND_EVENT_CONTRACT_STANDARD

## Current facts

| Surface | Present | Gap |
| --- | --- | --- |
| Unified AuditEvent and audit_events | correlation_id, causation_id, trace_id, actor and operation identity | Values must be traced per writer; columns existing does not prove propagation |
| Processing Job/AI task | optional correlation context; processing audit writer propagates it as trace context | Not a complete causal event envelope |
| Processing outbox insert | event ID/type/schema, tenant, aggregate, payload, occurred_at | No complete persisted correlation/causation/producer envelope |
| messaging::DomainEvent | event ID/type/schema, tenant, aggregate, payload, optional trace_id | No explicit correlation_id/causation_id/producer contract |

The previous review's claim that all audit/event schemas lacked correlation
and causation columns was incorrect. This proposal targets the missing event
contract and propagation, not a second unified audit model.

## Proposed decision

- Preserve business/execution owners and transactional outbox writes. The
  publisher relays stored identity; it does not generate a new event ID or
  causal chain on retries.
- Define a versioned envelope with tenant, event identity, schema version,
  correlation identity, causation identity, producer module/version and time.
  Principal identity stays governed by the applicable event contract; do not
  put credentials or arbitrary request metadata into the envelope.
- A root command creates a stable correlation identity. A derived command or
  event records its triggering command/event identity as causation. Propagate
  both into the UoW before persisting state, Audit and Outbox atomically.
- Do not coerce current arbitrary trace strings into AuditEvent UUID fields.
  Define typed correlation identity separately and retain trace context.
- Add nullable persistence columns first; old rows are explicitly legacy
  envelope v1 with unknown causal metadata. Never fabricate historic causation.
- New producers emit the new envelope version only after compatible consumers
  are deployed. Readers dispatch by supported version; unknown versions fail
  closed to a retained error state, not an unbounded retry loop.
- Absence of a production consumer is not proof of compatibility. Use versioned
  consumer contract fixtures without installing a broker or inventing a service.

## Acceptance after approval

Tests: root-to-step-to-AI-to-result propagation, retry keeps event ID, tenant
isolation, legacy reader fixture, unsupported version rejection, out-of-order
aggregate-version handling in the consuming contract, schema mismatch, redaction,
publisher crash after send/before ack, and real PostgreSQL atomicity/replay.

The actual consuming capability must define whether an older event is ignored,
rejected or reconciled. Audit append sequence is not a substitute for that rule.

Rollback: stop v2 producers, retain columns and queued v2 rows, run only consumers
able to understand them. Do not down-convert pending events by dropping causal
metadata. Protocol/schema changes require accepted design and independent review.

## Current disposition

Proposed only. D-G06/A-G07 remains open C1; no public event DTO, migration or
runtime was changed by this proposal. No Wave 4/5 or broker activation.
