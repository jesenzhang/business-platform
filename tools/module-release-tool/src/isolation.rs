//! Consumer isolation, judged from the resolved dependency graph.
//!
//! Searching `Cargo.toml` and `Cargo.lock` for a backslash path proves nothing:
//! `Cargo.lock` does not record where a path package lives, a forward slash or a
//! relative path is not a backslash, and a path dependency reached *transitively*
//! is not mentioned in the consumer manifest at all. The authoritative answer is
//! `cargo metadata --format-version 1`, which resolves the whole graph —
//! including transitive path dependencies — and reports the manifest path of
//! every package it resolved.
//!
//! The rule this module implements, and which both the orchestration script and
//! the Rust regression tests call (rather than each keeping their own copy):
//!
//! * every package with `source: null` — i.e. a local path package, the consumer
//!   itself and everything it reaches by path — must resolve, after symlink
//!   resolution and per-platform case rules, inside the consumer workspace or an
//!   explicitly allowed artifact directory;
//! * a local package that resolves inside a forbidden root (the source
//!   repository) or outside every declared root is a violation;
//! * anything that cannot be resolved on disk is a violation, never a pass.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::{ToolError, ToolResult};

/// Check identifier reported on a violation.
pub const CHECK_ISOLATION: &str = "consumer-graph-isolation";

/// Roots a resolved local package may legitimately live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationPolicy {
    /// The consumer workspace itself.
    pub consumer_root: PathBuf,
    /// Artifact directories the consumer is allowed to depend on.
    pub allowed_roots: Vec<PathBuf>,
    /// Roots that must never appear in the graph, typically the source
    /// repository the artifact was produced from.
    pub forbidden_roots: Vec<PathBuf>,
}

/// One local package found in the resolved graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPackage {
    /// `name version`.
    pub package: String,
    /// Manifest path exactly as cargo reported it.
    pub reported_path: String,
    /// The same path after symlink resolution and case normalisation.
    pub resolved_path: String,
    /// Which allowed root it fell inside, or `forbidden` / `unauthorized`.
    pub verdict: String,
}

/// Outcome of one isolation check, suitable for an evidence log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationReport {
    /// Resolved consumer workspace root.
    pub workspace_root: String,
    /// Every local (path-sourced) package in the graph, sorted.
    pub local_packages: Vec<LocalPackage>,
    /// Count of registry / non-local packages, which the rule ignores on purpose.
    pub registry_packages: usize,
}

#[derive(Deserialize)]
struct MetadataPackage {
    name: String,
    version: String,
    #[serde(default)]
    source: Option<String>,
    manifest_path: String,
}

#[derive(Deserialize)]
struct MetadataDocument {
    packages: Vec<MetadataPackage>,
    workspace_root: String,
}

/// Runs the isolation rule over a `cargo metadata --format-version 1` document.
pub fn check_metadata(
    metadata_json: &str,
    policy: &IsolationPolicy,
) -> ToolResult<IsolationReport> {
    let document: MetadataDocument =
        serde_json::from_str(metadata_json).map_err(|source| ToolError::Verification {
            check: CHECK_ISOLATION.to_owned(),
            reason: format!("cargo metadata could not be parsed: {source}"),
        })?;

    let consumer = NormalisedPath::resolve(&policy.consumer_root)?;
    let allowed: Vec<NormalisedPath> = policy
        .allowed_roots
        .iter()
        .map(|root| NormalisedPath::resolve(root))
        .collect::<ToolResult<_>>()?;
    let forbidden: Vec<NormalisedPath> = policy
        .forbidden_roots
        .iter()
        .map(|root| NormalisedPath::resolve(root))
        .collect::<ToolResult<_>>()?;

    let workspace = NormalisedPath::resolve(Path::new(&document.workspace_root))?;
    let mut local_packages = Vec::new();
    let mut registry_packages = 0usize;
    for package in &document.packages {
        if package.source.is_some() {
            registry_packages += 1;
            continue;
        }
        let manifest = NormalisedPath::resolve(Path::new(&package.manifest_path))?;
        let verdict = if manifest.within(&consumer) {
            "consumer".to_owned()
        } else if let Some(root) = allowed.iter().find(|root| manifest.within(root)) {
            format!("allowed: {}", root.display())
        } else if forbidden.iter().any(|root| manifest.within(root)) {
            "FORBIDDEN".to_owned()
        } else {
            "unauthorized".to_owned()
        };
        local_packages.push(LocalPackage {
            package: format!("{} {}", package.name, package.version),
            reported_path: package.manifest_path.clone(),
            resolved_path: manifest.display(),
            verdict,
        });
    }
    local_packages.sort_by(|left, right| left.package.cmp(&right.package));

    let mut offenders: Vec<String> = local_packages
        .iter()
        .filter(|package| package.verdict == "FORBIDDEN" || package.verdict == "unauthorized")
        .map(|package| {
            format!(
                "{} resolves to '{}' (reported '{}', verdict {})",
                package.package, package.resolved_path, package.reported_path, package.verdict
            )
        })
        .collect();
    if !(workspace.within(&consumer) || allowed.iter().any(|root| workspace.within(root))) {
        offenders.push(format!(
            "the resolved workspace root is '{}' rather than the consumer workspace '{}', so the \
             consumer is being built inside some other workspace",
            workspace.display(),
            consumer.display()
        ));
    }
    if !offenders.is_empty() {
        offenders.sort();
        return Err(ToolError::Verification {
            check: CHECK_ISOLATION.to_owned(),
            reason: format!(
                "the resolved dependency graph is not isolated from the source workspace: {}",
                offenders.join("; ")
            ),
        });
    }
    Ok(IsolationReport {
        workspace_root: workspace.display(),
        local_packages,
        registry_packages,
    })
}

