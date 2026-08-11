use std::collections::{BTreeMap, BTreeSet, HashMap};

use business_module_contracts::{BusinessModuleId, BusinessModuleManifest, ModuleDependency};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::model::{
    DatasetDefinition, DimensionDefinition, FieldDefinition, FilterPolicyDefinition,
    LineageDefinition, MeasureDefinition, MetricDefinition, ProjectionDefinition,
    RelationshipDefinition, SemanticContribution, SemanticObjectKind, SemanticReference,
    SemanticReferenceAccess, SemanticVersion, TimeDimensionDefinition,
};

/// Version of the platform-native compiled semantic contract.
pub const SEMANTIC_CONTRACT_SCHEMA_VERSION: &str = "semantic-contract.v1";

/// A version advertised by the platform capability registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformCapabilityVersion {
    /// Stable platform capability ID.
    pub capability_id: String,
    /// Installed capability version.
    pub version: String,
}

/// Inputs required for a pure semantic compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCompilationInput {
    /// Business module manifests to compile.
    pub modules: Vec<BusinessModuleManifest>,
    /// One semantic contribution per module version.
    pub contributions: Vec<SemanticContribution>,
    /// Platform capabilities available to the compiler.
    pub platform_capabilities: Vec<PlatformCapabilityVersion>,
}

/// A module entry in the compiled registry input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledModule {
    /// Module ID.
    pub module_id: BusinessModuleId,
    /// Module version.
    pub module_version: business_module_contracts::BusinessModuleVersion,
    /// Owner-scoped migration namespace.
    pub migration_namespace: String,
    /// Fully namespaced semantic IDs published by this module.
    pub semantic_ids: Vec<String>,
}

/// Canonical, rebuildable semantic registry input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledSemanticManifest {
    /// Compiled contract schema version.
    pub schema_version: String,
    /// Stable module ordering.
    pub modules: Vec<CompiledModule>,
    /// Stable semantic contribution ordering.
    pub contributions: Vec<SemanticContribution>,
}

/// Result of compiling and hashing semantic contributions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCompilation {
    /// Canonical compiled manifest.
    pub manifest: CompiledSemanticManifest,
    /// Compact canonical JSON bytes.
    pub canonical_json: Vec<u8>,
    /// Lower-case SHA-256 of the canonical JSON.
    pub sha256: String,
}

/// Stateless semantic compiler.
#[derive(Debug, Clone, Copy, Default)]
pub struct SemanticCompiler;

impl SemanticCompiler {
    /// Validates, namespaces, sorts and hashes semantic contributions.
    pub fn compile(
        input: SemanticCompilationInput,
    ) -> Result<SemanticCompilation, SemanticCompilationError> {
        let modules = collect_modules(input.modules)?;
        for module in modules.values() {
            module
                .validate()
                .map_err(|error| SemanticCompilationError::InvalidModule {
                    module_id: module.module_id.to_string(),
                    reason: error.to_string(),
                })?;
        }

        let platform_capabilities = collect_platform_capabilities(input.platform_capabilities)?;
        validate_platform_requirements(&modules, &platform_capabilities)?;
        validate_module_dependencies(&modules)?;

        let contributions = collect_contributions(input.contributions, &modules)?;
        let object_infos = collect_object_infos(&contributions, &modules)?;
        validate_contribution_declarations(&modules, &object_infos)?;
        let known_ids = object_infos.keys().cloned().collect::<BTreeSet<_>>();
        let module_ids = modules.keys().cloned().collect::<BTreeSet<_>>();
        let normalized_contributions = contributions
            .into_values()
            .map(|contribution| normalize_contribution(contribution, &module_ids, &known_ids))
            .collect::<Result<Vec<_>, _>>()?;

        let compiled_modules = build_compiled_modules(&modules, &normalized_contributions)?;
        let manifest = CompiledSemanticManifest {
            schema_version: SEMANTIC_CONTRACT_SCHEMA_VERSION.to_owned(),
            modules: compiled_modules,
            contributions: normalized_contributions,
        };
        let canonical_json = serde_json::to_vec(&manifest).map_err(|error| {
            SemanticCompilationError::Serialization {
                reason: error.to_string(),
            }
        })?;
        let digest = Sha256::digest(&canonical_json);
        let sha256 = format!("{digest:x}");
        Ok(SemanticCompilation {
            manifest,
            canonical_json,
            sha256,
        })
    }
}

#[derive(Debug, Clone)]
struct ObjectInfo {
    kind: SemanticObjectKind,
    version: SemanticVersion,
    owner_module: BusinessModuleId,
    metric_key: Option<String>,
}

fn collect_modules(
    modules: Vec<BusinessModuleManifest>,
) -> Result<BTreeMap<BusinessModuleId, BusinessModuleManifest>, SemanticCompilationError> {
    let mut indexed = BTreeMap::new();
    for module in modules {
        let module_id = module.module_id.clone();
        if indexed.insert(module_id.clone(), module).is_some() {
            return Err(SemanticCompilationError::DuplicateModule {
                module_id: module_id.to_string(),
            });
        }
    }
    Ok(indexed)
}

fn collect_platform_capabilities(
    capabilities: Vec<PlatformCapabilityVersion>,
) -> Result<BTreeMap<String, String>, SemanticCompilationError> {
    let mut indexed = BTreeMap::new();
    for capability in capabilities {
        if capability.capability_id.trim().is_empty() || capability.version.trim().is_empty() {
            return Err(SemanticCompilationError::InvalidPlatformCapability {
                capability_id: capability.capability_id,
            });
        }
        if indexed
            .insert(capability.capability_id.clone(), capability.version)
            .is_some()
        {
            return Err(SemanticCompilationError::DuplicatePlatformCapability {
                capability_id: capability.capability_id,
            });
        }
    }
    Ok(indexed)
}

