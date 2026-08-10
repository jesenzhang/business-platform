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
    source_root: PathBuf,
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
        if mode == ExecutionMode::Production {
            return Err(BoundaryError::ProductionMode);
        }

        let source_root = existing_directory(source_root.as_ref(), RootKind::Source)?;
        let isolation_root = existing_directory(isolation_root.as_ref(), RootKind::Isolation)?;
        let target_root = existing_directory(target_root.as_ref(), RootKind::Target)?;

        if isolation_root.starts_with(&source_root) {
            return Err(BoundaryError::IsolationInsideSource);
        }
        if target_root.starts_with(&source_root) || source_root.starts_with(&target_root) {
            return Err(BoundaryError::SourceTargetOverlap);
        }
        if !target_root.starts_with(&isolation_root) {
            return Err(BoundaryError::TargetOutsideIsolation);
        }

        Ok(Self {
            source_root,
            isolation_root,
            target_root,
        })
    }

    /// Absolute, canonical source root.
    #[must_use]
    pub fn source_root(&self) -> &Path {
        &self.source_root
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
            root: self.source_root.clone(),
        }
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{BoundaryError, ExecutionMode, RehearsalBoundary};

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
}
