//! One fixed commit, used as the source of a candidate's inputs.
//!
//! Repair 1 proved that the packed bytes *equal* the blobs of the pinned commit.
//! That is not the same statement as "the candidate was produced *from* those
//! blobs": the derived members (the packed crate manifests, the shipped module
//! manifest, the contract surface and the migration mapping) were assembled from
//! whatever happened to be on disk and were then only re-compiled from the
//! candidate's own bytes, so a rewrite inside the artifact that left it
//! self-consistent was accepted, and a source file deleted from the checkout
//! disappeared from both sides of the comparison (review F1-A / F1-B).
//!
//! [`Inputs`] closes that gap structurally rather than by adding another check:
//! every input that *produces* a candidate, and every input `verify` re-derives a
//! candidate from, is read through this type. Reading it from a commit is the only
//! way to build a projection; the working-tree variant exists solely so that
//! generation can prove the checkout agrees with the commit before anything is
//! written, which is a different question from where the inputs came from.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::git::{Git, TreeEntry};
use crate::provenance::{
    ProvenanceInput, CHECK_INPUTS, CHECK_MEMBERS, CHECK_REVISION_EXISTS, CHECK_TOOLCHAIN,
};
use crate::repo::RepoContext;
use crate::spec::{self, ReleaseSpec};
use crate::{ToolError, ToolResult};

/// Where repository inputs are read from.
#[derive(Debug, Clone)]
pub enum Inputs {
    /// The bytes on disk below `root`. Only the generation pre-flight may use
    /// this variant, and only to show the checkout agrees with a commit.
    WorkingTree {
        /// Repository root.
        root: PathBuf,
    },
    /// The blobs one commit stores. Every input that reaches a payload, and every
    /// input a candidate is re-derived from during verification, comes from here.
    Commit {
        /// Read-only Git handle.
        git: Git,
        /// The commit every input is read out of.
        revision: String,
    },
}

impl Inputs {
    /// The files on disk under `root`.
    #[must_use]
    pub fn working_tree(root: &Path) -> Self {
        Self::WorkingTree {
            root: root.to_path_buf(),
        }
    }

    /// The blobs of `revision`, refusing a revision Git does not have.
    ///
    /// There is deliberately no fallback: if the commit object is not in this
    /// repository, the inputs cannot be read, and reading the working tree
    /// instead would be a different (and unprovable) claim.
    pub fn open_commit(root: &Path, revision: &str) -> ToolResult<Self> {
        let git = Git::open(root)?;
        if !git.has_commit(revision)? {
            return Err(ToolError::Verification {
                check: CHECK_REVISION_EXISTS.to_owned(),
                reason: format!(
                    "Git has no such commit object '{revision}' in this repository, so no claim \
                     about where these bytes came from can be verified"
                ),
            });
        }
        Ok(Self::Commit {
            git,
            revision: revision.to_owned(),
        })
    }

    /// The commit, when these inputs are a commit.
    #[must_use]
    pub fn revision(&self) -> Option<&str> {
        match self {
            Self::WorkingTree { .. } => None,
            Self::Commit { revision, .. } => Some(revision),
        }
    }

