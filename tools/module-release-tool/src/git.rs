//! Git metadata access for source provenance.
//!
//! The artifact pins a commit, so "which commit" and "were the packed inputs
//! really read from that commit" have to be answered by Git itself rather than by
//! hand-parsing `HEAD`, `refs/` or `packed-refs` — the latter silently disagrees
//! with Git in a linked worktree, a shallow clone, or as soon as a ref lives in a
//! ref store the parser does not know about.
//!
//! Every call goes through [`Git::run`], which builds an argument array
//! (`Command::args`, never a shell string), pins the working directory with
//! `-C <root>` and passes `--no-optional-locks` so verification never writes
//! repository locks. Only read-only subcommands are used: `rev-parse` and
//! `cat-file`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{ToolError, ToolResult};

/// A read-only handle on the Git repository one candidate was built from.
#[derive(Debug, Clone)]
pub struct Git {
    root: PathBuf,
}

impl Git {
    /// Opens `root` and confirms Git considers it a work tree.
    pub fn open(root: &Path) -> ToolResult<Self> {
        let git = Self {
            root: root.to_path_buf(),
        };
        let inside = git.run(&["rev-parse", "--is-inside-work-tree"])?;
        if String::from_utf8_lossy(&inside.stdout).trim() != "true" {
            return Err(ToolError::Git {
                command: "rev-parse --is-inside-work-tree".to_owned(),
                reason: format!("'{}' is not a Git work tree", root.display()),
            });
        }
        Ok(git)
    }

    /// Runs one Git command in this repository and returns its raw output.
    ///
    /// A non-zero exit is *data*, not an error, unless Git could not be started
    /// at all: callers decide what a failed `cat-file` means.
    fn run(&self, arguments: &[&str]) -> ToolResult<std::process::Output> {
        let displayed = format!("git -C {} {}", self.root.display(), arguments.join(" "));
        Command::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(&self.root)
            .args(arguments)
            .output()
            .map_err(|source| ToolError::Git {
                command: displayed,
                reason: source.to_string(),
            })
    }

    /// Runs one Git command that must succeed, returning trimmed stdout.
    fn run_checked(&self, arguments: &[&str]) -> ToolResult<String> {
        let displayed = arguments.join(" ");
        let output = self.run(arguments)?;
        if !output.status.success() {
            return Err(ToolError::Git {
                command: displayed,
                reason: format!(
                    "exit {:?}: {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    /// The commit `HEAD` points at — identical for a normal checkout, a detached
    /// `HEAD` and a linked worktree, because Git resolves it.
    pub fn head(&self) -> ToolResult<String> {
        self.run_checked(&["rev-parse", "HEAD"])
    }

    /// Whether `revision` names a commit object in this repository.
    ///
    /// A 40-hex-digit string is only a *shape*; this asks Git whether the object
    /// exists, which is what makes a pinned revision checkable at all.
    pub fn has_commit(&self, revision: &str) -> ToolResult<bool> {
        let probe = format!("cat-file -e {revision}^{{commit}}");
        let output = self.run(&["cat-file", "-e", &format!("{revision}^{{commit}}")])?;
        if output.status.success() {
            return Ok(true);
        }
        // A malformed object name is a definite "no"; anything else (broken
        // repository, unusable object database) must not be reported as absent.
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("not a valid object name")
            || stderr.contains("bad object")
            || stderr.contains("unknown revision")
        {
            return Ok(false);
        }
        Err(ToolError::Git {
            command: probe,
            reason: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    /// The blob of `repo_path` as it exists in `revision`, or `None` when the
    /// commit exists but does not contain that path.
    ///
    /// `cat-file blob` returns the *committed* bytes, which is what a
    /// `core.autocrlf` checkout can be compared against once both sides are
    /// normalised the way the payload is normalised.
    pub fn blob(&self, revision: &str, repo_path: &str) -> ToolResult<Option<Vec<u8>>> {
        let spec = format!("{revision}:{}", repo_path.replace('\\', "/"));
        let located = self.run(&["rev-parse", "--quiet", "--verify", &spec])?;
        if !located.status.success() {
            return Ok(None);
        }
        let oid = String::from_utf8_lossy(&located.stdout).trim().to_owned();
        if oid.is_empty() {
            return Ok(None);
        }
        let blob = self.run(&["cat-file", "blob", &oid])?;
        if !blob.status.success() {
            return Err(ToolError::Git {
                command: format!("cat-file blob {oid}"),
                reason: String::from_utf8_lossy(&blob.stderr).trim().to_owned(),
            });
        }
        Ok(Some(blob.stdout))
    }
}
