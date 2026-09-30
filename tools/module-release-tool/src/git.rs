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
//! repository locks. Only read-only subcommands are used: `rev-parse`, `cat-file`
//! and `ls-tree`.
//!
//! The listing and the bytes are read separately and deliberately: `ls-tree -r -z`
//! answers *which* members a commit contains (the `-z` form cannot be confused by
//! a path containing a space, a tab or a newline, and it names the entry type, so a
//! symlink or a submodule cannot pass as a file), and `cat-file blob` answers what
//! one of those members *holds*. Neither `git show` nor a working-tree listing can
//! stand in for either question: the former mixes the two answers into a
//! human-readable dump, the latter answers a different question entirely.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{ToolError, ToolResult};

/// A read-only handle on the Git repository one candidate was built from.
#[derive(Debug, Clone)]
pub struct Git {
    root: PathBuf,
}

/// One entry of a commit tree, exactly as `git ls-tree` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// The mode string, e.g. `100644`, `100755`, `120000` (symlink), `160000`
    /// (gitlink).
    pub mode: String,
    /// The object type: `blob`, `tree` or `commit`.
    pub kind: String,
    /// The object id.
    pub object: String,
    /// Repository-relative POSIX path.
    pub path: String,
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

    /// Whether `revision` stores a tree (directory) at `repo_path`.
    ///
    /// A missing path and a path that is not a directory are distinguished from an
    /// empty directory, which is a legal tree with no entries: `ls-tree` alone
    /// cannot tell those three apart, and the difference decides whether a
    /// candidate's member set is *empty* or *unknown*.
    pub fn tree_object(&self, revision: &str, repo_path: &str) -> ToolResult<Option<String>> {
        let spec = format!("{revision}:{}", repo_path.replace('\\', "/"));
        let located = self.run(&["rev-parse", "--quiet", "--verify", &spec])?;
        if !located.status.success() {
            return Ok(None);
        }
        let oid = String::from_utf8_lossy(&located.stdout).trim().to_owned();
        if oid.is_empty() {
            return Ok(None);
        }
        let kind = self.run(&["cat-file", "-t", &oid])?;
        if !kind.status.success() {
            return Err(ToolError::Git {
                command: format!("cat-file -t {oid}"),
                reason: String::from_utf8_lossy(&kind.stderr).trim().to_owned(),
            });
        }
        let kind = String::from_utf8_lossy(&kind.stdout).trim().to_owned();
        Ok((kind == "tree").then_some(oid))
    }

    /// Every entry under `repo_path` in `revision`, recursively.
    ///
    /// `-z` is what makes this parseable without guessing: entries are separated
    /// by NUL, so a path containing whitespace survives intact, and each entry
    /// still carries its mode and object type for the caller to judge.
    /// `--full-name` keeps the paths repository-relative no matter where the
    /// process happens to be running.
    pub fn tree(&self, revision: &str, repo_path: &str) -> ToolResult<Vec<TreeEntry>> {
        let prefix = repo_path.replace('\\', "/");
        let listing = self.run(&[
            "ls-tree",
            "-r",
            "-z",
            "--full-name",
            revision,
            "--",
            &prefix,
        ])?;
        if !listing.status.success() {
            return Err(ToolError::Git {
                command: format!("ls-tree -r -z --full-name {revision} -- {prefix}"),
                reason: String::from_utf8_lossy(&listing.stderr).trim().to_owned(),
            });
        }
        let mut entries = Vec::new();
        for record in listing.stdout.split(|byte| *byte == 0) {
            if record.is_empty() {
                continue;
            }
            entries.push(parse_entry(record, &prefix)?);
        }
        entries.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
        Ok(entries)
    }
}

/// Splits one `ls-tree` record, `<mode> <type> <object>\t<path>`.
fn parse_entry(record: &[u8], prefix: &str) -> ToolResult<TreeEntry> {
    let shown = String::from_utf8_lossy(record).into_owned();
    let bad = || ToolError::Git {
        command: format!("ls-tree -r -z … -- {prefix}"),
        reason: format!("unparsable tree entry '{shown}'"),
    };
    let (header, path) = shown.split_once('\t').ok_or_else(bad)?;
    let mut fields = header.split(' ');
    let mode = fields.next().ok_or_else(bad)?.to_owned();
    let kind = fields.next().ok_or_else(bad)?.to_owned();
    let object = fields.next().ok_or_else(bad)?.to_owned();
    if path.is_empty() || mode.is_empty() || kind.is_empty() || object.is_empty() {
        return Err(bad());
    }
    Ok(TreeEntry {
        mode,
        kind,
        object,
        path: path.to_owned(),
    })
}
