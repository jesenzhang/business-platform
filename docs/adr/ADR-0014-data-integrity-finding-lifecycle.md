# ADR-0014: Data Integrity Finding Lifecycle

> Status: Accepted

## Decision

Integrity rules are versioned, bounded-context-owned detectors. Findings are
durable, tenant-scoped, deduplicated by rule/version/resource/fingerprint, and
transition through an optimistic lifecycle. Transient dependency failure is a
scan failure/unknown result, never an automatic corruption finding.

Revision 1 chooses explicit recurrence reopening: when the same rule version
detects a previously `repaired` or `false_positive` identity, it reopens the
finding and records `reopened_at`, increments `reopen_count`, and preserves
`previous_resolution`. A changed rule version remains a separate identity.

## Consequences

Scan state and findings can be rebuilt or re-run without duplicate noise;
owner query ports remain the only cross-context read seam.
