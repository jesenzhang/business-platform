//! Read-only facts about the repository the candidate is built from.
//!
//! Everything here is derived from checked-in files:
//!
//! * `source_revision` comes from `.git/HEAD` (following a symref through
//!   `refs/` and `packed-refs`), **not** from a branch or tag name;
//! * the platform version, edition, MSRV and licence come from the root
//!   `[workspace.package]` table, so a module release version can never be
//!   confused with the product version;
//! * the toolchain identifier comes from `rust-toolchain.toml`, which keeps the
//!   artifact free of host names, host paths and build timestamps;
//! * dependency versions are pinned from `Cargo.lock`, so a packed crate cannot
//!   silently resolve to something else later.
//!
//! The only outside process used here is `git` itself, and only to ask what `HEAD`
//! resolves to (see [`read_git_revision`]); no other external command is run and
//! nothing outside the repository is read.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use business_module_contracts::{BuildToolchain, SourceRevision};
use semver::{Version, VersionReq};

use crate::{ToolError, ToolResult};

/// Facts read from the repository, all of them reproducible.
#[derive(Debug, Clone)]
pub struct RepoContext {
    root: PathBuf,
    revision: String,
    platform_version: String,
    edition: String,
    rust_version: String,
    license: String,
    toolchain_channel: String,
    lock_versions: BTreeMap<String, Vec<String>>,
    workspace_requirements: BTreeMap<String, String>,
    workspace_dependency_entries: BTreeMap<String, String>,
}

impl RepoContext {
    /// Finds the repository root by walking up from this crate's manifest.
    pub fn discover() -> ToolResult<Self> {
        let own = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut cursor = Some(own);
        while let Some(directory) = cursor {
            if directory.join("rust-toolchain.toml").is_file()
                && directory.join("Cargo.lock").is_file()
                && directory.join("Cargo.toml").is_file()
            {
                return Self::with_root(directory.to_path_buf());
            }
            cursor = directory.parent();
        }
        Err(ToolError::Usage(format!(
            "could not locate the repository root above '{}'",
            own.display()
        )))
    }

    /// Reads the repository facts for an explicit root.
    pub fn with_root(root: PathBuf) -> ToolResult<Self> {
        let root_text = root.display().to_string();
        let workspace = read_file(&root.join("Cargo.toml"))?;
        let package_table = parse_section(&workspace, "workspace.package")?;
        let toolchain_file = read_file(&root.join("rust-toolchain.toml"))?;
        let lock = read_file(&root.join("Cargo.lock"))?;
        let workspace_dependencies = parse_section(&workspace, "workspace.dependencies")?;

        let revision = read_git_revision(&root)?;
        let context = Self {
            root,
            revision,
            platform_version: required(&package_table, "version", "root [workspace.package]")?,
            edition: required(&package_table, "edition", "root [workspace.package]")?,
            rust_version: required(&package_table, "rust-version", "root [workspace.package]")?,
            license: required(&package_table, "license", "root [workspace.package]")?,
            toolchain_channel: required(
                &parse_section(&toolchain_file, "toolchain")?,
                "channel",
                "rust-toolchain.toml [toolchain]",
            )?,
            lock_versions: parse_lock_versions(&lock)?,
            workspace_requirements: extract_versions(&workspace_dependencies),
            workspace_dependency_entries: workspace_dependencies,
        };
        if context.revision.is_empty() {
            return Err(ToolError::Usage(format!(
                "repository root '{root_text}' has no readable git revision"
            )));
        }
        Ok(context)
    }

    /// Repository root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Pinned source revision, validated by the pure contract type.
    pub fn source_revision(&self) -> ToolResult<SourceRevision> {
        Ok(SourceRevision::new(self.revision.clone())?)
    }

    /// Full revision string.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Toolchain pin used in the release identity.
    #[must_use]
    pub fn build_toolchain(&self) -> BuildToolchain {
        BuildToolchain {
            toolchain_id: format!("rust-toolchain-{}", self.rust_version),
            version: self.toolchain_channel.clone(),
        }
    }

    /// Product/platform version used as the compiler's platform coordinate.
    #[must_use]
    pub fn platform_version(&self) -> &str {
        &self.platform_version
    }

    /// Edition to write into a packed crate manifest.
    #[must_use]
    pub fn edition(&self) -> &str {
        &self.edition
    }

    /// MSRV to write into a packed crate manifest.
    #[must_use]
    pub fn rust_version(&self) -> &str {
        &self.rust_version
    }

    /// Licence to write into a packed crate manifest.
    #[must_use]
    pub fn license(&self) -> &str {
        &self.license
    }

    /// The dependency entry as the root `[workspace.dependencies]` table writes
    /// it (an inline table, or a bare version string).
    ///
    /// Packaging starts from this entry rather than from the member crate's
    /// `x.workspace = true` line: a workspace may attach a rename (`package`),
    /// `default-features = false` or features to a dependency, and dropping
    /// them while pinning the version would silently change what the released
    /// crate is.
    pub fn workspace_dependency_entry(&self, dependency: &str) -> ToolResult<String> {
        self.workspace_dependency_entries
            .get(dependency)
            .cloned()
            .ok_or_else(|| ToolError::Packaging {
                crate_name: dependency.to_owned(),
                reason: format!(
                    "'{dependency}' is not declared in the root [workspace.dependencies] table"
                ),
            })
    }

