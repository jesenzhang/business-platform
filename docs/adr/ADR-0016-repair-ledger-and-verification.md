# ADR-0016: Repair Ledger and Verification

> Status: Accepted

## Decision

Every repair step writes an append-only, redacted ledger entry with before and
after hashes, outcome, actor, finding, rule, and trace. The durable run uses
leases, fences, checkpoints, retries, cancellation, and resume; execution
revalidates before changing state and verifies the rule after the change.

## Consequences

Crash recovery and stale-worker rejection are testable without relying on
process memory. Ledger snapshots remain bounded and never include raw content
or secrets.
