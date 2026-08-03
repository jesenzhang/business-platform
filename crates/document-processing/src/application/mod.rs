//! Use-case orchestration lives here; adapters implement the ports in
//! [`crate::ports`]. The first MVP keeps these commands small and explicit.

mod review;
mod runner;

pub use review::{ReviewCandidateCommand, ReviewCandidateResult};
pub use runner::{FixedPipelineRunner, PipelineRunResult, ProcessingSource};
