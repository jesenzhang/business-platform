//! Durable document processing domain and application ports.
//!
//! This crate deliberately contains no database, HTTP, worker-runtime, or
//! provider dependency. A processing job is a durable business process with a
//! fixed pipeline; it is not a general workflow/DAG engine.

pub mod application;
pub mod domain;
pub mod ports;

pub use application::{FixedPipelineRunner, PipelineRunResult, ProcessingSource};

pub use domain::{
    extract_text_artifact, CandidateEvidence, CandidateEvidenceSource, CandidatePayload,
    CandidateReview, DeterministicLocalExtractor, DocumentFieldExtractor, ExtractionCandidate,
    ExtractionError, ExtractionRequest, FixedPipeline, JobVersion, ProcessingDomainError,
    ProcessingEvent, ProcessingEventEnvelope, ProcessingFailureKind, ProcessingJob,
    ProcessingJobStatus, ProcessingStepKind, ProcessingStepStatus, ReviewDecision, ReviewError,
    TextArtifact, TextCheckpoint,
};
pub use ports::{
    CandidateQuery, ClassifiedProcessingFailure, CompleteAiTaskCommand, ExecutionFence,
    FinalizeReviewCommand, FinalizeReviewResult, ProcessingExecutionUnitOfWork,
    ProcessingFailureDisposition, ProcessingJobClaimPort, ProcessingJobCommandPort,
    ProcessingJobQuery, ProcessingRepositoryError, ProcessingStepQuery, TextArtifactReference,
};
