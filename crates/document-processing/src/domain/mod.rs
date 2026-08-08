mod artifact;
mod candidate;
mod error;
mod events;
mod job;
pub mod pipeline;

pub use artifact::{
    ArtifactKind, Evidence, ProcessingArtifact, ProcessingArtifactError, ProcessingRun,
    ProcessingRunStatus,
};
pub use candidate::{
    CandidateEvidence, CandidateEvidenceSource, CandidatePayload, CandidateReview,
    ExtractionCandidate, ReviewDecision,
};
pub use error::{ExtractionError, ProcessingDomainError, ReviewError};
pub use events::{ProcessingEvent, ProcessingEventEnvelope};
pub use job::{
    FixedPipeline, JobVersion, ProcessingFailureKind, ProcessingJob, ProcessingJobStatus,
    ProcessingStepKind, ProcessingStepStatus,
};
pub use pipeline::{
    extract_text_artifact, DeterministicLocalExtractor, DocumentFieldExtractor, ExtractionRequest,
    TextArtifact, TextCheckpoint,
};
