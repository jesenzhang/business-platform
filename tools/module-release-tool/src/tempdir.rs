//! Scratch directories for tests and for local verification runs.
//!
//! The tool itself never writes outside a caller-provided output path; this
//! helper exists so tests and scripts get a scratch directory that is created
//! below the repository build directory (same volume, already ignored by git),
//! and that is **removed when the guard is dropped** — including on a test
//! failure. Set `MODULE_RELEASE_TMP` to put scratch space elsewhere.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{ToolError, ToolResult};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A scratch directory that deletes itself.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a scratch directory below the repository build directory.
    pub fn new(tag: &str) -> ToolResult<Self> {
        Self::in_base(&default_base(), tag)
    }

    /// Creates a scratch directory below an explicit base directory.
    pub fn in_base(base: &Path, tag: &str) -> ToolResult<Self> {
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = base.join(format!("{}-{}-{serial}", std::process::id(), sanitize(tag)));
        if path.exists() {
            std::fs::remove_dir_all(&path).map_err(|source| ToolError::Io {
                path: path.display().to_string(),
                source,
            })?;
        }
        std::fs::create_dir_all(&path).map_err(|source| ToolError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Self { path })
    }

    /// The scratch directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn default_base() -> PathBuf {
    match std::env::var("MODULE_RELEASE_TMP") {
        Ok(value) if !value.is_empty() => PathBuf::from(value),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/module-release-tool"),
    }
}

fn sanitize(tag: &str) -> String {
    tag.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect()
}
