# ADR-0013: Unified Runtime Audit Model

> Status: Accepted

## Decision

Use one validated `AuditEvent` language and tenant-scoped append-only model for
business, processing, governance, and repair evidence. Audit is separate from
technical logs and domain/execution events. Owner adapters append it in the
same transaction as state and Outbox.

## Consequences

Existing document and processing audit writes migrate through a shared mapper;
the hash chain provides tamper evidence, while WORM remains an operational
extension. Public DTOs and logs remain redacted.
