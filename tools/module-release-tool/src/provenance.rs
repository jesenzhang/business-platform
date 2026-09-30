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
//! Reading the commit is [`crate::commit::Inputs`]'s job; this module owns the two
//! *comparisons* a candidate is bound by, and the identifiers that name them, so
//! there is exactly one place that decides what "these bytes are that commit's
//! bytes" means. Repair 2 added the two identifiers that name what the first two
//! comparisons do **not** cover: which members exist ([`CHECK_MEMBERS`]) and what
//! the members that are *derived* rather than copied actually are
//! ([`CHECK_DERIVED`]).
//!
//! Normalisation is the payload's own ([`normalize_text_bytes`]), which is what
//! lets a `core.autocrlf` checkout be compared with committed bytes at all.
//!
//! The toolchain field is verified as a **declaration** check only: it proves the
//! artifact agrees with `rust-toolchain.toml` at that commit, not that this
//! process ran on that toolchain (the run logs in the evidence directory carry
//! that part).

use std::path::Path;

use crate::commit::Inputs;
use crate::ToolResult;

/// Check identifier used when the pinned revision is not a commit.
pub const CHECK_REVISION_EXISTS: &str = "source-revision-exists";
/// Check identifier used when a packed input does not match the pinned commit.
pub const CHECK_INPUTS: &str = "source-inputs-match-revision";
/// Check identifier used for the toolchain *declaration* comparison.
pub const CHECK_TOOLCHAIN: &str = "toolchain-declaration";
/// Check identifier used when the payload's members are not the members the
/// pinned commit's tree produces — a dropped, missing or extra member.
///
/// Separate from [`CHECK_INPUTS`] on purpose: that one compares bytes that are
/// present, this one compares *which* members exist. A candidate can pass one and
/// fail the other, and a report that conflates them cannot say what was wrong.
pub const CHECK_MEMBERS: &str = "source-member-set-matches-revision";
/// Check identifier used when a member that is *derived* rather than copied —
/// a packed crate manifest, the shipped module manifest, the contract surface or
/// the migration mapping — is not what the pinned commit produces.
///
/// Separate from `artifact-integrity-digest`, which only proves the payload still
/// matches its own record. Repair 1's finding F1-A was exactly a candidate that
/// passed the physical check and had never been derived from the commit it pinned.
pub const CHECK_DERIVED: &str = "derived-members-match-revision";

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
    inputs: Inputs,
    revision: String,
}

impl Provenance {
    /// Resolves `revision` in `root`, refusing a revision Git does not have.
    pub fn open(root: &Path, revision: &str) -> ToolResult<Self> {
        let inputs = Inputs::open_commit(root, revision)?;
        Ok(Self {
            inputs,
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
        self.inputs.compare("payload member", inputs)
    }

    /// Compares every feeding file with its blob at the pinned commit.
    pub fn compare_sources(&self, inputs: &[ProvenanceInput]) -> ToolResult<usize> {
        self.inputs.compare("input file", inputs)
    }

    /// The toolchain channel declared by `rust-toolchain.toml` at the pinned
    /// commit — a declaration comparison, never a claim about this process.
    pub fn declared_toolchain_channel(&self) -> ToolResult<String> {
        self.inputs.declared_toolchain_channel()
    }
}
