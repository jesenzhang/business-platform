//! Pure, deterministic compilation of declared business application packages.
//!
//! This crate produces rebuildable evidence only. It does not register, load,
//! persist, execute, or discover business applications.

use business_module_contracts::{
    BusinessModuleId, BusinessModuleManifest, BusinessModuleVersion, ExtensionContribution,
    ManifestValidationError, PackageDigest, PublicContributionCatalog, PublicContributionTarget,
    PublicTargetKind, PublishedDependencyCatalog, PublishedDependencyReference,
    PublishedExtensionPoint, TypedContributionSet,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const BUSINESS_APPLICATION_PACKAGE_SCHEMA_VERSION: &str = "business-application.package.v1";

/// A typed package declaration and its optional published extension contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessApplicationPackage {
    pub manifest: BusinessModuleManifest,
    #[serde(default)]
    pub contributions: TypedContributionSet,
    #[serde(default)]
    pub extension_points: Vec<PublishedExtensionPoint>,
    #[serde(default)]
    pub extension_contributions: Vec<ExtensionContribution>,
}

/// Input to a compilation. Installed versions are evidence supplied by a host;
/// they do not become registry state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusinessApplicationCompilerInput {
    pub platform_version: String,
    pub packages: Vec<BusinessApplicationPackage>,
    #[serde(default)]
    pub installed_versions: BTreeMap<BusinessModuleId, BusinessModuleVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledBusinessApplicationManifest {
    pub schema_version: String,
    pub platform_version: String,
    pub packages: Vec<BusinessApplicationPackage>,
    /// SHA-256 of `canonical_json`; this field is excluded from that payload.
    pub package_digest: PackageDigest,
    #[serde(skip)]
    canonical_json: Vec<u8>,
}

impl CompiledBusinessApplicationManifest {
    #[must_use]
    /// Canonical digest payload, excluding the self-referential digest field.
    pub fn canonical_json(&self) -> &[u8] {
        &self.canonical_json
    }

    #[must_use]
    pub fn package_digest(&self) -> &PackageDigest {
        &self.package_digest
    }
}