    /// Exact `Cargo.lock` version for one third-party dependency.
    ///
    /// A dependency that exists at several versions in the lock is resolved
    /// against the requirement declared in the root `[workspace.dependencies]`
    /// table. Anything ambiguous fails closed instead of guessing.
    pub fn locked_version(&self, dependency: &str) -> ToolResult<String> {
        let candidates =
            self.lock_versions
                .get(dependency)
                .ok_or_else(|| ToolError::Packaging {
                    crate_name: dependency.to_owned(),
                    reason: format!("dependency '{dependency}' is absent from Cargo.lock"),
                })?;
        if candidates.len() == 1 {
            return Ok(candidates[0].clone());
        }
        let requirement =
            self.workspace_requirements
                .get(dependency)
                .ok_or_else(|| ToolError::Packaging {
                    crate_name: dependency.to_owned(),
                    reason: format!(
                        "dependency '{dependency}' resolves to {} versions in Cargo.lock and the \
                     workspace requirement is unknown",
                        candidates.len()
                    ),
                })?;
        let requirement = VersionReq::parse(requirement).map_err(|_| ToolError::Packaging {
            crate_name: dependency.to_owned(),
            reason: format!(
                "workspace requirement '{requirement}' is not a valid version requirement"
            ),
        })?;
        let mut matching = Vec::new();
        for candidate in candidates {
            if let Ok(parsed) = Version::parse(candidate) {
                if requirement.matches(&parsed) {
                    matching.push(candidate.clone());
                }
            }
        }
        if matching.len() != 1 {
            return Err(ToolError::Packaging {
                crate_name: dependency.to_owned(),
                reason: format!(
                    "Cargo.lock holds {} candidate versions for '{dependency}' ({candidates:?}); \
                     requirement '{requirement}' must select exactly one",
                    candidates.len()
                ),
            });
        }
        Ok(matching[0].clone())
    }
}

fn read_file(path: &Path) -> ToolResult<String> {
    let shown = path.display().to_string();
    std::fs::read_to_string(path).map_err(|source| ToolError::Io {
        path: shown,
        source,
    })
}

pub(crate) fn required(
    table: &BTreeMap<String, String>,
    key: &str,
    where_: &str,
) -> ToolResult<String> {
    table
        .get(key)
        .cloned()
        .ok_or_else(|| ToolError::Usage(format!("{where_} lacks '{key}'")))
}

/// Collects `key = value` pairs of one `[section]` header.
///
/// The `Result` is deliberate even though this scanner cannot fail today: the
/// callers all treat repository parsing as fallible, and a stricter scanner
/// added later must not force every call site to change.
#[allow(clippy::unnecessary_wraps)]
///
/// This is intentionally a narrow scanner for this repository's own manifests:
/// it understands `key = "value"` and `key = { ... }` (stored as the raw right
/// hand side). Anything it cannot classify is ignored rather than guessed, and
/// callers fail closed when a key they need is missing.
pub(crate) fn parse_section(content: &str, wanted: &str) -> ToolResult<BTreeMap<String, String>> {
    let mut pairs = BTreeMap::new();
    let mut inside = false;
    for raw_line in content.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            inside = header.trim_matches('"') == wanted;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().trim_matches('"').to_owned();
            let value = value.trim();
            let value = if value.starts_with('{') {
                value.to_owned()
            } else {
                value.trim_matches('"').to_owned()
            };
            pairs.insert(key, value);
        }
    }
    Ok(pairs)
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(head, _)| head)
}

/// Pulls `version = "x"` out of an inline table, or returns the plain value.
fn extract_versions(pairs: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut requirements = BTreeMap::new();
    for (key, value) in pairs {
        if value.starts_with('{') {
            if let Some(found) = inline_value(value, "version") {
                requirements.insert(key.clone(), found);
            }
        } else if !value.is_empty() {
            requirements.insert(key.clone(), value.clone());
        }
    }
    requirements
}

fn inline_value(table: &str, wanted: &str) -> Option<String> {
    for part in table.split(',') {
        if let Some((key, value)) = part.split_once('=') {
            if key.trim() == wanted {
                return Some(value.trim().trim_matches('"').to_owned());
            }
        }
    }
    None
}

#[allow(clippy::unnecessary_wraps)]
fn parse_lock_versions(content: &str) -> ToolResult<BTreeMap<String, Vec<String>>> {
    let mut versions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line == "[[package]]" {
            if let (Some(name), Some(version)) = (name.take(), version.take()) {
                versions.entry(name).or_default().push(version);
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            if key == "name" {
                name = Some(value.to_owned());
            } else if key == "version" {
                version = Some(value.to_owned());
            }
        }
    }
    if let (Some(name), Some(version)) = (name, version) {
        versions.entry(name).or_default().push(version);
    }
    for list in versions.values_mut() {
        list.sort();
        list.dedup();
    }
    Ok(versions)
}

/// The commit `HEAD` resolves to, asked of Git itself.
///
/// This used to parse `.git/HEAD`, `refs/` and `packed-refs` by hand. That parser
/// is the reason a pinned revision could not be trusted to mean "commit Git also
/// means": it disagreed with Git for a linked worktree whose `gitdir:` pointer is
/// relative, and for any ref layout it did not know. `git rev-parse HEAD` answers
/// the same question for a normal checkout, a detached `HEAD` and a linked
/// worktree, and it fails loudly instead of guessing.
fn read_git_revision(root: &Path) -> ToolResult<String> {
    crate::git::Git::open(root)?.head()
}
