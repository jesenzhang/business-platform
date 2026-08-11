use business_module_contracts::{BusinessModuleId, DataClassification};
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_SEMANTIC_TOKEN_LENGTH: usize = 192;

/// A version attached to a semantic object.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SemanticVersion(String);

impl SemanticVersion {
    /// Creates a validated semantic version token.
    pub fn new(value: impl Into<String>) -> Result<Self, SemanticVersionError> {
        let value = value.into();
        if value.is_empty() {
            return Err(SemanticVersionError::Empty);
        }
        if value.len() > MAX_SEMANTIC_TOKEN_LENGTH {
            return Err(SemanticVersionError::TooLong);
        }
        if !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-+".contains(character))
        {
            return Err(SemanticVersionError::InvalidCharacters);
        }
        Ok(Self(value))
    }

    /// Returns the stable version representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for SemanticVersion {
    type Error = SemanticVersionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SemanticVersion> for String {
    fn from(value: SemanticVersion) -> Self {
        value.0
    }
}

/// Errors raised while constructing a semantic version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SemanticVersionError {
    /// The version was empty.
    #[error("semantic version must not be empty")]
    Empty,
    /// The version was too large for a bounded contract token.
    #[error("semantic version is too long")]
    TooLong,
    /// The version contains an unsupported character.
    #[error("semantic version contains an invalid character")]
    InvalidCharacters,
}

/// Kind of semantic object published by a business module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticObjectKind {
    /// A governed set of business fields and resources.
    Dataset,
    /// A rebuildable query-facing projection.
    Projection,
    /// A governed business field.
    Field,
    /// A relationship between semantic objects.
    Relationship,
    /// A numeric value and aggregation definition.
    Measure,
    /// A versioned business metric.
    Metric,
    /// A sliceable business dimension.
    Dimension,
    /// A time-aware business dimension.
    TimeDimension,
    /// A subject, tenant and field policy reference.
    FilterPolicy,
    /// A source and transformation chain.
    Lineage,
}

impl SemanticObjectKind {
    /// Returns the manifest descriptor spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dataset => "dataset",
            Self::Projection => "projection",
            Self::Field => "field",
            Self::Relationship => "relationship",
            Self::Measure => "measure",
            Self::Metric => "metric",
            Self::Dimension => "dimension",
            Self::TimeDimension => "time_dimension",
            Self::FilterPolicy => "filter_policy",
            Self::Lineage => "lineage",
        }
    }
}

/// Visibility mechanism for a semantic reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticReferenceAccess {
    /// A published semantic object.
    PublishedObject,
    /// A stable reference to an owner resource.
    ResourceReference,
    /// A public, rebuildable analytics projection.
    PublicProjection,
    /// A versioned historical reference and snapshot.
    ReferenceSnapshot,
    /// A private reference that cannot cross module ownership.
    Private,
}

/// A typed reference to another semantic object or public resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticReference {
    /// Local or namespaced semantic identifier before compilation.
    pub semantic_id: String,
    /// Boundary used to access the referenced object.
    pub access: SemanticReferenceAccess,
}

/// A field published by a business module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    /// Module-local field identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Platform-neutral semantic type name.
    pub semantic_type: String,
    /// Most restrictive classification of the field.
    pub classification: DataClassification,
    /// Optional lineage object.
    #[serde(default)]
    pub lineage_id: Option<SemanticReference>,
}

/// A governed dataset exposed by a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetDefinition {
    /// Module-local dataset identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Published fields in this dataset.
    #[serde(default)]
    pub field_ids: Vec<SemanticReference>,
    /// Dataset classification.
    pub classification: DataClassification,
    /// Optional lineage object.
    #[serde(default)]
    pub lineage_id: Option<SemanticReference>,
}

/// A rebuildable query-facing projection of a dataset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionDefinition {
    /// Module-local projection identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Dataset behind the projection.
    pub dataset_id: SemanticReference,
    /// Fields exposed by the projection.
    #[serde(default)]
    pub field_ids: Vec<SemanticReference>,
    /// Projection classification.
    pub classification: DataClassification,
    /// Optional lineage object.
    #[serde(default)]
    pub lineage_id: Option<SemanticReference>,
}

/// A relationship between two published semantic objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipDefinition {
    /// Module-local relationship identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Left semantic endpoint.
    pub from: SemanticReference,
    /// Right semantic endpoint.
    pub to: SemanticReference,
    /// Business cardinality name.
    pub relationship_kind: String,
    /// Whether the relationship intentionally crosses module ownership.
    pub cross_module: bool,
}