#[derive(Debug, Error)]
pub enum CompilationError {
    #[error("manifest validation failed: {0}")]
    Manifest(#[from] ManifestValidationError),
    #[error("invalid SemVer in {field}: '{value}'")]
    InvalidVersion { field: &'static str, value: String },
    #[error("invalid SemVer requirement in {field}: '{value}'")]
    InvalidRequirement { field: &'static str, value: String },
    #[error("platform version '{version}' is incompatible with package '{module_id}'")]
    IncompatiblePlatform { module_id: String, version: String },
    #[error("unknown module dependency '{module_id}'")]
    UnknownDependency { module_id: String },
    #[error("module dependency '{module_id}' is incompatible with '{requirement}'")]
    IncompatibleDependency {
        module_id: String,
        requirement: String,
    },
    #[error("module dependency graph contains a cycle")]
    DependencyCycle,
    #[error("package downgrade for '{module_id}': installed {installed}, incoming {incoming}")]
    Downgrade {
        module_id: String,
        installed: String,
        incoming: String,
    },
    #[error("duplicate {kind} '{identifier}'")]
    Duplicate {
        kind: &'static str,
        identifier: String,
    },
    #[error("ownership collision for {kind} '{identifier}'")]
    OwnershipCollision {
        kind: &'static str,
        identifier: String,
    },
    #[error("extension contribution targets unknown extension point '{identifier}'")]
    UnknownExtension { identifier: String },
    #[error("canonical manifest serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid package digest")]
    Digest,
}

#[allow(clippy::too_many_lines)]
pub fn compile(
    mut input: BusinessApplicationCompilerInput,
) -> Result<CompiledBusinessApplicationManifest, CompilationError> {
    let platform = parse_version("platform version", &input.platform_version)?;
    let mut modules = BTreeMap::new();
    let mut owned_contexts = BTreeMap::<String, String>::new();
    let mut public_ids = BTreeMap::<String, String>::new();
    let mut extension_ids = BTreeSet::new();
    let mut extension_contribution_ids = BTreeSet::new();

    for package in &input.packages {
        package.manifest.validate()?;
    }
    let public_catalog = public_catalog(&input.packages)?;
    let dependency_catalog = dependency_catalog(&input.packages);

    for package in &mut input.packages {
        let module_id = package.manifest.module_id.to_string();
        let module_version =
            parse_version("module version", package.manifest.module_version.as_str())?;
        if modules
            .insert(module_id.clone(), module_version.clone())
            .is_some()
        {
            return Err(CompilationError::Duplicate {
                kind: "module",
                identifier: module_id,
            });
        }
        if let Some(installed) = input.installed_versions.get(&package.manifest.module_id) {
            let installed_version = parse_version("installed module version", installed.as_str())?;
            if module_version < installed_version {
                return Err(CompilationError::Downgrade {
                    module_id,
                    installed: installed.to_string(),
                    incoming: module_version.to_string(),
                });
            }
        }
        validate_platform(&package.manifest, &platform)?;
        package
            .contributions
            .validate(&package.manifest.module_id, &public_catalog)?;
        validate_manifest_versions(&package.manifest)?;
        register_manifest_ids(package, &mut owned_contexts, &mut public_ids)?;
        for point in &package.extension_points {
            if !extension_ids.insert(point.extension_point_id.clone()) {
                return Err(CompilationError::Duplicate {
                    kind: "extension point",
                    identifier: point.extension_point_id.to_string(),
                });
            }
            if point.owner_module_id != package.manifest.module_id {
                return Err(CompilationError::OwnershipCollision {
                    kind: "extension point",
                    identifier: point.extension_point_id.to_string(),
                });
            }
            point.validate_against_catalog(&dependency_catalog)?;
        }
        for contribution in &package.extension_contributions {
            if contribution.consumer_module_id != package.manifest.module_id {
                return Err(CompilationError::OwnershipCollision {
                    kind: "extension contribution",
                    identifier: contribution.contribution_id.to_string(),
                });
            }
            if !extension_contribution_ids.insert(contribution.contribution_id.clone()) {
                return Err(CompilationError::Duplicate {
                    kind: "extension contribution",
                    identifier: contribution.contribution_id.to_string(),
                });
            }
        }
    }
    for package in &input.packages {
        for dependency in &package.manifest.dependencies {
            let Some(version) = modules.get(dependency.module_id.as_str()) else {
                return Err(CompilationError::UnknownDependency {
                    module_id: dependency.module_id.to_string(),
                });
            };
            let requirement =
                parse_requirement("module dependency", &dependency.version_requirement)?;
            if !requirement.matches(version) {
                return Err(CompilationError::IncompatibleDependency {
                    module_id: dependency.module_id.to_string(),
                    requirement: dependency.version_requirement.clone(),
                });
            }
        }
        for contribution in &package.extension_contributions {
            let point = input
                .packages
                .iter()
                .flat_map(|p| p.extension_points.iter())
                .find(|p| p.extension_point_id == contribution.target_extension_point_id)
                .ok_or_else(|| CompilationError::UnknownExtension {
                    identifier: contribution.target_extension_point_id.to_string(),
                })?;
            if contribution.contribution_id.module_id() != contribution.consumer_module_id.as_str()
            {
                return Err(CompilationError::OwnershipCollision {
                    kind: "extension contribution",
                    identifier: contribution.contribution_id.to_string(),
                });
            }
            contribution.validate_against_catalog(point, &dependency_catalog)?;
        }
    }
    detect_cycles(&input.packages)?;
    normalize(&mut input.packages);
    input
        .packages
        .sort_by_key(|p| p.manifest.module_id.to_string());
    let mut compiled = CompiledBusinessApplicationManifest {
        schema_version: BUSINESS_APPLICATION_PACKAGE_SCHEMA_VERSION.to_owned(),
        platform_version: platform.to_string(),
        packages: input.packages,
        package_digest: PackageDigest::new("0".repeat(64)).map_err(|_| CompilationError::Digest)?,
        canonical_json: Vec::new(),
    };
    let bytes = canonical_bytes(&compiled)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    compiled.package_digest = PackageDigest::new(digest).map_err(|_| CompilationError::Digest)?;
    compiled.canonical_json = bytes;
    Ok(compiled)
}

fn parse_version(field: &'static str, value: &str) -> Result<Version, CompilationError> {
    Version::parse(value).map_err(|_| CompilationError::InvalidVersion {
        field,
        value: value.to_owned(),
    })
}

fn parse_requirement(field: &'static str, value: &str) -> Result<VersionReq, CompilationError> {
    VersionReq::parse(value).map_err(|_| CompilationError::InvalidRequirement {
        field,
        value: value.to_owned(),
    })
}

fn canonical_bytes(
    compiled: &CompiledBusinessApplicationManifest,
) -> Result<Vec<u8>, serde_json::Error> {
    #[derive(Serialize)]
    struct Canonical<'a> {
        schema_version: &'a str,
        platform_version: &'a str,
        packages: &'a [BusinessApplicationPackage],
    }
    serde_json::to_vec(&Canonical {
        schema_version: &compiled.schema_version,
        platform_version: &compiled.platform_version,
        packages: &compiled.packages,
    })
}

fn public_catalog(
    packages: &[BusinessApplicationPackage],
) -> Result<PublicContributionCatalog, CompilationError> {
    let mut targets = Vec::new();
    for package in packages {
        let owner = package.manifest.module_id.clone();
        targets.extend(package.manifest.resource_kinds.iter().map(|item| {
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Resource {
                    resource_kind: item.resource_kind.clone(),
                },
                version: item.version.clone(),
            }
        }));
        targets.extend(package.manifest.published_queries.iter().map(|item| {
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Query {
                    query_id: item.contract_id.clone(),
                },
                version: item.version.clone(),
            }
        }));
        targets.extend(package.manifest.published_commands.iter().map(|item| {
            PublicContributionTarget {
                owner_module_id: owner.clone(),
                target: PublicTargetKind::Command {
                    command_id: item.contract_id.clone(),
                },
                version: item.version.clone(),
            }
        }));
    }
    targets.sort_by_key(|target| serde_json::to_string(target).unwrap_or_else(|_| String::new()));
    if targets.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CompilationError::Duplicate {
            kind: "public target",
            identifier: serde_json::to_string(&targets[0])?,
        });
    }
    Ok(PublicContributionCatalog {
        public_targets: targets,
    })
}

