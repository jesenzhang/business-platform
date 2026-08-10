//! Safety boundary for the PLAN-0009 legacy migration rehearsal.
//!
//! The rehearsal reads an existing legacy tree and writes only to an explicitly
//! isolated target workspace.  This crate deliberately exposes no write API for
//! the source tree; later inventory and mapping code must use the read-only
//! handle defined here.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Execution modes accepted by the rehearsal boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Read legacy input and write only isolated rehearsal output.
    Rehearsal,
    /// Production migration is intentionally not part of PLAN-0009.
    Production,
}

/// Errors raised before a rehearsal can access source or target paths.
#[derive(Debug, Error)]
pub enum BoundaryError {
    #[error("source root must be an existing directory")]
    SourceRootMissing,
    #[error("isolation root must be an existing directory")]
    IsolationRootMissing,
    #[error("target root must be an existing directory")]
    TargetRootMissing,
    #[error("source, isolation and target roots must be directories")]
    NotDirectory,
    #[error("production migration is forbidden by PLAN-0009")]
    ProductionMode,
    #[error("target root must be inside the configured isolation root")]
    TargetOutsideIsolation,
    #[error("source and target roots must be disjoint")]
    SourceTargetOverlap,
    #[error("at least one source root is required")]
    SourceRootsEmpty,
    #[error("source roots must not overlap")]
    SourceRootsOverlap,
    #[error("isolation root must not be inside the source root")]
    IsolationInsideSource,
    #[error("source path escapes the read-only source root")]
    SourcePathEscape,
    #[error("source file is missing")]
    SourceFileMissing,
    #[error("source file could not be opened read-only")]
    SourceOpen(#[source] io::Error),
    #[error("path could not be resolved")]
    PathResolution(#[source] io::Error),
}

/// Validated source/target boundary for one rehearsal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RehearsalBoundary {
    source_roots: Vec<PathBuf>,
    isolation_root: PathBuf,
    target_root: PathBuf,
}

impl RehearsalBoundary {
    /// Validate a read-only source and an isolated target before any work starts.
    pub fn validate(
        source_root: impl AsRef<Path>,
        isolation_root: impl AsRef<Path>,
        target_root: impl AsRef<Path>,
        mode: ExecutionMode,
    ) -> Result<Self, BoundaryError> {
        Self::validate_sources([source_root.as_ref()], isolation_root, target_root, mode)
    }

    /// Validate multiple source roots, such as a legacy repository and a
    /// separately configured DATA_ROOT, against one isolated target.
    pub fn validate_sources<I, P>(
        source_roots: I,
        isolation_root: impl AsRef<Path>,
        target_root: impl AsRef<Path>,
        mode: ExecutionMode,
    ) -> Result<Self, BoundaryError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        if mode == ExecutionMode::Production {
            return Err(BoundaryError::ProductionMode);
        }

        let source_roots = source_roots
            .into_iter()
            .map(|source| existing_directory(source.as_ref(), RootKind::Source))
            .collect::<Result<Vec<_>, _>>()?;
        if source_roots.is_empty() {
            return Err(BoundaryError::SourceRootsEmpty);
        }
        let isolation_root = existing_directory(isolation_root.as_ref(), RootKind::Isolation)?;
        let target_root = existing_directory(target_root.as_ref(), RootKind::Target)?;

        for (index, source_root) in source_roots.iter().enumerate() {
            if isolation_root.starts_with(source_root) {
                return Err(BoundaryError::IsolationInsideSource);
            }
            if target_root.starts_with(source_root) || source_root.starts_with(&target_root) {
                return Err(BoundaryError::SourceTargetOverlap);
            }
            if source_roots
                .iter()
                .skip(index + 1)
                .any(|other| other.starts_with(source_root) || source_root.starts_with(other))
            {
                return Err(BoundaryError::SourceRootsOverlap);
            }
        }
        if !target_root.starts_with(&isolation_root) {
            return Err(BoundaryError::TargetOutsideIsolation);
        }

        Ok(Self {
            source_roots,
            isolation_root,
            target_root,
        })
    }

    /// Absolute, canonical source root.
    #[must_use]
    pub fn source_root(&self) -> &Path {
        &self.source_roots[0]
    }

    /// All canonical source roots protected by this boundary.
    #[must_use]
    pub fn source_roots(&self) -> &[PathBuf] {
        &self.source_roots
    }

    /// Absolute, canonical isolation root.
    #[must_use]
    pub fn isolation_root(&self) -> &Path {
        &self.isolation_root
    }