    /// How to name this source of inputs in a refusal.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::WorkingTree { root } => format!("the working tree at '{}'", root.display()),
            Self::Commit { revision, .. } => format!("commit '{revision}'"),
        }
    }

    /// The bytes stored at `repo_path`, refusing a path this source does not have.
    ///
    /// A missing input is reported under the input identifier: it is the same
    /// defect the Repair 1 review described as "the artifact claims a revision that
    /// does not contain its own inputs", whatever the reason the bytes are absent.
    pub fn bytes(&self, repo_path: &str) -> ToolResult<Vec<u8>> {
        match self {
            Self::WorkingTree { root } => {
                let path = root.join(repo_path.replace('/', "\\"));
                let shown = path.display().to_string();
                let metadata =
                    std::fs::symlink_metadata(&path).map_err(|source| ToolError::Io {
                        path: shown.clone(),
                        source,
                    })?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(ToolError::UnsupportedEntry { path: shown });
                }
                std::fs::read(&path).map_err(|source| ToolError::Io {
                    path: shown,
                    source,
                })
            }
            Self::Commit { git, revision } => {
                let path = repo_path.replace('\\', "/");
                git.blob(revision, &path)?
                    .ok_or_else(|| ToolError::Verification {
                        check: CHECK_INPUTS.to_owned(),
                        reason: format!(
                            "input '{path}' is required to rebuild this candidate, but commit \
                         '{revision}' does not contain it; the artifact cannot claim that \
                         revision",
                        ),
                    })
            }
        }
    }

    /// The same bytes as UTF-8 text.
    pub fn text(&self, repo_path: &str) -> ToolResult<String> {
        let path = repo_path.replace('\\', "/");
        let bytes = self.bytes(&path)?;
        String::from_utf8(bytes).map_err(|_| ToolError::NotUtf8 { path })
    }

    /// Every path stored under the directory `prefix`, as repository-relative
    /// POSIX paths.
    ///
    /// In a commit this is `git ls-tree -r -z`, and only blobs: a symlink
    /// (`120000`) or a gitlink (`160000`, i.e. a submodule) is refused instead of
    /// being packed, because neither can be checked out as the file the artifact
    /// would claim it is.
    pub fn paths_under(&self, prefix: &str) -> ToolResult<BTreeSet<String>> {
        let prefix = prefix.trim_end_matches('/');
        match self {
            Self::WorkingTree { root } => {
                let directory = root.join(prefix.replace('/', "\\"));
                let shown = directory.display().to_string();
                let metadata =
                    std::fs::symlink_metadata(&directory).map_err(|source| ToolError::Io {
                        path: shown.clone(),
                        source,
                    })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(ToolError::UnsupportedEntry { path: shown });
                }
                let mut found = BTreeSet::new();
                collect_disk(&directory, root, &mut found)?;
                Ok(found)
            }
            Self::Commit { git, revision } => {
                if git.tree_object(revision, prefix)?.is_none() {
                    return Err(ToolError::Verification {
                        check: CHECK_MEMBERS.to_owned(),
                        reason: format!(
                            "commit '{revision}' does not contain the directory '{prefix}', so \
                             the member set of this candidate cannot be derived from the \
                             revision it pins"
                        ),
                    });
                }
                let mut found = BTreeSet::new();
                for entry in git.tree(revision, prefix)? {
                    reject_unpackable(&entry, revision)?;
                    found.insert(entry.path);
                }
                Ok(found)
            }
        }
    }

    /// The repository facts, read from these inputs rather than from a checkout.
    pub fn repo_context(&self, root: &Path) -> ToolResult<RepoContext> {
        let workspace = self.text("Cargo.toml")?;
        let lock = self.text("Cargo.lock")?;
        let toolchain = self.text("rust-toolchain.toml")?;
        let revision = match self {
            Self::Commit { revision, .. } => revision.clone(),
            Self::WorkingTree { .. } => crate::repo::read_git_revision(root)?,
        };
        RepoContext::assemble(root.to_path_buf(), revision, &workspace, &lock, &toolchain)
    }

    /// The release spec of one module, as these inputs declare it.
    pub fn release_spec(&self, module: &str) -> ToolResult<ReleaseSpec> {
        let path = spec::spec_path(module);
        let declared = self
            .revision()
            .map_or_else(|| path.clone(), |revision| format!("{revision}:{path}"));
        ReleaseSpec::from_json(&self.text(&path)?, module, &declared)
    }

    /// Compares bytes that were used somewhere against what these inputs store at
    /// the same repository path, and returns how many were compared.
    ///
    /// `kind` names the category in the refusal (`payload member`, `input file`) so
    /// a failure says which side of the artifact was found to disagree. Both sides
    /// are normalised the way the payload is normalised, which is the only reason a
    /// `core.autocrlf` checkout can be compared with committed bytes at all.
    pub fn compare(&self, kind: &str, given: &[ProvenanceInput]) -> ToolResult<usize> {
        let revision = self.revision().ok_or_else(|| {
            ToolError::Usage(format!(
                "comparing inputs against a commit needs a commit, and these inputs are {}",
                self.label()
            ))
        })?;
        for input in given {
            let blob = self.bytes_or_none(&input.repo_path)?;
            let Some(blob) = blob else {
                return Err(ToolError::Verification {
                    check: CHECK_INPUTS.to_owned(),
                    reason: format!(
                        "{kind} '{origin}' comes from '{path}', which commit '{revision}' does \
                         not contain; the artifact cannot claim that revision",
                        origin = input.origin,
                        path = input.repo_path,
                    ),
                });
            };
            let expected = crate::canonical::normalize_text_bytes(&blob, &input.repo_path)?;
            let observed = crate::canonical::normalize_text_bytes(&input.bytes, &input.origin)?;
            if expected != observed {
                return Err(ToolError::Verification {
                    check: CHECK_INPUTS.to_owned(),
                    reason: format!(
                        "{kind} '{origin}' does not match '{path}' as committed in \
                         '{revision}': committed {want}, packed {got}; the packed inputs must be \
                         the ones the pinned revision contains",
                        origin = input.origin,
                        path = input.repo_path,
                        want = crate::canonical::sha256_hex(&expected),
                        got = crate::canonical::sha256_hex(&observed),
                    ),
                });
            }
        }
        Ok(given.len())
    }

    /// Like [`Inputs::bytes`], but a path the commit does not store is an ordinary
    /// empty answer rather than a refusal — the caller decides what absence means.
    fn bytes_or_none(&self, repo_path: &str) -> ToolResult<Option<Vec<u8>>> {
        match self {
            Self::WorkingTree { .. } => self.bytes(repo_path).map(Some),
            Self::Commit { git, revision } => git.blob(revision, &repo_path.replace('\\', "/")),
        }
    }

    /// The toolchain channel declared by `rust-toolchain.toml` in these inputs.
    pub fn declared_toolchain_channel(&self) -> ToolResult<String> {
        let text = match self.bytes_or_none("rust-toolchain.toml")? {
            Some(blob) => String::from_utf8_lossy(&blob).into_owned(),
            None => {
                return Err(ToolError::Verification {
                    check: CHECK_TOOLCHAIN.to_owned(),
                    reason: format!(
                        "'rust-toolchain.toml' is absent from {}, so the toolchain the artifact \
                         declares cannot be traced to a declaration",
                        self.label()
                    ),
                })
            }
        };
        let section = crate::repo::parse_section(&text, "toolchain")?;
        crate::repo::required(
            &section,
            "channel",
            &format!("rust-toolchain.toml [toolchain] at {}", self.label()),
        )
    }
}

