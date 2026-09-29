//! Binding a candidate to the commit its packed inputs were read from.
//!
//! A pinned `source_revision` is only a claim. It becomes checkable when three
//! things hold, all of them answered against Git rather than against the working
//! tree alone:
//!
//! 1. the revision names a real commit object (see [`Provenance::open`]);
//! 2. every payload member that is a verbatim copy of a repository file equals
//!    that file's committed blob ([`Provenance::compare_payload`]);
//! 3. every repository file that only *feeds* a derived record (the workspace
//!    manifest, the lock file, the toolchain declaration, the crate manifests
//!    before packing, the migration catalog files) still equals its committed
//!    blob ([`Provenance::compare_sources`]), so a derived record cannot be
//!    produced from modified-then-uncommitted input and still claim the commit.
//!
//! Normalisation is the payload's own ([`normalize_text_bytes`]), which is what
//! lets a `core.autocrlf` checkout be compared with committed bytes at all.
//!
//! The toolchain field is verified as a **declaration** check only: it proves the
//! artifact agrees with `rust-toolchain.toml` at that commit, not that this
//! process ran on that toolchain (the run logs in the evidence directory carry
//! that part).

use std::path::Path;

use crate::canonical::{normalize_text_bytes, sha256_hex};
use crate::git::Git;
use crate::repo::parse_section;
use crate::ToolResult;

/// Check identifier used when the pinned revision is not a commit.
pub const CHECK_REVISION_EXISTS: &str = "source-revision-exists";
/// Check identifier used when a packed input does not match the pinned commit.
pub const CHECK_INPUTS: &str = "source-inputs-match-revision";
/// Check identifier used for the toolchain *declaration* comparison.
pub const CHECK_TOOLCHAIN: &str = "toolchain-declaration";

/// One repository file compared against its committed blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceInput {
    /// Repository-relative POSIX path of the file as committed.
    pub repo_path: String,
    /// Human-readable origin of the compared bytes (a payload member, or
    /// `working tree` for a file that only feeds a derived record).
    pub origin: String,
    /// The bytes that were actually used.
    pub bytes: Vec<u8>,
}

impl ProvenanceInput {
    /// A payload member copied verbatim from `repo_path`.
    #[must_use]
    pub fn from_member(repo_path: String, member: &str, bytes: Vec<u8>) -> Self {
        Self {
            repo_path,
            origin: format!("package/{member}"),
            bytes,
        }
    }

    /// A repository file that feeds a derived record.
    #[must_use]
    pub fn from_working_tree(repo_path: String, bytes: Vec<u8>) -> Self {
        Self {
            repo_path,
            origin: "working tree".to_owned(),
            bytes,
        }
    }
}

/// The pinned commit, resolved by Git.
#[derive(Debug, Clone)]
pub struct Provenance {
    git: Git,
    revision: String,
}

impl Provenance {
    /// Resolves `revision` in `root`, refusing a revision Git does not have.
    pub fn open(root: &Path, revision: &str) -> ToolResult<Self> {
        let git = Git::open(root)?;
        if !git.has_commit(revision)? {
            return Err(crate::ToolError::Verification {
                check: CHECK_REVISION_EXISTS.to_owned(),
                reason: format!(
                    "the artifact pins source_revision '{revision}' but Git has no such commit \
                     object in this repository, so no claim about where these bytes came from \
                     can be verified"
                ),
            });
        }
        Ok(Self {
            git,
            revision: revision.to_owned(),
        })
    }

    /// The pinned commit itself.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Compares every verbatim payload member with its blob at the pinned commit.
    pub fn compare_payload(&self, inputs: &[ProvenanceInput]) -> ToolResult<usize> {
        self.compare(inputs, "payload member")
    }

    /// Compares every feeding file with its blob at the pinned commit.
    pub fn compare_sources(&self, inputs: &[ProvenanceInput]) -> ToolResult<usize> {
        self.compare(inputs, "input file")
    }

    fn compare(&self, inputs: &[ProvenanceInput], kind: &str) -> ToolResult<usize> {
        for input in inputs {
            let blob = self.git.blob(&self.revision, &input.repo_path)?;
            let Some(blob) = blob else {
                return Err(crate::ToolError::Verification {
                    check: CHECK_INPUTS.to_owned(),
                    reason: format!(
                        "{kind} '{origin}' comes from '{path}', which commit '{}' does not \
                         contain; the artifact cannot claim that revision",
                        self.revision,
                        origin = input.origin,
                        path = input.repo_path,
                    ),
                });
            };
            let expected = normalize_text_bytes(&blob, &input.repo_path)?;
            let observed = normalize_text_bytes(&input.bytes, &input.origin)?;
            if expected != observed {
                return Err(crate::ToolError::Verification {
                    check: CHECK_INPUTS.to_owned(),
                    reason: format!(
                        "{kind} '{origin}' does not match '{path}' as committed in '{}': \
                         committed {want}, packed {got}; the packed inputs must be the ones the \
                         pinned revision contains",
                        self.revision,
                        origin = input.origin,
                        path = input.repo_path,
                        want = sha256_hex(&expected),
                        got = sha256_hex(&observed),
                    ),
                });
            }
        }
        Ok(inputs.len())
    }

    /// The toolchain channel declared by `rust-toolchain.toml` at the pinned
    /// commit — a declaration comparison, never a claim about this process.
    pub fn declared_toolchain_channel(&self) -> ToolResult<String> {
        let blob = self.git.blob(&self.revision, "rust-toolchain.toml")?;
        let Some(blob) = blob else {
            return Err(crate::ToolError::Verification {
                check: CHECK_TOOLCHAIN.to_owned(),
                reason: format!(
                    "'rust-toolchain.toml' is absent from commit '{}', so the toolchain the \
                     artifact declares cannot be traced to a declaration",
                    self.revision
                ),
            });
        };
        let text = String::from_utf8_lossy(&blob).into_owned();
        let section = parse_section(&text, "toolchain")?;
        crate::repo::required(
            &section,
            "channel",
            "rust-toolchain.toml [toolchain] at the pinned \
                                                     revision",
        )
    }
}