fn dependency_catalog(packages: &[BusinessApplicationPackage]) -> PublishedDependencyCatalog {
    let mut public_dependencies = Vec::new();
    for package in packages {
        let owner = package.manifest.module_id.clone();
        public_dependencies.extend(package.manifest.resource_kinds.iter().map(|item| {
            PublishedDependencyReference::PublicResource {
                owner_module_id: owner.clone(),
                resource_kind: item.resource_kind.clone(),
                version: item.version.clone(),
            }
        }));
        public_dependencies.extend(package.manifest.published_queries.iter().map(|item| {
            PublishedDependencyReference::PublicQuery {
                owner_module_id: owner.clone(),
                query_id: item.contract_id.clone(),
                version: item.version.clone(),
            }
        }));
        public_dependencies.extend(package.manifest.published_commands.iter().map(|item| {
            PublishedDependencyReference::PublicCapability {
                owner_module_id: owner.clone(),
                capability_id: item.contract_id.clone(),
                version: item.version.clone(),
            }
        }));
    }
    public_dependencies.sort_by_key(|dependency| {
        serde_json::to_string(dependency).unwrap_or_else(|_| String::new())
    });
    PublishedDependencyCatalog {
        public_dependencies,
    }
}

fn validate_platform(
    manifest: &BusinessModuleManifest,
    platform: &Version,
) -> Result<(), CompilationError> {
    let min = manifest
        .compatibility
        .minimum_platform_version
        .as_deref()
        .map(|v| parse_version("minimum platform version", v))
        .transpose()?;
    let max = manifest
        .compatibility
        .maximum_platform_version
        .as_deref()
        .map(|v| parse_version("maximum platform version", v))
        .transpose()?;
    if min.as_ref().is_some_and(|v| platform < v)
        || max.as_ref().is_some_and(|v| platform > v)
        || min.as_ref().zip(max.as_ref()).is_some_and(|(a, b)| a > b)
    {
        return Err(CompilationError::IncompatiblePlatform {
            module_id: manifest.module_id.to_string(),
            version: platform.to_string(),
        });
    }
    for capability in manifest
        .required_platform_capabilities
        .iter()
        .chain(&manifest.optional_platform_capabilities)
    {
        parse_requirement("platform capability", &capability.version_requirement)?;
    }
    Ok(())
}

fn validate_manifest_versions(manifest: &BusinessModuleManifest) -> Result<(), CompilationError> {
    for version in manifest
        .published_commands
        .iter()
        .map(|item| &item.version)
        .chain(manifest.published_queries.iter().map(|item| &item.version))
        .chain(manifest.published_events.iter().map(|item| &item.version))
        .chain(manifest.resource_kinds.iter().map(|item| &item.version))
        .chain(
            manifest
                .semantic_contributions
                .iter()
                .map(|item| &item.version),
        )
        .chain(manifest.ui_contributions.iter().map(|item| &item.version))
        .chain(
            manifest
                .agent_tool_contributions
                .iter()
                .map(|item| &item.version),
        )
    {
        parse_version("contract version", version)?;
    }
    Ok(())
}

