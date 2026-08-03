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
}

#[derive(Debug, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub path: Option<String>,
}

pub fn validate(metadata: &Metadata) -> Result<(), Vec<String>> {
    let mut violations = Vec::new();
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
        let direct: BTreeSet<&str> = package
            .dependencies
            .iter()
            .map(|dependency| dependency.name.as_str())
            .collect();
        let forbidden: &[&str] = match package.name.as_str() {
            "shared-kernel" => &["axum", "sqlx", "reqwest", "aws-sdk-s3", "config", "tracing"],
            "document" => &[
                "axum",
                "sqlx",
                "reqwest",
                "aws-sdk-s3",
                "object-storage",
                "messaging",
            ],
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
            r#"{"packages":[{"name":"document","manifest_path":"C:/repo/crates/document/Cargo.toml","dependencies":[{"name":"shared-kernel","path":"C:/repo/crates/shared-kernel"}]}]}"#,
        );
        assert!(validate(&metadata).is_ok());
    }

    #[test]
    fn rejects_forbidden_core_dependency() {
        let metadata = parse(
            r#"{"packages":[{"name":"document","manifest_path":"C:/repo/crates/document/Cargo.toml","dependencies":[{"name":"sqlx","path":null}]}]}"#,
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
            r#"{"packages":[{"name":"workflow","manifest_path":"C:/repo/crates/workflow/Cargo.toml","dependencies":[{"name":"business-api","path":"C:/repo/apps/business-api"}]}]}"#,
        );
        assert!(validate(&metadata).is_err());
    }
}