    /// Absolute, canonical target root.
    #[must_use]
    pub fn target_root(&self) -> &Path {
        &self.target_root
    }

    /// Create a read-only source handle after the boundary has been validated.
    #[must_use]
    pub fn read_only_source(&self) -> ReadOnlySource {
        ReadOnlySource {
            root: self.source_roots[0].clone(),
        }
    }

    /// Create a read-only source handle for a protected source root.
    #[must_use]
    pub fn read_only_source_at(&self, index: usize) -> Option<ReadOnlySource> {
        self.source_roots
            .get(index)
            .cloned()
            .map(|root| ReadOnlySource { root })
    }
}

/// Read-only view over the validated legacy source tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOnlySource {
    root: PathBuf,
}

impl ReadOnlySource {
    /// Open a source-relative file without creating or modifying it.
    pub fn open(&self, relative_path: impl AsRef<Path>) -> Result<File, BoundaryError> {
        let relative_path = relative_path.as_ref();
        if relative_path.is_absolute()
            || relative_path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(BoundaryError::SourcePathEscape);
        }

        let path = self.root.join(relative_path);
        let canonical = fs::canonicalize(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                BoundaryError::SourceFileMissing
            } else {
                BoundaryError::PathResolution(error)
            }
        })?;
        if !canonical.starts_with(&self.root) || !canonical.is_file() {
            return Err(BoundaryError::SourcePathEscape);
        }

        OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(canonical)
            .map_err(BoundaryError::SourceOpen)
    }

    /// Absolute, canonical source root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, Copy)]
enum RootKind {
    Source,
    Isolation,
    Target,
}

fn existing_directory(path: &Path, kind: RootKind) -> Result<PathBuf, BoundaryError> {
    let metadata = fs::metadata(path).map_err(|error| match kind {
        RootKind::Source => {
            if error.kind() == io::ErrorKind::NotFound {
                BoundaryError::SourceRootMissing
            } else {
                BoundaryError::PathResolution(error)
            }
        }
        RootKind::Isolation => {
            if error.kind() == io::ErrorKind::NotFound {
                BoundaryError::IsolationRootMissing
            } else {
                BoundaryError::PathResolution(error)
            }
        }
        RootKind::Target => {
            if error.kind() == io::ErrorKind::NotFound {
                BoundaryError::TargetRootMissing
            } else {
                BoundaryError::PathResolution(error)
            }
        }
    })?;
    if !metadata.is_dir() {
        return Err(BoundaryError::NotDirectory);
    }
    fs::canonicalize(path).map_err(BoundaryError::PathResolution)
}

/// The fixed rehearsal sample size required by PLAN-0009.
pub const REHEARSAL_SELECTION_LIMIT: usize = 120;

/// Stable classification values shared by inventory, planning, and replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InventoryClassification {
    Exact,
    Probable,
    Ambiguous,
    Conflict,
    Orphan,
    Missing,
    Rejected,
}

impl InventoryClassification {
    /// Return the contract-level spelling used in the frozen manifest.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "Exact",
            Self::Probable => "Probable",
            Self::Ambiguous => "Ambiguous",
            Self::Conflict => "Conflict",
            Self::Orphan => "Orphan",
            Self::Missing => "Missing",
            Self::Rejected => "Rejected",
        }
    }

    /// All classifications in their stable manifest and count order.
    pub const ALL: [Self; 7] = [
        Self::Exact,
        Self::Probable,
        Self::Ambiguous,
        Self::Conflict,
        Self::Orphan,
        Self::Missing,
        Self::Rejected,
    ];
}

/// Errors raised when the deterministic 120-contract selection cannot be made.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SelectionError {
    #[error("the source inventory contains fewer than {REHEARSAL_SELECTION_LIMIT} contracts")]
    TooFewContracts,
}

