# ADR-0016: Repair Ledger and Verification

> Status: Accepted

## Decision

Every repair step writes an append-only, redacted ledger entry with before and
after hashes, outcome, actor, finding, rule, and trace. The durable run uses
leases, fences, checkpoints, retries, cancellation, and resume; execution
revalidates before changing state and verifies the rule after the change.

Revision 1 requires owner-backed preview reads and a post-mutation owner rule
verification. A handler-reported success is insufficient: failed
verification moves the run/step/finding to manual review and cannot commit a
repaired finding. Repair lifecycle commands update the Run and Step through
compare-and-swap transactions.

## Consequences

Crash recovery and stale-worker rejection are testable without relying on
process memory. Ledger snapshots remain bounded and never include raw content
or secrets.
