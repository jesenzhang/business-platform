# ADR-0012: Document Candidate and Human Review

- Status: Accepted
- Date: 2026-08-03
- Decision owners: Document Intelligence / Document Management

## Context

Extraction output is uncertain and must not silently become formal business
data. Consumers need a stable, bounded schema and an explicit human decision.

## Decision

Persist a versioned `document.generic.v1` candidate with typed payload,
revision-bounded line evidence, provider/model metadata, and a bounded size.
Expose only redacted candidate and job views. An authorized reviewer submits
`accepted`, `edited`, or `rejected` with the candidate version; optimistic
concurrency permits one final review. Accepted/edited candidates complete the
processing job, while rejected candidates enter the explicit rejected terminal
state. Formal business writes remain in the owning application use case.

## Consequences

The candidate schema can evolve independently from Document metadata and review
conflicts are visible instead of overwritten. The MVP intentionally does not
apply candidate fields to Contract, Customer, or Finance data.

## Revision 1 clarification

Candidate persistence plus the `WaitingForReview` transition is one execution
unit of work. Review finalization validates tenant, candidate identity, and
version while holding the job/candidate/review records in one transaction;
replaying the same review is idempotent, while a different review conflicts.