/// Select exactly 120 distinct contract identifiers in stable ascending order.
///
/// The caller supplies identifiers read from the authoritative source. The
/// function deliberately sorts and deduplicates before truncating so replay is
/// independent of database cursor order while still failing closed when the
/// source cannot satisfy the fixed rehearsal size.
pub fn select_rehearsal_contract_ids<I>(ids: I) -> Result<Vec<i64>, SelectionError>
where
    I: IntoIterator<Item = i64>,
{
    let mut ids = ids.into_iter().collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() < REHEARSAL_SELECTION_LIMIT {
        return Err(SelectionError::TooFewContracts);
    }
    ids.truncate(REHEARSAL_SELECTION_LIMIT);
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        select_rehearsal_contract_ids, BoundaryError, ExecutionMode, RehearsalBoundary,
        SelectionError, REHEARSAL_SELECTION_LIMIT,
    };

    static NEXT_CASE: AtomicUsize = AtomicUsize::new(0);

    struct Fixture {
        root: PathBuf,
        source: PathBuf,
        isolation: PathBuf,
        target: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "plan-0009-boundary-{}-{}",
                std::process::id(),
                NEXT_CASE.fetch_add(1, Ordering::Relaxed)
            ));
            let source = root.join("source");
            let isolation = root.join("isolation");
            let target = isolation.join("target");
            fs::create_dir_all(&target).expect("test fixture directories");
            fs::create_dir_all(source.join("data")).expect("test source directory");
            fs::write(source.join("data/input.txt"), b"read-only input").expect("test source file");
            Self {
                root,
                source,
                isolation,
                target,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn accepts_disjoint_target_inside_isolation() {
        let fixture = Fixture::new();
        let boundary = RehearsalBoundary::validate(
            &fixture.source,
            &fixture.isolation,
            &fixture.target,
            ExecutionMode::Rehearsal,
        )
        .expect("valid rehearsal boundary");
        assert_eq!(
            boundary.target_root().file_name().and_then(|v| v.to_str()),
            Some("target")
        );
    }

    #[test]
    fn rejects_target_inside_source() {
        let fixture = Fixture::new();
        let target = fixture.source.join("target");
        fs::create_dir_all(&target).expect("nested target");
        let error = RehearsalBoundary::validate(
            &fixture.source,
            &fixture.source,
            &target,
            ExecutionMode::Rehearsal,
        )
        .expect_err("source and target must not overlap");
        assert!(matches!(error, BoundaryError::IsolationInsideSource));
    }

    #[test]
    fn rejects_production_mode_before_path_access() {
        let fixture = Fixture::new();
        let error = RehearsalBoundary::validate(
            fixture.root.join("missing-source"),
            &fixture.isolation,
            &fixture.target,
            ExecutionMode::Production,
        )
        .expect_err("production migration is out of scope");
        assert!(matches!(error, BoundaryError::ProductionMode));
    }

    #[test]
    fn rejects_target_outside_isolation() {
        let fixture = Fixture::new();
        let outside = fixture.root.join("outside");
        fs::create_dir_all(&outside).expect("outside target");
        let error = RehearsalBoundary::validate(
            &fixture.source,
            &fixture.isolation,
            &outside,
            ExecutionMode::Rehearsal,
        )
        .expect_err("target must remain isolated");
        assert!(matches!(error, BoundaryError::TargetOutsideIsolation));
    }

    #[test]
    fn protects_multiple_disjoint_source_roots() {
        let fixture = Fixture::new();
        let secondary = fixture.root.join("data-root");
        fs::create_dir_all(&secondary).expect("secondary source root");
        let boundary = RehearsalBoundary::validate_sources(
            [fixture.source.as_path(), secondary.as_path()],
            &fixture.isolation,
            &fixture.target,
            ExecutionMode::Rehearsal,
        )
        .expect("multiple source roots are protected");
        assert_eq!(boundary.source_roots().len(), 2);
        assert!(boundary.read_only_source_at(1).is_some());
    }

    #[test]
    fn source_handle_opens_existing_file_without_write_api() {
        let fixture = Fixture::new();
        let boundary = RehearsalBoundary::validate(
            &fixture.source,
            &fixture.isolation,
            &fixture.target,
            ExecutionMode::Rehearsal,
        )
        .expect("valid rehearsal boundary");
        let mut file = boundary
            .read_only_source()
            .open("data/input.txt")
            .expect("read-only source file");
        let mut content = String::new();
        file.read_to_string(&mut content).expect("read source file");
        assert_eq!(content, "read-only input");
    }

    #[test]
    fn rehearsal_selection_is_sorted_distinct_and_exactly_120() {
        let ids = (1_i64..=121).rev().chain([1, 2, 3]);
        let selected = select_rehearsal_contract_ids(ids).expect("120 contracts");
        assert_eq!(selected.len(), REHEARSAL_SELECTION_LIMIT);
        assert_eq!(selected.first(), Some(&1));
        assert_eq!(selected.last(), Some(&120));
        assert!(selected.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn rehearsal_selection_fails_closed_when_source_is_too_small() {
        assert_eq!(
            select_rehearsal_contract_ids(1_i64..=119),
            Err(SelectionError::TooFewContracts)
        );
    }
}