fn validate_platform_requirements(
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
    capabilities: &BTreeMap<String, String>,
) -> Result<(), SemanticCompilationError> {
    for module in modules.values() {
        for requirement in module
            .required_platform_capabilities
            .iter()
            .chain(module.optional_platform_capabilities.iter())
        {
            if let Some(actual) = capabilities.get(&requirement.capability_id) {
                if !version_satisfies(&requirement.version_requirement, actual) {
                    return Err(SemanticCompilationError::IncompatiblePlatformCapability {
                        module_id: module.module_id.to_string(),
                        capability_id: requirement.capability_id.clone(),
                        required: requirement.version_requirement.clone(),
                        actual: actual.clone(),
                    });
                }
            } else if module
                .required_platform_capabilities
                .iter()
                .any(|required| required.capability_id == requirement.capability_id)
            {
                return Err(
                    SemanticCompilationError::MissingRequiredPlatformCapability {
                        module_id: module.module_id.to_string(),
                        capability_id: requirement.capability_id.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

fn validate_module_dependencies(
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
) -> Result<(), SemanticCompilationError> {
    let mut states = HashMap::new();
    let mut stack = Vec::new();
    for module_id in modules.keys() {
        visit_module(module_id, modules, &mut states, &mut stack)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Visited,
}

fn visit_module(
    module_id: &BusinessModuleId,
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
    states: &mut HashMap<BusinessModuleId, VisitState>,
    stack: &mut Vec<BusinessModuleId>,
) -> Result<(), SemanticCompilationError> {
    match states.get(module_id) {
        Some(VisitState::Visited) => return Ok(()),
        Some(VisitState::Visiting) => {
            let cycle_start = stack.iter().position(|item| item == module_id).unwrap_or(0);
            let cycle = stack[cycle_start..]
                .iter()
                .map(ToString::to_string)
                .chain(std::iter::once(module_id.to_string()))
                .collect();
            return Err(SemanticCompilationError::CyclicModuleDependency { cycle });
        }
        None => {}
    }

    states.insert(module_id.clone(), VisitState::Visiting);
    stack.push(module_id.clone());
    let module = modules.get(module_id).ok_or_else(|| {
        SemanticCompilationError::MissingModuleDependency {
            module_id: module_id.to_string(),
            dependency_id: module_id.to_string(),
        }
    })?;
    for dependency in &module.dependencies {
        validate_dependency(module, dependency, modules)?;
        visit_module(&dependency.module_id, modules, states, stack)?;
    }
    stack.pop();
    states.insert(module_id.clone(), VisitState::Visited);
    Ok(())
}

fn validate_dependency(
    module: &BusinessModuleManifest,
    dependency: &ModuleDependency,
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
) -> Result<(), SemanticCompilationError> {
    if is_platform_identifier(dependency.module_id.as_str()) {
        return Err(SemanticCompilationError::IllegalPlatformDependency {
            module_id: module.module_id.to_string(),
            dependency_id: dependency.module_id.to_string(),
        });
    }
    let target = modules.get(&dependency.module_id).ok_or_else(|| {
        SemanticCompilationError::MissingModuleDependency {
            module_id: module.module_id.to_string(),
            dependency_id: dependency.module_id.to_string(),
        }
    })?;
    if !version_satisfies(
        &dependency.version_requirement,
        target.module_version.as_str(),
    ) {
        return Err(SemanticCompilationError::IncompatibleModuleVersion {
            module_id: dependency.module_id.to_string(),
            required: dependency.version_requirement.clone(),
            actual: target.module_version.to_string(),
        });
    }
    Ok(())
}

fn collect_contributions(
    contributions: Vec<SemanticContribution>,
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
) -> Result<BTreeMap<BusinessModuleId, SemanticContribution>, SemanticCompilationError> {
    let mut indexed = BTreeMap::new();
    for contribution in contributions {
        let module_id = contribution.module_id.clone();
        let module = modules.get(&module_id).ok_or_else(|| {
            SemanticCompilationError::MissingModuleForContribution {
                module_id: module_id.to_string(),
            }
        })?;
        if module.module_version != contribution.module_version {
            return Err(SemanticCompilationError::ContributionVersionMismatch {
                module_id: module_id.to_string(),
                manifest_version: module.module_version.to_string(),
                contribution_version: contribution.module_version.to_string(),
            });
        }
        if indexed.insert(module_id.clone(), contribution).is_some() {
            return Err(SemanticCompilationError::DuplicateSemanticContribution {
                module_id: module_id.to_string(),
            });
        }
    }
    Ok(indexed)
}

fn collect_object_infos(
    contributions: &BTreeMap<BusinessModuleId, SemanticContribution>,
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
) -> Result<BTreeMap<String, ObjectInfo>, SemanticCompilationError> {
    macro_rules! insert_group {
        ($objects:expr, $contribution:expr, $definitions:expr, $kind:expr) => {
            for definition in $definitions {
                insert_object(
                    $objects,
                    &$contribution.module_id,
                    &definition.id,
                    &definition.version,
                    &definition.owner_module,
                    $kind,
                    None,
                )?;
            }
        };
    }

    let mut objects = BTreeMap::new();
    let mut metric_owners = BTreeMap::<String, (BusinessModuleId, String)>::new();
    for contribution in contributions.values() {
        ensure_contribution_owner(contribution, modules)?;
        insert_group!(
            &mut objects,
            contribution,
            &contribution.datasets,
            SemanticObjectKind::Dataset
        );
        insert_group!(
            &mut objects,
            contribution,
            &contribution.projections,
            SemanticObjectKind::Projection
        );
        insert_group!(
            &mut objects,
            contribution,
            &contribution.fields,
            SemanticObjectKind::Field
        );
        insert_group!(
            &mut objects,
            contribution,
            &contribution.relationships,
            SemanticObjectKind::Relationship
        );
        insert_group!(
            &mut objects,
            contribution,
            &contribution.measures,
            SemanticObjectKind::Measure
        );
        for definition in &contribution.metrics {
            let metric_id = qualified_id(&contribution.module_id, &definition.id)?;
            if let Some((first_owner, first_id)) = metric_owners.get(&definition.metric_key) {
                return Err(SemanticCompilationError::MetricOwnershipConflict {
                    metric_key: definition.metric_key.clone(),
                    first_owner: first_owner.to_string(),
                    first_metric_id: first_id.clone(),
                    second_owner: definition.owner_module.to_string(),
                    second_metric_id: metric_id,
                });
            }
            metric_owners.insert(
                definition.metric_key.clone(),
                (definition.owner_module.clone(), metric_id.clone()),
            );
            insert_object(
                &mut objects,
                &contribution.module_id,
                &definition.id,
                &definition.version,
                &definition.owner_module,
                SemanticObjectKind::Metric,
                Some(definition.metric_key.clone()),
            )?;
        }
        insert_group!(
            &mut objects,
            contribution,
            &contribution.dimensions,
            SemanticObjectKind::Dimension
        );
        insert_group!(
            &mut objects,
            contribution,
            &contribution.time_dimensions,
            SemanticObjectKind::TimeDimension
        );
        insert_group!(
            &mut objects,
            contribution,
            &contribution.filter_policies,
            SemanticObjectKind::FilterPolicy
        );
        insert_group!(
            &mut objects,
            contribution,
            &contribution.lineages,
            SemanticObjectKind::Lineage
        );
    }
    Ok(objects)
}

fn ensure_contribution_owner(
    contribution: &SemanticContribution,
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
) -> Result<(), SemanticCompilationError> {
    let module_id = &contribution.module_id;
    let definitions = definition_owners(contribution);
    for owner in definitions {
        if &owner != module_id {
            return Err(SemanticCompilationError::SemanticOwnerMismatch {
                module_id: module_id.to_string(),
                owner_module: owner.to_string(),
            });
        }
    }
    if !modules.contains_key(module_id) {
        return Err(SemanticCompilationError::MissingModuleForContribution {
            module_id: module_id.to_string(),
        });
    }
    Ok(())
}

fn definition_owners(contribution: &SemanticContribution) -> BTreeSet<BusinessModuleId> {
    let mut owners = BTreeSet::new();
    owners.extend(
        contribution
            .datasets
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .projections
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .fields
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .relationships
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .measures
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .metrics
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .dimensions
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .time_dimensions
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .filter_policies
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners.extend(
        contribution
            .lineages
            .iter()
            .map(|definition| definition.owner_module.clone()),
    );
    owners
}

fn insert_object(
    objects: &mut BTreeMap<String, ObjectInfo>,
    contribution_module: &BusinessModuleId,
    id: &str,
    version: &SemanticVersion,
    owner_module: &BusinessModuleId,
    kind: SemanticObjectKind,
    metric_key: Option<String>,
) -> Result<(), SemanticCompilationError> {
    if owner_module != contribution_module {
        return Err(SemanticCompilationError::SemanticOwnerMismatch {
            module_id: contribution_module.to_string(),
            owner_module: owner_module.to_string(),
        });
    }
    let qualified = qualified_id(contribution_module, id)?;
    if objects
        .insert(
            qualified.clone(),
            ObjectInfo {
                kind,
                version: version.clone(),
                owner_module: owner_module.clone(),
                metric_key,
            },
        )
        .is_some()
    {
        return Err(SemanticCompilationError::DuplicateSemanticId {
            semantic_id: qualified,
        });
    }
    Ok(())
}

fn validate_contribution_declarations(
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
    objects: &BTreeMap<String, ObjectInfo>,
) -> Result<(), SemanticCompilationError> {
    let mut declared = BTreeMap::<String, (String, String)>::new();
    for module in modules.values() {
        for descriptor in &module.semantic_contributions {
            let semantic_id = qualified_id(&module.module_id, &descriptor.semantic_id)?;
            if declared
                .insert(
                    semantic_id.clone(),
                    (descriptor.semantic_kind.clone(), descriptor.version.clone()),
                )
                .is_some()
            {
                return Err(SemanticCompilationError::DuplicateSemanticDescriptor { semantic_id });
            }
        }
    }

    for (semantic_id, object) in objects {
        let Some((declared_kind, declared_version)) = declared.get(semantic_id) else {
            return Err(SemanticCompilationError::UndeclaredContribution {
                semantic_id: semantic_id.clone(),
                module_id: object.owner_module.to_string(),
            });
        };
        if declared_kind != object.kind.as_str()
            || declared_version != object.version.as_str()
            || object.metric_key.as_ref().is_some_and(String::is_empty)
        {
            return Err(SemanticCompilationError::ContributionDescriptorMismatch {
                semantic_id: semantic_id.clone(),
                declared_kind: declared_kind.clone(),
                actual_kind: object.kind.as_str().to_owned(),
                declared_version: declared_version.clone(),
                actual_version: object.version.to_string(),
            });
        }
    }
    for semantic_id in declared.keys() {
        if !objects.contains_key(semantic_id) {
            let module_id = semantic_id
                .split('.')
                .next()
                .map_or_else(|| "unknown".to_owned(), ToOwned::to_owned);
            return Err(SemanticCompilationError::DeclaredContributionMissing {
                semantic_id: semantic_id.clone(),
                module_id,
            });
        }
    }
    Ok(())
}

fn normalize_contribution(
    mut contribution: SemanticContribution,
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<SemanticContribution, SemanticCompilationError> {
    let owner = contribution.module_id.clone();
    normalize_datasets(&owner, &mut contribution.datasets, module_ids, known_ids)?;
    normalize_projections(&owner, &mut contribution.projections, module_ids, known_ids)?;
    normalize_fields(&owner, &mut contribution.fields, module_ids, known_ids)?;
    normalize_relationships(
        &owner,
        &mut contribution.relationships,
        module_ids,
        known_ids,
    )?;
    normalize_measures(&owner, &mut contribution.measures, module_ids, known_ids)?;
    normalize_metrics(&owner, &mut contribution.metrics, module_ids, known_ids)?;
    normalize_dimensions(&owner, &mut contribution.dimensions, module_ids, known_ids)?;
    normalize_time_dimensions(
        &owner,
        &mut contribution.time_dimensions,
        module_ids,
        known_ids,
    )?;
    normalize_filter_policies(&owner, &mut contribution.filter_policies)?;
    normalize_lineages(&owner, &mut contribution.lineages, module_ids, known_ids)?;
    sort_contribution(&mut contribution);
    Ok(contribution)
}

fn normalize_datasets(
    owner: &BusinessModuleId,
    definitions: &mut [DatasetDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        normalize_references(
            owner,
            &mut definition.field_ids,
            module_ids,
            known_ids,
            None,
        )?;
        sort_references(&mut definition.field_ids);
        normalize_optional_reference(owner, &mut definition.lineage_id, module_ids, known_ids)?;
    }
    Ok(())
}

fn normalize_projections(
    owner: &BusinessModuleId,
    definitions: &mut [ProjectionDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        normalize_reference(
            owner,
            &mut definition.dataset_id,
            module_ids,
            known_ids,
            None,
        )?;
        normalize_references(
            owner,
            &mut definition.field_ids,
            module_ids,
            known_ids,
            None,
        )?;
        sort_references(&mut definition.field_ids);
        normalize_optional_reference(owner, &mut definition.lineage_id, module_ids, known_ids)?;
    }
    Ok(())
}

fn normalize_fields(
    owner: &BusinessModuleId,
    definitions: &mut [FieldDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        normalize_optional_reference(owner, &mut definition.lineage_id, module_ids, known_ids)?;
    }
    Ok(())
}

fn normalize_relationships(
    owner: &BusinessModuleId,
    definitions: &mut [RelationshipDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        let relationship_id = qualified_id(owner, &definition.id)?;
        definition.id.clone_from(&relationship_id);
        normalize_reference(
            owner,
            &mut definition.from,
            module_ids,
            known_ids,
            Some(&relationship_id),
        )?;
        normalize_reference(
            owner,
            &mut definition.to,
            module_ids,
            known_ids,
            Some(&relationship_id),
        )?;
        let is_cross_module = reference_owner(&definition.from) != reference_owner(&definition.to);
        if is_cross_module != definition.cross_module {
            return Err(
                SemanticCompilationError::RelationshipCrossModuleFlagMismatch {
                    relationship_id,
                    expected_cross_module: is_cross_module,
                },
            );
        }
    }
    Ok(())
}

fn normalize_measures(
    owner: &BusinessModuleId,
    definitions: &mut [MeasureDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        normalize_reference(
            owner,
            &mut definition.source_field,
            module_ids,
            known_ids,
            None,
        )?;
        normalize_optional_reference(owner, &mut definition.lineage_id, module_ids, known_ids)?;
    }
    Ok(())
}

fn normalize_metrics(
    owner: &BusinessModuleId,
    definitions: &mut [MetricDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        if definition.metric_key.trim().is_empty() {
            return Err(SemanticCompilationError::EmptySemanticField {
                semantic_id: definition.id.clone(),
                field: "metric_key".to_owned(),
            });
        }
        reject_sql_like_text(definition.formula.as_deref(), &definition.id)?;
        normalize_reference(owner, &mut definition.measure, module_ids, known_ids, None)?;
        normalize_references(
            owner,
            &mut definition.dimensions,
            module_ids,
            known_ids,
            None,
        )?;
        sort_references(&mut definition.dimensions);
        normalize_optional_reference(owner, &mut definition.time_dimension, module_ids, known_ids)?;
        normalize_optional_reference(owner, &mut definition.lineage_id, module_ids, known_ids)?;
    }
    Ok(())
}

fn normalize_dimensions(
    owner: &BusinessModuleId,
    definitions: &mut [DimensionDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        normalize_reference(
            owner,
            &mut definition.source_field,
            module_ids,
            known_ids,
            None,
        )?;
        normalize_optional_reference(owner, &mut definition.lineage_id, module_ids, known_ids)?;
    }
    Ok(())
}

fn normalize_time_dimensions(
    owner: &BusinessModuleId,
    definitions: &mut [TimeDimensionDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        normalize_reference(
            owner,
            &mut definition.source_field,
            module_ids,
            known_ids,
            None,
        )?;
        normalize_optional_reference(owner, &mut definition.lineage_id, module_ids, known_ids)?;
    }
    Ok(())
}

fn normalize_filter_policies(
    owner: &BusinessModuleId,
    definitions: &mut [FilterPolicyDefinition],
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        if definition.policy_id.trim().is_empty() {
            return Err(SemanticCompilationError::EmptySemanticField {
                semantic_id: definition.id.clone(),
                field: "policy_id".to_owned(),
            });
        }
    }
    Ok(())
}

fn normalize_lineages(
    owner: &BusinessModuleId,
    definitions: &mut [LineageDefinition],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    for definition in definitions {
        definition.id = qualified_id(owner, &definition.id)?;
        reject_sql_like_text(definition.transformation.as_deref(), &definition.id)?;
        for source in &mut definition.sources {
            normalize_reference(owner, &mut source.reference, module_ids, known_ids, None)?;
        }
        definition
            .sources
            .sort_by(|left, right| left.reference.semantic_id.cmp(&right.reference.semantic_id));
    }
    Ok(())
}

fn sort_references(references: &mut [SemanticReference]) {
    references.sort_by(|left, right| {
        left.semantic_id
            .cmp(&right.semantic_id)
            .then(left.access.cmp(&right.access))
    });
}

fn normalize_optional_reference(
    owner: &BusinessModuleId,
    reference: &mut Option<SemanticReference>,
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
) -> Result<(), SemanticCompilationError> {
    if let Some(reference) = reference {
        normalize_reference(owner, reference, module_ids, known_ids, None)?;
    }
    Ok(())
}

fn normalize_references(
    owner: &BusinessModuleId,
    references: &mut [SemanticReference],
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
    relationship_id: Option<&str>,
) -> Result<(), SemanticCompilationError> {
    for reference in references {
        normalize_reference(owner, reference, module_ids, known_ids, relationship_id)?;
    }
    Ok(())
}

fn normalize_reference(
    owner: &BusinessModuleId,
    reference: &mut SemanticReference,
    module_ids: &BTreeSet<BusinessModuleId>,
    known_ids: &BTreeSet<String>,
    relationship_id: Option<&str>,
) -> Result<(), SemanticCompilationError> {
    let semantic_id = resolve_reference_id(owner, &reference.semantic_id, module_ids)?;
    if !known_ids.contains(&semantic_id) {
        return if let Some(relationship_id) = relationship_id {
            Err(SemanticCompilationError::UnknownRelationshipEndpoint {
                relationship_id: relationship_id.to_owned(),
                endpoint: semantic_id,
            })
        } else {
            Err(SemanticCompilationError::UnknownSemanticReference {
                owner_module: owner.to_string(),
                reference: semantic_id,
            })
        };
    }
    let target_owner = reference_owner_from_id(&semantic_id);
    if target_owner != owner.as_str() && reference.access == SemanticReferenceAccess::Private {
        return Err(SemanticCompilationError::CrossModulePrivateReference {
            owner_module: owner.to_string(),
            reference: semantic_id,
        });
    }
    reference.semantic_id = semantic_id;
    Ok(())
}

fn resolve_reference_id(
    owner: &BusinessModuleId,
    reference: &str,
    module_ids: &BTreeSet<BusinessModuleId>,
) -> Result<String, SemanticCompilationError> {
    validate_semantic_token(reference)?;
    let first_segment = reference.split('.').next().unwrap_or_default();
    if module_ids
        .iter()
        .any(|module_id| module_id.as_str() == first_segment)
    {
        Ok(reference.to_owned())
    } else {
        qualified_id(owner, reference)
    }
}

fn qualified_id(owner: &BusinessModuleId, id: &str) -> Result<String, SemanticCompilationError> {
    validate_semantic_token(id)?;
    let prefix = format!("{}.", owner.as_str());
    if id.starts_with(&prefix) {
        Ok(id.to_owned())
    } else {
        Ok(format!("{prefix}{id}"))
    }
}

fn validate_semantic_token(value: &str) -> Result<(), SemanticCompilationError> {
    if value.is_empty()
        || value.len() > MAX_SEMANTIC_TOKEN_LENGTH
        || value.starts_with('.')
        || value.starts_with('-')
        || value.starts_with('_')
        || value.ends_with('.')
        || value.ends_with('-')
        || value.ends_with('_')
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || ".-_".contains(character)
        })
    {
        return Err(SemanticCompilationError::InvalidSemanticId {
            semantic_id: value.to_owned(),
        });
    }
    Ok(())
}

fn reference_owner(reference: &SemanticReference) -> &str {
    reference_owner_from_id(&reference.semantic_id)
}

fn reference_owner_from_id(reference: &str) -> &str {
    reference.split('.').next().unwrap_or(reference)
}

fn sort_contribution(contribution: &mut SemanticContribution) {
    contribution
        .datasets
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .projections
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .fields
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .relationships
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .measures
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .metrics
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .dimensions
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .time_dimensions
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .filter_policies
        .sort_by(|left, right| left.id.cmp(&right.id));
    contribution
        .lineages
        .sort_by(|left, right| left.id.cmp(&right.id));
}

fn build_compiled_modules(
    modules: &BTreeMap<BusinessModuleId, BusinessModuleManifest>,
    contributions: &[SemanticContribution],
) -> Result<Vec<CompiledModule>, SemanticCompilationError> {
    let mut ids_by_module = BTreeMap::<BusinessModuleId, Vec<String>>::new();
    for contribution in contributions {
        let ids = ids_by_module
            .entry(contribution.module_id.clone())
            .or_default();
        ids.extend(all_definition_ids(contribution));
    }
    for ids in ids_by_module.values_mut() {
        ids.sort();
    }
    modules
        .values()
        .map(|module| {
            Ok(CompiledModule {
                module_id: module.module_id.clone(),
                module_version: module.module_version.clone(),
                migration_namespace: module.migration_namespace.clone(),
                semantic_ids: ids_by_module
                    .get(&module.module_id)
                    .cloned()
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn all_definition_ids(contribution: &SemanticContribution) -> Vec<String> {
    contribution
        .datasets
        .iter()
        .map(|definition| definition.id.clone())
        .chain(
            contribution
                .projections
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .fields
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .relationships
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .measures
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .metrics
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .dimensions
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .time_dimensions
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .filter_policies
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .chain(
            contribution
                .lineages
                .iter()
                .map(|definition| definition.id.clone()),
        )
        .collect()
}

fn reject_sql_like_text(
    value: Option<&str>,
    semantic_id: &str,
) -> Result<(), SemanticCompilationError> {
    let Some(value) = value else {
        return Ok(());
    };
    let normalized = value.to_ascii_lowercase();
    let forbidden = [
        "select ", " from ", " join ", "insert ", "update ", "delete ",
    ];
    if forbidden.iter().any(|token| normalized.contains(token)) {
        return Err(SemanticCompilationError::ForbiddenPhysicalExpression {
            semantic_id: semantic_id.to_owned(),
        });
    }
    Ok(())
}

fn is_platform_identifier(identifier: &str) -> bool {
    identifier == "platform"
        || identifier.starts_with("platform-")
        || identifier.starts_with("platform.")
}

fn version_satisfies(requirement: &str, actual: &str) -> bool {
    if requirement == "*" || requirement.is_empty() {
        return true;
    }
    let (operator, required) = requirement
        .strip_prefix('^')
        .map_or((None, requirement), |value| (Some('^'), value));
    let (operator, required) = required
        .strip_prefix('~')
        .map_or((operator, required), |value| (Some('~'), value));
    if operator.is_none() {
        return required == actual;
    }
    let Some(required_parts) = parse_numeric_version(required) else {
        return false;
    };
    let Some(actual_parts) = parse_numeric_version(actual) else {
        return false;
    };
    if actual_parts < required_parts {
        return false;
    }
    match operator {
        Some('^') => actual_parts[0] == required_parts[0],
        Some('~') => actual_parts[0] == required_parts[0] && actual_parts[1] == required_parts[1],
        _ => false,
    }
}

fn parse_numeric_version(value: &str) -> Option<[u64; 3]> {
    let value = value.split(['-', '+']).next()?;
    let mut parts = [0_u64; 3];
    for (count, component) in value.split('.').enumerate() {
        if count == 3 || component.is_empty() {
            return None;
        }
        parts[count] = component.parse().ok()?;
    }
    Some(parts)
}

/// Stable compiler failures. Variants are intentionally domain-facing and do
/// not expose database/provider error types.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SemanticCompilationError {
    /// A module manifest failed local validation.
    #[error("module '{module_id}' is invalid: {reason}")]
    InvalidModule { module_id: String, reason: String },
    /// A module ID occurred more than once.
    #[error("duplicate module '{module_id}'")]
    DuplicateModule { module_id: String },
    /// A platform capability declaration is invalid.
    #[error("invalid platform capability '{capability_id}'")]
    InvalidPlatformCapability { capability_id: String },
    /// A platform capability occurred more than once in the registry.
    #[error("duplicate platform capability '{capability_id}'")]
    DuplicatePlatformCapability { capability_id: String },
    /// A required platform capability is unavailable.
    #[error("module '{module_id}' requires missing platform capability '{capability_id}'")]
    MissingRequiredPlatformCapability {
        module_id: String,
        capability_id: String,
    },
    /// A platform capability version is incompatible.
    #[error("module '{module_id}' requires platform capability '{capability_id}' version '{required}', actual '{actual}'")]
    IncompatiblePlatformCapability {
        module_id: String,
        capability_id: String,
        required: String,
        actual: String,
    },
    /// A module depends on a platform identifier through the business module edge.
    #[error("module '{module_id}' has illegal platform dependency '{dependency_id}'")]
    IllegalPlatformDependency {
        module_id: String,
        dependency_id: String,
    },
    /// A declared module dependency is not installed.
    #[error("module '{module_id}' depends on missing module '{dependency_id}'")]
    MissingModuleDependency {
        module_id: String,
        dependency_id: String,
    },
    /// A module dependency version is incompatible.
    #[error("module dependency '{module_id}' requires version '{required}', actual '{actual}'")]
    IncompatibleModuleVersion {
        module_id: String,
        required: String,
        actual: String,
    },
    /// Module dependencies contain a cycle.
    #[error("cyclic module dependency: {cycle:?}")]
    CyclicModuleDependency { cycle: Vec<String> },
    /// A contribution references a module not present in the input.
    #[error("semantic contribution belongs to missing module '{module_id}'")]
    MissingModuleForContribution { module_id: String },
    /// A module has more than one contribution for the same version.
    #[error("duplicate semantic contribution for module '{module_id}'")]
    DuplicateSemanticContribution { module_id: String },
    /// Contribution and manifest versions disagree.
    #[error("module '{module_id}' manifest version '{manifest_version}' does not match contribution version '{contribution_version}'")]
    ContributionVersionMismatch {
        module_id: String,
        manifest_version: String,
        contribution_version: String,
    },
    /// A definition owner differs from its contribution owner.
    #[error("semantic contribution '{module_id}' contains definition owned by '{owner_module}'")]
    SemanticOwnerMismatch {
        module_id: String,
        owner_module: String,
    },
    /// Two definitions use one global semantic ID.
    #[error("duplicate semantic ID '{semantic_id}'")]
    DuplicateSemanticId { semantic_id: String },
    /// Two descriptors use one global semantic ID.
    #[error("duplicate semantic descriptor '{semantic_id}'")]
    DuplicateSemanticDescriptor { semantic_id: String },
    /// A semantic ID was invalid.
    #[error("invalid semantic ID '{semantic_id}'")]
    InvalidSemanticId { semantic_id: String },
    /// A declaration is missing from the module manifest.
    #[error("semantic object '{semantic_id}' from module '{module_id}' was not declared")]
    UndeclaredContribution {
        semantic_id: String,
        module_id: String,
    },
    /// A declared semantic object was not supplied by the contribution.
    #[error("declared semantic object '{semantic_id}' from module '{module_id}' is missing")]
    DeclaredContributionMissing {
        semantic_id: String,
        module_id: String,
    },
    /// A descriptor's kind/version disagrees with the definition.
    #[error("semantic descriptor '{semantic_id}' does not match definition {declared_kind}@{declared_version} vs {actual_kind}@{actual_version}")]
    ContributionDescriptorMismatch {
        semantic_id: String,
        declared_kind: String,
        actual_kind: String,
        declared_version: String,
        actual_version: String,
    },
    /// One stable metric key has multiple owners.
    #[error("metric key '{metric_key}' has owners '{first_owner}'/'{first_metric_id}' and '{second_owner}'/'{second_metric_id}'")]
    MetricOwnershipConflict {
        metric_key: String,
        first_owner: String,
        first_metric_id: String,
        second_owner: String,
        second_metric_id: String,
    },
    /// A relationship endpoint could not be resolved.
    #[error("relationship '{relationship_id}' references unknown endpoint '{endpoint}'")]
    UnknownRelationshipEndpoint {
        relationship_id: String,
        endpoint: String,
    },
    /// A non-relationship semantic reference could not be resolved.
    #[error("module '{owner_module}' references unknown semantic object '{reference}'")]
    UnknownSemanticReference {
        owner_module: String,
        reference: String,
    },
    /// A private semantic reference crosses module ownership.
    #[error("module '{owner_module}' cannot use private cross-module reference '{reference}'")]
    CrossModulePrivateReference {
        owner_module: String,
        reference: String,
    },
    /// A relationship's explicit flag disagrees with its endpoints.
    #[error("relationship '{relationship_id}' cross_module flag must be {expected_cross_module}")]
    RelationshipCrossModuleFlagMismatch {
        relationship_id: String,
        expected_cross_module: bool,
    },
    /// A required semantic scalar is empty.
    #[error("semantic object '{semantic_id}' has empty field '{field}'")]
    EmptySemanticField { semantic_id: String, field: String },
    /// SQL-like or physical execution text was supplied where semantic text is required.
    #[error("semantic object '{semantic_id}' contains a forbidden physical/query expression")]
    ForbiddenPhysicalExpression { semantic_id: String },
    /// Canonical output serialization failed.
    #[error("canonical semantic manifest serialization failed: {reason}")]
    Serialization { reason: String },
}

const MAX_SEMANTIC_TOKEN_LENGTH: usize = 192;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        FieldDefinition, MeasureDefinition, MetricDefinition, RelationshipDefinition,
    };
    use business_module_contracts::{
        BusinessModuleVersion, CompatibilityDescriptor, ContractDescriptor, DataClassification,
        ManifestSchemaVersion, PlatformCapabilityRequirement, SemanticContributionDescriptor,
    };

    fn module_id(value: &str) -> Result<BusinessModuleId, String> {
        BusinessModuleId::new(value.to_owned()).map_err(|error| error.to_string())
    }

    fn module_version(value: &str) -> Result<BusinessModuleVersion, String> {
        BusinessModuleVersion::new(value.to_owned()).map_err(|error| error.to_string())
    }

    fn schema_version() -> Result<ManifestSchemaVersion, String> {
        ManifestSchemaVersion::new("business-module.manifest.v1".to_owned())
            .map_err(|error| error.to_string())
    }

    fn semantic_version() -> Result<SemanticVersion, String> {
        SemanticVersion::new("1.0.0".to_owned()).map_err(|error| error.to_string())
    }

    fn reference(id: &str, access: SemanticReferenceAccess) -> SemanticReference {
        SemanticReference {
            semantic_id: id.to_owned(),
            access,
        }
    }

    fn descriptors(key: &str) -> Vec<SemanticContributionDescriptor> {
        vec![
            SemanticContributionDescriptor {
                semantic_id: "amount".to_owned(),
                semantic_kind: "field".to_owned(),
                version: "1.0.0".to_owned(),
            },
            SemanticContributionDescriptor {
                semantic_id: key.to_owned(),
                semantic_kind: "measure".to_owned(),
                version: "1.0.0".to_owned(),
            },
            SemanticContributionDescriptor {
                semantic_id: format!("{key}_metric"),
                semantic_kind: "metric".to_owned(),
                version: "1.0.0".to_owned(),
            },
        ]
    }

    fn manifest(
        id: &str,
        version: &str,
        contribution_descriptors: Vec<SemanticContributionDescriptor>,
    ) -> Result<BusinessModuleManifest, String> {
        Ok(BusinessModuleManifest {
            module_id: module_id(id)?,
            module_version: module_version(version)?,
            manifest_schema_version: schema_version()?,
            owned_bounded_contexts: vec![format!("{id}-context")],
            required_platform_capabilities: Vec::new(),
            optional_platform_capabilities: Vec::new(),
            published_commands: vec![ContractDescriptor {
                contract_id: format!("{id}.read"),
                version: "1.0.0".to_owned(),
            }],
            published_queries: Vec::new(),
            published_events: Vec::new(),
            resource_kinds: Vec::new(),
            data_classification: vec![DataClassification::Internal],
            migration_namespace: id.to_owned(),
            semantic_contributions: contribution_descriptors,
            ui_contributions: Vec::new(),
            agent_tool_contributions: Vec::new(),
            dependencies: Vec::new(),
            compatibility: CompatibilityDescriptor::default(),
        })
    }

    fn contribution(id: &str, metric_key: &str) -> Result<SemanticContribution, String> {
        let owner = module_id(id)?;
        let version = module_version("1.0.0")?;
        let semantic_version = semantic_version()?;
        Ok(SemanticContribution {
            module_id: owner.clone(),
            module_version: version,
            datasets: Vec::new(),
            projections: Vec::new(),
            fields: vec![FieldDefinition {
                id: "amount".to_owned(),
                version: semantic_version.clone(),
                owner_module: owner.clone(),
                semantic_type: "money".to_owned(),
                classification: DataClassification::Confidential,
                lineage_id: None,
            }],
            relationships: Vec::new(),
            measures: vec![MeasureDefinition {
                id: metric_key.to_owned(),
                version: semantic_version.clone(),
                owner_module: owner.clone(),
                value_type: "decimal".to_owned(),
                aggregation: "sum".to_owned(),
                source_field: reference("amount", SemanticReferenceAccess::PublishedObject),
                lineage_id: None,
            }],
            metrics: vec![MetricDefinition {
                id: format!("{metric_key}_metric"),
                version: semantic_version,
                owner_module: owner,
                metric_key: metric_key.to_owned(),
                measure: reference(metric_key, SemanticReferenceAccess::PublishedObject),
                dimensions: Vec::new(),
                time_dimension: None,
                formula: Some("sum of the published measure".to_owned()),
                lineage_id: None,
            }],
            dimensions: Vec::new(),
            time_dimensions: Vec::new(),
            filter_policies: Vec::new(),
            lineages: Vec::new(),
        })
    }

    fn field_only_contribution(id: &str) -> Result<SemanticContribution, String> {
        let owner = module_id(id)?;
        Ok(SemanticContribution {
            module_id: owner.clone(),
            module_version: module_version("1.0.0")?,
            datasets: Vec::new(),
            projections: Vec::new(),
            fields: vec![FieldDefinition {
                id: "amount".to_owned(),
                version: semantic_version()?,
                owner_module: owner,
                semantic_type: "money".to_owned(),
                classification: DataClassification::Confidential,
                lineage_id: None,
            }],
            relationships: Vec::new(),
            measures: Vec::new(),
            metrics: Vec::new(),
            dimensions: Vec::new(),
            time_dimensions: Vec::new(),
            filter_policies: Vec::new(),
            lineages: Vec::new(),
        })
    }

    fn input(
        modules: Vec<BusinessModuleManifest>,
        contributions: Vec<SemanticContribution>,
    ) -> SemanticCompilationInput {
        SemanticCompilationInput {
            modules,
            contributions,
            platform_capabilities: Vec::new(),
        }
    }

    #[test]
    fn compilation_is_deterministic_when_input_order_changes() -> Result<(), String> {
        let sales = manifest("sales", "1.0.0", descriptors("sales_amount"))?;
        let finance = manifest("finance", "1.0.0", descriptors("finance_amount"))?;
        let sales_contribution = contribution("sales", "sales_amount")?;
        let finance_contribution = contribution("finance", "finance_amount")?;
        let first = SemanticCompiler::compile(input(
            vec![sales.clone(), finance.clone()],
            vec![sales_contribution.clone(), finance_contribution.clone()],
        ))
        .map_err(|error| error.to_string())?;
        let second = SemanticCompiler::compile(input(
            vec![finance, sales],
            vec![finance_contribution, sales_contribution],
        ))
        .map_err(|error| error.to_string())?;
        assert_eq!(first.canonical_json, second.canonical_json);
        assert_eq!(first.sha256, second.sha256);
        assert_eq!(first.manifest, second.manifest);
        assert!(first
            .manifest
            .modules
            .iter()
            .flat_map(|module| module.semantic_ids.iter())
            .all(|semantic_id| semantic_id.contains('.')));
        Ok(())
    }

    #[test]
    fn semantic_namespaces_are_unique_across_modules() -> Result<(), String> {
        let sales = manifest("sales", "1.0.0", descriptors("sales_amount"))?;
        let finance = manifest("finance", "1.0.0", descriptors("finance_amount"))?;
        let compiled = SemanticCompiler::compile(input(
            vec![sales, finance],
            vec![
                contribution("sales", "sales_amount")?,
                contribution("finance", "finance_amount")?,
            ],
        ))
        .map_err(|error| error.to_string())?;

        let semantic_ids = compiled
            .manifest
            .modules
            .iter()
            .flat_map(|module| module.semantic_ids.iter())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(semantic_ids.len(), 6);
        assert!(semantic_ids.contains("sales.amount"));
        assert!(semantic_ids.contains("finance.amount"));
        Ok(())
    }

    #[test]
    fn duplicate_module_id_is_rejected() -> Result<(), String> {
        let sales = manifest("sales", "1.0.0", Vec::new())?;
        let result = SemanticCompiler::compile(input(vec![sales.clone(), sales], Vec::new()));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::DuplicateModule { .. })
        ));
        Ok(())
    }

    #[test]
    fn duplicate_semantic_id_is_rejected() -> Result<(), String> {
        let mut contribution = contribution("sales", "sales_amount")?;
        contribution.fields.push(contribution.fields[0].clone());
        let result = SemanticCompiler::compile(input(
            vec![manifest("sales", "1.0.0", descriptors("sales_amount"))?],
            vec![contribution],
        ));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::DuplicateSemanticId { .. })
        ));
        Ok(())
    }

    #[test]
    fn duplicate_metric_owner_is_rejected() -> Result<(), String> {
        let first = manifest("sales", "1.0.0", descriptors("sales_amount"))?;
        let second = manifest("finance", "1.0.0", descriptors("finance_amount"))?;
        let result = SemanticCompiler::compile(input(
            vec![first, second],
            vec![
                contribution("sales", "revenue")?,
                contribution("finance", "revenue")?,
            ],
        ));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::MetricOwnershipConflict { .. })
        ));
        Ok(())
    }

    #[test]
    fn semantic_owner_mismatch_is_rejected() -> Result<(), String> {
        let mut contribution = field_only_contribution("sales")?;
        contribution.fields[0].owner_module = module_id("finance")?;
        let result = SemanticCompiler::compile(input(
            vec![manifest("sales", "1.0.0", descriptors("sales_amount"))?],
            vec![contribution],
        ));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::SemanticOwnerMismatch { .. })
        ));
        Ok(())
    }

    #[test]
    fn incompatible_module_version_is_rejected() -> Result<(), String> {
        let mut sales = manifest("sales", "1.0.0", Vec::new())?;
        sales.dependencies.push(ModuleDependency {
            module_id: module_id("finance")?,
            version_requirement: "^2.0.0".to_owned(),
        });
        let finance = manifest("finance", "1.0.0", Vec::new())?;
        let result = SemanticCompiler::compile(input(vec![sales, finance], Vec::new()));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::IncompatibleModuleVersion { .. })
        ));
        Ok(())
    }

    #[test]
    fn cyclic_module_dependency_is_rejected() -> Result<(), String> {
        let mut sales = manifest("sales", "1.0.0", Vec::new())?;
        sales.dependencies.push(ModuleDependency {
            module_id: module_id("finance")?,
            version_requirement: "1.0.0".to_owned(),
        });
        let mut finance = manifest("finance", "1.0.0", Vec::new())?;
        finance.dependencies.push(ModuleDependency {
            module_id: module_id("sales")?,
            version_requirement: "1.0.0".to_owned(),
        });
        let result = SemanticCompiler::compile(input(vec![sales, finance], Vec::new()));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::CyclicModuleDependency { .. })
        ));
        Ok(())
    }

    #[test]
    fn illegal_platform_dependency_is_rejected() -> Result<(), String> {
        let mut sales = manifest("sales", "1.0.0", Vec::new())?;
        sales.dependencies.push(ModuleDependency {
            module_id: module_id("platform-analytics")?,
            version_requirement: "1.0.0".to_owned(),
        });
        let result = SemanticCompiler::compile(input(vec![sales], Vec::new()));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::IllegalPlatformDependency { .. })
        ));
        Ok(())
    }

    #[test]
    fn missing_module_dependency_is_rejected() -> Result<(), String> {
        let mut sales = manifest("sales", "1.0.0", Vec::new())?;
        sales.dependencies.push(ModuleDependency {
            module_id: module_id("finance")?,
            version_requirement: "1.0.0".to_owned(),
        });
        let result = SemanticCompiler::compile(input(vec![sales], Vec::new()));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::MissingModuleDependency { .. })
        ));
        Ok(())
    }

    #[test]
    fn unknown_relationship_endpoint_is_rejected() -> Result<(), String> {
        let finance = manifest(
            "finance",
            "1.0.0",
            vec![
                SemanticContributionDescriptor {
                    semantic_id: "account".to_owned(),
                    semantic_kind: "field".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                SemanticContributionDescriptor {
                    semantic_id: "account_owner".to_owned(),
                    semantic_kind: "relationship".to_owned(),
                    version: "1.0.0".to_owned(),
                },
            ],
        )?;
        let finance_id = module_id("finance")?;
        let contribution = SemanticContribution {
            module_id: finance_id.clone(),
            module_version: module_version("1.0.0")?,
            datasets: Vec::new(),
            projections: Vec::new(),
            fields: vec![FieldDefinition {
                id: "account".to_owned(),
                version: semantic_version()?,
                owner_module: finance_id.clone(),
                semantic_type: "account".to_owned(),
                classification: DataClassification::Confidential,
                lineage_id: None,
            }],
            relationships: vec![RelationshipDefinition {
                id: "account_owner".to_owned(),
                version: semantic_version()?,
                owner_module: finance_id,
                from: reference("missing-account", SemanticReferenceAccess::PublishedObject),
                to: reference("account", SemanticReferenceAccess::PublishedObject),
                relationship_kind: "references".to_owned(),
                cross_module: false,
            }],
            measures: Vec::new(),
            metrics: Vec::new(),
            dimensions: Vec::new(),
            time_dimensions: Vec::new(),
            filter_policies: Vec::new(),
            lineages: Vec::new(),
        };
        let result = SemanticCompiler::compile(input(vec![finance], vec![contribution]));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::UnknownRelationshipEndpoint { .. })
        ));
        Ok(())
    }

    #[test]
    fn undeclared_semantic_object_is_rejected() -> Result<(), String> {
        let result = SemanticCompiler::compile(input(
            vec![manifest("sales", "1.0.0", Vec::new())?],
            vec![field_only_contribution("sales")?],
        ));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::UndeclaredContribution { .. })
        ));
        Ok(())
    }

    #[test]
    fn cross_module_private_relationship_is_rejected() -> Result<(), String> {
        let sales = manifest(
            "sales",
            "1.0.0",
            vec![SemanticContributionDescriptor {
                semantic_id: "amount".to_owned(),
                semantic_kind: "field".to_owned(),
                version: "1.0.0".to_owned(),
            }],
        )?;
        let finance = manifest(
            "finance",
            "1.0.0",
            vec![
                SemanticContributionDescriptor {
                    semantic_id: "account".to_owned(),
                    semantic_kind: "field".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                SemanticContributionDescriptor {
                    semantic_id: "sales_account".to_owned(),
                    semantic_kind: "relationship".to_owned(),
                    version: "1.0.0".to_owned(),
                },
            ],
        )?;
        let finance_id = module_id("finance")?;
        let contribution = SemanticContribution {
            module_id: finance_id.clone(),
            module_version: module_version("1.0.0")?,
            datasets: Vec::new(),
            projections: Vec::new(),
            fields: vec![FieldDefinition {
                id: "account".to_owned(),
                version: semantic_version()?,
                owner_module: finance_id.clone(),
                semantic_type: "account".to_owned(),
                classification: DataClassification::Confidential,
                lineage_id: None,
            }],
            relationships: vec![RelationshipDefinition {
                id: "sales_account".to_owned(),
                version: semantic_version()?,
                owner_module: finance_id,
                from: reference("sales.amount", SemanticReferenceAccess::Private),
                to: reference("account", SemanticReferenceAccess::PublishedObject),
                relationship_kind: "references".to_owned(),
                cross_module: true,
            }],
            measures: Vec::new(),
            metrics: Vec::new(),
            dimensions: Vec::new(),
            time_dimensions: Vec::new(),
            filter_policies: Vec::new(),
            lineages: Vec::new(),
        };
        let result = SemanticCompiler::compile(input(
            vec![sales, finance],
            vec![field_only_contribution("sales")?, contribution],
        ));
        assert!(matches!(
            result,
            Err(SemanticCompilationError::CrossModulePrivateReference { .. })
        ));
        Ok(())
    }

    #[test]
    fn required_platform_capability_must_match() -> Result<(), String> {
        let mut sales = manifest("sales", "1.0.0", Vec::new())?;
        sales.required_platform_capabilities = vec![PlatformCapabilityRequirement {
            capability_id: "analytics-query".to_owned(),
            version_requirement: "^2.0.0".to_owned(),
        }];
        let mut compilation_input = input(vec![sales], Vec::new());
        compilation_input.platform_capabilities = vec![PlatformCapabilityVersion {
            capability_id: "analytics-query".to_owned(),
            version: "1.0.0".to_owned(),
        }];
        let result = SemanticCompiler::compile(compilation_input);
        assert!(matches!(
            result,
            Err(SemanticCompilationError::IncompatiblePlatformCapability { .. })
        ));
        Ok(())
    }

    #[test]
    fn removing_one_fixture_module_leaves_the_other_manifest_unchanged() -> Result<(), String> {
        let sales_manifest = manifest("fixture-sales", "1.0.0", descriptors("sales_amount"))?;
        let finance_manifest = manifest("fixture-finance", "1.0.0", descriptors("finance_amount"))?;
        let sales_contribution = contribution("fixture-sales", "sales_amount")?;
        let finance_contribution = contribution("fixture-finance", "finance_amount")?;

        let both = SemanticCompiler::compile(input(
            vec![sales_manifest.clone(), finance_manifest],
            vec![sales_contribution.clone(), finance_contribution],
        ))
        .map_err(|error| error.to_string())?;
        let sales_only =
            SemanticCompiler::compile(input(vec![sales_manifest], vec![sales_contribution]))
                .map_err(|error| error.to_string())?;

        let sales_from_both = both
            .manifest
            .modules
            .iter()
            .find(|module| module.module_id.as_str() == "fixture-sales")
            .ok_or("fixture-sales was not compiled")?;
        assert_eq!(
            sales_only.manifest.modules.as_slice(),
            std::slice::from_ref(sales_from_both)
        );
        assert!(!sales_only
            .manifest
            .modules
            .iter()
            .any(|module| module.module_id.as_str() == "fixture-finance"));
        assert!(!String::from_utf8_lossy(&sales_only.canonical_json).contains("fixture-finance"));
        Ok(())
    }
}
