//! Cargo-metadata based workspace dependency fitness rules.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub manifest_path: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub metadata: Option<PackageMetadata>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PackageMetadata {
    #[serde(default)]
    pub architecture: Option<ArchitectureMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArchitectureMetadata {
    #[serde(default)]
    pub bounded_context: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    pub layer: String,
    #[serde(default)]
    pub migration_catalog: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

impl ArchitectureMetadata {
    fn context_name(&self) -> Option<&str> {
        self.context.as_deref().or(self.bounded_context.as_deref())
    }

    fn is_adapter(&self) -> bool {
        self.layer.contains("infrastructure")
            || self
                .role
                .as_deref()
                .is_some_and(|role| role.ends_with("-adapter"))
    }
}

#[derive(Debug, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub path: Option<String>,
}

pub fn validate(metadata: &Metadata) -> Result<(), Vec<String>> {
    let mut violations = Vec::new();
    let architecture_by_name = metadata
        .packages
        .iter()
        .filter_map(|package| {
            package
                .metadata
                .as_ref()?
                .architecture
                .as_ref()
                .map(|architecture| (package.name.as_str(), architecture))
        })
        .collect::<std::collections::HashMap<_, _>>();

    for package in &metadata.packages {
        let manifest = package.manifest_path.replace('\\', "/");
        if !manifest.contains("/crates/") {
            continue;
        }
        for dependency in &package.dependencies {
            if dependency
                .path
                .as_deref()
                .is_some_and(|path| path.replace('\\', "/").contains("/apps/"))
            {
                violations.push(format!(
                    "core crate {} depends on application {}",
                    package.name, dependency.name
                ));
            }
        }

        let Some(architecture) = package
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.architecture.as_ref())
        else {
            continue;
        };

        let direct: BTreeSet<&str> = package
            .dependencies
            .iter()
            .map(|dependency| dependency.name.as_str())
            .collect();
        let forbidden: &[&str] = match architecture.layer.as_str() {
            "domain-and-application" | "domain" => &[
                "axum",
                "sqlx",
                "sqlite",
                "postgres",
                "sea-orm",
                "diesel",
                "reqwest",
                "aws-sdk-s3",
                "object-storage",
                "messaging",
                "config",
                "tracing",
            ],
            "shared-kernel" => &["axum", "sqlx", "reqwest", "aws-sdk-s3", "config", "tracing"],
            _ => &[],
        };
        for dependency in forbidden {
            if direct.contains(dependency) {
                violations.push(format!(
                    "{} has forbidden direct dependency {}",
                    package.name, dependency
                ));
            }
        }

        if architecture.is_adapter() {
            for dependency in &package.dependencies {
                let Some(path) = dependency.path.as_deref() else {
                    continue;
                };
                if path.replace('\\', "/").contains("/crates/")
                    && !architecture_by_name
                        .get(dependency.name.as_str())
                        .is_some_and(|dependency_architecture| {
                            dependency_architecture.context_name() == architecture.context_name()
                                || dependency_architecture.layer == "platform-infrastructure"
                                || dependency_architecture.layer == "contracts"
                        })
                {
                    violations.push(format!(
                        "{} has forbidden inward workspace dependency {}",
                        package.name, dependency.name
                    ));
                }
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

pub fn load(path: &Path) -> Result<Metadata, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Metadata {
        serde_json::from_str(input).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn accepts_legal_core_graph() {
        let metadata = parse(
            r#"{"packages":[{"name":"document","manifest_path":"C:/repo/crates/document/Cargo.toml","metadata":{"architecture":{"bounded_context":"document-management","layer":"domain-and-application"}},"dependencies":[{"name":"shared-kernel","path":"C:/repo/crates/shared-kernel"}]}]}"#,
        );
        assert!(validate(&metadata).is_ok());
    }

    #[test]
    fn rejects_forbidden_core_dependency() {
        let metadata = parse(
            r#"{"packages":[{"name":"document","manifest_path":"C:/repo/crates/document/Cargo.toml","metadata":{"architecture":{"bounded_context":"document-management","layer":"domain-and-application"}},"dependencies":[{"name":"sqlx","path":null}]}]}"#,
        );
        let violations = match validate(&metadata) {
            Ok(()) => unreachable!(),
            Err(violations) => violations,
        };
        assert!(violations[0].contains("forbidden direct dependency sqlx"));
    }

    #[test]
    fn rejects_core_dependency_on_application() {
        let metadata = parse(
            r#"{"packages":[{"name":"workflow","manifest_path":"C:/repo/crates/workflow/Cargo.toml","metadata":{"architecture":{"bounded_context":"workflow","layer":"domain-and-application"}},"dependencies":[{"name":"business-api","path":"C:/repo/apps/business-api"}]}]}"#,
        );
        assert!(validate(&metadata).is_err());
    }

    #[test]
    fn rejects_database_adapter_dependency_on_an_unrelated_core_crate() {
        let metadata = parse(
            r#"{"packages":[{"name":"document-sqlite","manifest_path":"C:/repo/crates/document-sqlite/Cargo.toml","metadata":{"architecture":{"bounded_context":"document-management","layer":"infrastructure-adapter"}},"dependencies":[{"name":"document","path":"C:/repo/crates/document"},{"name":"messaging","path":"C:/repo/crates/messaging"}]}]}"#,
        );
        assert!(validate(&metadata).is_err());
    }
}