/// A path compared the way a filesystem compares it, not the way a string does.
///
/// Symlinks are resolved, the Windows verbatim prefix is removed, comparison
/// honours the platform's case rules, and containment is decided on component
/// boundaries so `F:\repo` never swallows `F:\repo-evil`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalisedPath {
    components: Vec<String>,
    display: String,
}

impl NormalisedPath {
    fn resolve(path: &Path) -> ToolResult<Self> {
        let shown = path.display().to_string();
        let resolved = std::fs::canonicalize(path).map_err(|source| ToolError::Verification {
            check: CHECK_ISOLATION.to_owned(),
            reason: format!("cannot resolve '{shown}' on disk to decide isolation: {source}"),
        })?;
        Ok(Self::new(&resolved))
    }

    fn new(path: &Path) -> Self {
        let display = strip_verbatim_prefix(&path.display().to_string());
        let components = path
            .components()
            .map(|component| match component {
                Component::Prefix(prefix) => fold_case(&prefix.as_os_str().to_string_lossy()),
                Component::RootDir => "/".to_owned(),
                Component::CurDir => ".".to_owned(),
                Component::ParentDir => "..".to_owned(),
                Component::Normal(name) => fold_case(&name.to_string_lossy()),
            })
            .collect::<Vec<_>>();
        Self {
            components,
            display,
        }
    }

    /// Whether this path is this very directory or lives inside it.
    fn within(&self, root: &Self) -> bool {
        self.components.len() >= root.components.len()
            && self
                .components
                .iter()
                .zip(root.components.iter())
                .all(|(mine, theirs)| mine == theirs)
    }

    fn display(&self) -> String {
        self.display.clone()
    }
}

/// Case folding for path comparison only; the display form is left alone.
fn fold_case(text: &str) -> String {
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text.to_owned()
    }
}

/// Strips the Windows verbatim prefixes (`\\?\`, `\\.\`) that `canonicalize`
/// adds, so a canonical path can be compared and reported like an ordinary one.
fn strip_verbatim_prefix(text: &str) -> String {
    for prefix in ["\\\\?\\UNC\\", "\\\\?\\", "\\\\.\\"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.to_owned();
        }
    }
    text.to_owned()
}

/// A deterministic list of the roots a policy declares, for logging.
#[must_use]
pub fn describe_policy(policy: &IsolationPolicy) -> Vec<String> {
    let mut lines = vec![format!(
        "consumer root : {}",
        policy.consumer_root.display()
    )];
    let mut allowed = BTreeSet::new();
    for root in &policy.allowed_roots {
        allowed.insert(format!("allowed root   : {}", root.display()));
    }
    let mut forbidden = BTreeSet::new();
    for root in &policy.forbidden_roots {
        forbidden.insert(format!("forbidden root : {}", root.display()));
    }
    lines.extend(allowed);
    lines.extend(forbidden);
    lines
}
