# ADR-0011: Worker Leases, Fencing, and Crash Recovery

- Status: Accepted
- Date: 2026-08-03
- Decision owners: Platform Foundation

## Context

Workers can stop after claiming a job, overlap after lease expiry, or retry a
request. A process-local mutex cannot protect independent adapters or
processes.

## Decision

PostgreSQL claims use row locking with `FOR UPDATE SKIP LOCKED`. Each claim
stores owner, opaque token, expiry, and a monotonic fence version. Every
heartbeat and write is conditioned on the complete lease identity and expected
aggregate version. Expired claims are reclaimed by a durable scan; stale
workers receive a safe lease-lost error. SQLite uses `BEGIN IMMEDIATE`, one
local worker, and explicitly does not claim distributed equivalence.

## Consequences

Crash recovery is deterministic and stale writes cannot win after reclaim.
Heartbeat cadence is configuration validated to be below half the lease.
Multi-worker PostgreSQL and local SQLite tests remain separate evidence. A
future distributed SQLite mode would require a new ADR and deployment model.

## Revision 1 clarification

The complete lease identity (tenant, owner, opaque token, fence version, and
unexpired lease) is required by every execution unit of work. Heartbeat guards
are owned by the worker operation and joined during shutdown. AI reclaim either
schedules a bounded retry with backoff or atomically marks both task and
waiting job failed; a stale worker cannot persist a candidate or transition a
job after reclaim.
