# ADR-0014: Data Integrity Finding Lifecycle

> Status: Accepted

## Decision

Integrity rules are versioned, bounded-context-owned detectors. Findings are
durable, tenant-scoped, deduplicated by rule/version/resource/fingerprint, and
transition through an optimistic lifecycle. Transient dependency failure is a
scan failure/unknown result, never an automatic corruption finding.

## Consequences

Scan state and findings can be rebuilt or re-run without duplicate noise;
owner query ports remain the only cross-context read seam.