/// A numeric value and aggregation definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasureDefinition {
    /// Module-local measure identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Value type name, for example `decimal` or `count`.
    pub value_type: String,
    /// Aggregation name, for example `sum` or `count`.
    pub aggregation: String,
    /// Published source field.
    pub source_field: SemanticReference,
    /// Optional lineage object.
    #[serde(default)]
    pub lineage_id: Option<SemanticReference>,
}

/// A versioned business metric with an explicit owner and measure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricDefinition {
    /// Module-local metric identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Stable cross-module ownership key for the business metric.
    pub metric_key: String,
    /// Measure used to compute the metric.
    pub measure: SemanticReference,
    /// Dimensions allowed for slicing.
    #[serde(default)]
    pub dimensions: Vec<SemanticReference>,
    /// Optional time dimension.
    #[serde(default)]
    pub time_dimension: Option<SemanticReference>,
    /// Optional platform-neutral formula label; this is not SQL.
    #[serde(default)]
    pub formula: Option<String>,
    /// Optional lineage object.
    #[serde(default)]
    pub lineage_id: Option<SemanticReference>,
}

/// A sliceable business dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DimensionDefinition {
    /// Module-local dimension identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Published source field.
    pub source_field: SemanticReference,
    /// Dimension classification.
    pub classification: DataClassification,
    /// Optional lineage object.
    #[serde(default)]
    pub lineage_id: Option<SemanticReference>,
}

/// A time dimension with explicit granularity and timezone semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeDimensionDefinition {
    /// Module-local time dimension identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Published source field.
    pub source_field: SemanticReference,
    /// Supported time grain.
    pub granularity: String,
    /// IANA timezone or explicit UTC policy name.
    pub timezone: String,
    /// Optional lineage object.
    #[serde(default)]
    pub lineage_id: Option<SemanticReference>,
}

/// A policy reference controlling subject, tenant, row/column and export rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterPolicyDefinition {
    /// Module-local policy identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Policy identifier owned by the policy context.
    pub policy_id: String,
    /// Highest classification the policy protects.
    pub classification: DataClassification,
}

/// A source participating in lineage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageSource {
    /// Published semantic/resource reference.
    pub reference: SemanticReference,
    /// Optional source revision or event version.
    #[serde(default)]
    pub source_version: Option<String>,
}

/// A versioned lineage chain for a semantic object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageDefinition {
    /// Module-local lineage identifier.
    pub id: String,
    /// Definition version.
    pub version: SemanticVersion,
    /// Owning module.
    pub owner_module: BusinessModuleId,
    /// Published source references.
    #[serde(default)]
    pub sources: Vec<LineageSource>,
    /// Human-readable semantic transformation label; this is not SQL.
    #[serde(default)]
    pub transformation: Option<String>,
    /// Freshness objective or observed freshness label.
    #[serde(default)]
    pub freshness: Option<String>,
}

/// All semantic definitions contributed by one module version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticContribution {
    /// Owning module.
    pub module_id: BusinessModuleId,
    /// Module version that authored the contribution.
    pub module_version: business_module_contracts::BusinessModuleVersion,
    /// Dataset definitions.
    #[serde(default)]
    pub datasets: Vec<DatasetDefinition>,
    /// Projection definitions.
    #[serde(default)]
    pub projections: Vec<ProjectionDefinition>,
    /// Field definitions.
    #[serde(default)]
    pub fields: Vec<FieldDefinition>,
    /// Relationship definitions.
    #[serde(default)]
    pub relationships: Vec<RelationshipDefinition>,
    /// Measure definitions.
    #[serde(default)]
    pub measures: Vec<MeasureDefinition>,
    /// Metric definitions.
    #[serde(default)]
    pub metrics: Vec<MetricDefinition>,
    /// Dimension definitions.
    #[serde(default)]
    pub dimensions: Vec<DimensionDefinition>,
    /// Time dimension definitions.
    #[serde(default)]
    pub time_dimensions: Vec<TimeDimensionDefinition>,
    /// Filter policy definitions.
    #[serde(default)]
    pub filter_policies: Vec<FilterPolicyDefinition>,
    /// Lineage definitions.
    #[serde(default)]
    pub lineages: Vec<LineageDefinition>,
}
