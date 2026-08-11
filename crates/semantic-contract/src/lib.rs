//! Pure semantic contract types and deterministic compilation.
//!
//! This crate deliberately stops at a validated, canonical registry input. It
//! does not know about SQL, physical schemas, databases, agents, providers or
//! query execution.

mod compiler;
mod model;

pub use compiler::{
    CompiledModule, CompiledSemanticManifest, PlatformCapabilityVersion, SemanticCompilation,
    SemanticCompilationError, SemanticCompilationInput, SemanticCompiler,
    SEMANTIC_CONTRACT_SCHEMA_VERSION,
};
pub use model::{
    DatasetDefinition, DimensionDefinition, FieldDefinition, FilterPolicyDefinition,
    LineageDefinition, LineageSource, MeasureDefinition, MetricDefinition, ProjectionDefinition,
    RelationshipDefinition, SemanticContribution, SemanticObjectKind, SemanticReference,
    SemanticReferenceAccess, SemanticVersion, TimeDimensionDefinition,
};
