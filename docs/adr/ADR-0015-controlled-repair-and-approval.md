# ADR-0015: Controlled Repair and Approval

> Status: Accepted

## Decision

Repairs use typed allow-listed handlers and the protocol
`DryRun -> Approve -> Execute -> Verify`. Governance does not execute SQL or
write another context's private tables. Medium/high risk repairs require
creator/approver separation; destructive business operations are never
automatic.

## Consequences

Repair plans are explicit and reviewable, and safe automation is limited to
small deterministic operations with owner-specific contracts.
