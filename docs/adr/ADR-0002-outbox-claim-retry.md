# ADR-0002: Outbox Claim/Lease/Retry Design

## Status
Accepted

## Context
The outbox pattern ensures atomicity between business writes and event
publication. The initial implementation used a simple `published: bool` flag
with polling, which does not support multiple concurrent workers safely and
provides no retry or failure handling.

As the platform scales to multiple worker instances processing outbox events,
we need a coordination mechanism that:

- Prevents duplicate delivery across workers
- Handles worker crashes gracefully
- Retries transient failures with backoff
- Provides deterministic processing order

## Decision

### Claim with FOR UPDATE SKIP LOCKED

Workers claim events using a PostgreSQL `SELECT ... FOR UPDATE SKIP LOCKED`
pattern inside a CTE-based `UPDATE ... RETURNING` query. This provides:

- **Multi-worker safety**: concurrent workers never claim the same row
- **No process-internal locks**: coordination is entirely database-level
- **No external coordinator**: no Redis, ZooKeeper, or advisory locks needed

### Lease-based ownership

Each claimed event receives a `claimed_by` worker identifier and a
`lease_until` timestamp. If a worker crashes before marking the event as
published, a periodic recovery sweep resets expired leases back to
`retry_scheduled`, making events available for other workers.

### Exponential backoff with max attempts

Failed events are retried with exponential backoff: `2^attempt` seconds,
capped at 5 minutes (300 seconds). After `max_attempts` (default 5) failures,
the event transitions to a permanent `failed` state for manual inspection.

### Deterministic ordering

Events are claimed in `(available_at, event_id)` order. This provides:

- Temporal fairness: older events are processed first
- Determinism: ties broken by UUIDv7 (time-ordered) event IDs
- Index-friendly: partial index on claimable statuses

### State machine

```text
pending -> processing -> published
                      -> retry_scheduled -> processing (retry loop)
                                         -> failed (max attempts exceeded)
```

## Consequences

- The `published: bool` column is superseded by `status VARCHAR(30)`
- Old index `idx_outbox_unpublished` is dropped; replaced by partial indexes
- Workers must run periodic lease recovery (recommended: every 30 seconds)
- The `claim_batch` query uses a CTE which requires PostgreSQL 12+
- No process-internal mutex or channel is used for cross-worker coordination;
  all safety guarantees come from PostgreSQL row-level locking