/// Refuses a tree entry that cannot be shipped as a file.
fn reject_unpackable(entry: &TreeEntry, revision: &str) -> ToolResult<()> {
    if entry.kind == "blob" && entry.mode != "120000" {
        return Ok(());
    }
    let what = if entry.mode == "160000" || entry.kind == "commit" {
        "a gitlink (submodule)"
    } else if entry.mode == "120000" || entry.kind == "symlink" {
        "a symbolic link"
    } else {
        "not a plain file"
    };
    Err(ToolError::Verification {
        check: CHECK_MEMBERS.to_owned(),
        reason: format!(
            "'{path}' is {what} in commit '{revision}', not a file; a candidate may only pack \
             plain committed files",
            path = entry.path,
        ),
    })
}

fn collect_disk(directory: &Path, root: &Path, found: &mut BTreeSet<String>) -> ToolResult<()> {
    let shown = directory.display().to_string();
    let read = std::fs::read_dir(directory).map_err(|source| ToolError::Io {
        path: shown.clone(),
        source,
    })?;
    for entry in read {
        let path = entry
            .map_err(|source| ToolError::Io {
                path: shown.clone(),
                source,
            })?
            .path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if metadata.is_dir() {
            collect_disk(&path, root, found)?;
            continue;
        }
        // A symbolic link or a special entry is refused rather than followed: the
        // payload can only contain a plain file whose bytes the commit can vouch for.
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ToolError::UnsupportedEntry {
                path: path.display().to_string(),
            });
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ToolError::InvalidMemberPath {
                path: path.display().to_string(),
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let _ = found.insert(relative);
    }
    Ok(())
}
