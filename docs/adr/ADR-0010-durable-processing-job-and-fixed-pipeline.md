# ADR-0010: Durable Processing Job and Fixed Pipeline

- Status: Accepted
- Date: 2026-08-03
- Decision owners: Platform Foundation / Document Intelligence

## Context

Document processing needs restartable execution without making the Document
aggregate own OCR, extraction, or worker state. A general workflow engine would
expand the first slice beyond its measurable need.

## Decision

Create a Document Intelligence `ProcessingJob` aggregate backed by durable job,
step, candidate, review, and outbox records. Use the versioned fixed sequence
`ValidateSource → DetectType → ExtractText → ExtractFields →
ValidateCandidate → AwaitReview`. Keep job execution state separate from
Document Management business state. Use a deterministic local extractor for the
MVP and define provider ports without introducing a model gateway, DAG editor,
or generic scheduler.

## Consequences

Jobs survive process restart and can be claimed by workers, while the domain
remains small and reviewable. Adding a new step or provider requires an
explicit versioned decision and migration. Candidates remain non-authoritative
until a review command accepts or rejects them.

## Revision 1 clarification

The fixed pipeline is advanced one `current_step` at a time through an
adapter-owned execution unit of work. Text extraction persists a real,
tenant-scoped artifact before `ExtractFields` is delegated to the independent
AI task worker; the artifact reference is a checkpoint, never a public DTO.

## Rejected alternatives

- In-memory `tokio::spawn` as the source of truth;
- a configurable DAG/workflow designer;
- embedding processing fields in `DocumentMetadata`;
- treating a completed technical job as formal business completion.