fn register_manifest_ids(
    package: &BusinessApplicationPackage,
    contexts: &mut BTreeMap<String, String>,
    public_ids: &mut BTreeMap<String, String>,
) -> Result<(), CompilationError> {
    let owner = package.manifest.module_id.to_string();
    for context in &package.manifest.owned_bounded_contexts {
        if let Some(previous) = contexts.insert(context.clone(), owner.clone()) {
            return Err(if previous == owner {
                CompilationError::Duplicate {
                    kind: "bounded context",
                    identifier: context.clone(),
                }
            } else {
                CompilationError::OwnershipCollision {
                    kind: "bounded context",
                    identifier: context.clone(),
                }
            });
        }
    }
    for id in package
        .manifest
        .published_commands
        .iter()
        .map(|x| &x.contract_id)
        .chain(
            package
                .manifest
                .published_queries
                .iter()
                .map(|x| &x.contract_id),
        )
        .chain(
            package
                .manifest
                .published_events
                .iter()
                .map(|x| &x.contract_id),
        )
        .chain(
            package
                .manifest
                .resource_kinds
                .iter()
                .map(|x| &x.resource_kind),
        )
        .chain(
            package
                .manifest
                .ui_contributions
                .iter()
                .map(|x| &x.contribution_id),
        )
        .chain(
            package
                .manifest
                .agent_tool_contributions
                .iter()
                .map(|x| &x.contribution_id),
        )
        .chain(
            package
                .manifest
                .semantic_contributions
                .iter()
                .map(|x| &x.semantic_id),
        )
    {
        if let Some(previous) = public_ids.insert(id.clone(), owner.clone()) {
            return Err(if previous == owner {
                CompilationError::Duplicate {
                    kind: "public identifier",
                    identifier: id.clone(),
                }
            } else {
                CompilationError::OwnershipCollision {
                    kind: "public identifier",
                    identifier: id.clone(),
                }
            });
        }
    }
    Ok(())
}

fn detect_cycles(packages: &[BusinessApplicationPackage]) -> Result<(), CompilationError> {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, Vec<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> bool {
        if active.contains(node) {
            return true;
        }
        if done.contains(node) {
            return false;
        }
        active.insert(node.to_owned());
        let cycle = graph
            .get(node)
            .is_some_and(|deps| deps.iter().any(|dep| visit(dep, graph, active, done)));
        active.remove(node);
        done.insert(node.to_owned());
        cycle
    }
    let graph: BTreeMap<_, _> = packages
        .iter()
        .map(|p| {
            (
                p.manifest.module_id.to_string(),
                p.manifest
                    .dependencies
                    .iter()
                    .map(|d| d.module_id.to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();
    if graph
        .keys()
        .any(|node| visit(node, &graph, &mut active, &mut done))
    {
        Err(CompilationError::DependencyCycle)
    } else {
        Ok(())
    }
}

fn normalize(packages: &mut [BusinessApplicationPackage]) {
    for package in packages {
        let m = &mut package.manifest;
        m.owned_bounded_contexts.sort();
        m.required_platform_capabilities
            .sort_by_key(|x| (x.capability_id.clone(), x.version_requirement.clone()));
        m.optional_platform_capabilities
            .sort_by_key(|x| (x.capability_id.clone(), x.version_requirement.clone()));
        m.published_commands
            .sort_by_key(|x| (x.contract_id.clone(), x.version.clone()));
        m.published_queries
            .sort_by_key(|x| (x.contract_id.clone(), x.version.clone()));
        m.published_events
            .sort_by_key(|x| (x.contract_id.clone(), x.version.clone()));
        m.resource_kinds
            .sort_by_key(|x| (x.resource_kind.clone(), x.version.clone()));
        m.data_classification.sort();
        m.semantic_contributions
            .sort_by_key(|x| (x.semantic_id.clone(), x.version.clone()));
        m.ui_contributions
            .sort_by_key(|x| (x.contribution_id.clone(), x.version.clone()));
        m.agent_tool_contributions
            .sort_by_key(|x| (x.contribution_id.clone(), x.version.clone()));
        m.dependencies
            .sort_by_key(|x| (x.module_id.to_string(), x.version_requirement.clone()));
        package
            .extension_points
            .sort_by_key(|x| x.extension_point_id.clone());
        package
            .extension_contributions
            .sort_by_key(|x| x.contribution_id.clone());
    }
}
