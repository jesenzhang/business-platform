# ADR-0013: Unified Runtime Audit Model

> Status: Accepted

## Decision

Use one validated `AuditEvent` language and tenant-scoped append-only model for
business, processing, governance, and repair evidence. Audit is separate from
technical logs and domain/execution events. Owner adapters append it in the
same transaction as state and Outbox.

Revision 1 makes append order explicit with `tenant_id + stream_sequence`,
`recorded_at`, and `chain_version`. Migration 013 deterministically assigns
sequence values to legacy rows but marks them `chain_version=0`; those rows
are a documented legacy/unverified boundary. New version-1 rows use the next
sequence and a null previous hash as the chain genesis. Business
`occurred_at` is a filter only and never controls chain linkage.

## Consequences

Existing document and processing audit writes migrate through a shared mapper;
the hash chain provides tamper evidence, while WORM remains an operational
extension. Public DTOs and logs remain redacted.
