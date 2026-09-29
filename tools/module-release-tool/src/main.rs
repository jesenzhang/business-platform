//! `module-release` — local release-candidate CLI (PLAN-0014 Wave 5A).
//!
//! ```text
//! module-release generate --module <id> --out <dir> [--repo <dir>]
//! module-release verify   --artifact <dir>            [--repo <dir>]
//! module-release compare  --left <dir> --right <dir>  [--repo <dir>]
//! ```
//!
//! Any failure exits non-zero. There is no path in this binary that reports a
//! failed check as success, and no command publishes, registers or installs
//! anything.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use module_release_tool::packaging;
use module_release_tool::repo::RepoContext;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("module-release: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<(), module_release_tool::ToolError> {
    let Some(command) = arguments.first() else {
        return Err(module_release_tool::ToolError::Usage(usage().to_owned()));
    };
    let options = Options::parse(&arguments[1..])?;
    let context = match &options.repo {
        Some(root) => RepoContext::with_root(root.clone())?,
        None => RepoContext::discover()?,
    };
    match command.as_str() {
        "generate" => {
            let module = options.text("module")?;
            let out = options.required("out")?;
            let artifact = packaging::generate(&context, module, out)?;
            println!("status=generated");
            println!("module_id={}", artifact.module_id);
            println!("module_version={}", artifact.module_version);
            println!("directory={}", artifact.directory.display());
            println!("members={}", artifact.member_count);
            println!("contract_digest={}", artifact.contract_digest);
            println!("package_digest={}", artifact.package_digest);
            println!(
                "artifact_integrity_digest={}",
                artifact.artifact_integrity_digest
            );
            println!("source_revision={}", context.revision());
            println!(
                "toolchain={} {}",
                context.build_toolchain().toolchain_id,
                context.build_toolchain().version
            );
            println!("platform_version={}", context.platform_version());
            Ok(())
        }
        "verify" => {
            let artifact = options.required("artifact")?;
            let index = packaging::verify(&context, artifact)?;
            println!("status=verified");
            println!("module_id={}", index.module_id);
            println!("module_version={}", index.module_version);
            println!(
                "artifact_integrity_digest={}",
                index.artifact_integrity_digest
            );
            for check in &index.checks {
                println!(
                    "check[{}] {check}: {detail}",
                    check.id,
                    detail = check.outcome,
                    check = check.description.replace('\n', " ")
                );
            }
            Ok(())
        }
        "compare" => {
            let left = options.required("left")?;
            let right = options.required("right")?;
            let comparison = packaging::compare(left, right)?;
            println!(
                "status={}",
                if comparison.identical {
                    "identical"
                } else {
                    "different"
                }
            );
            println!("left={}", left.display());
            println!("right={}", right.display());
            println!("left_digest={}", comparison.left_digest);
            println!("right_digest={}", comparison.right_digest);
            for difference in &comparison.differences {
                println!("difference={difference}");
            }
            if comparison.identical {
                Ok(())
            } else {
                Err(module_release_tool::ToolError::Verification {
                    check: "repeat-build-consistency".to_owned(),
                    reason: format!(
                        "{} difference(s) between the two candidates",
                        comparison.differences.len()
                    ),
                })
            }
        }
        other => Err(module_release_tool::ToolError::Usage(format!(
            "unknown command '{other}'; expected generate, verify or compare\n\n{}",
            usage()
        ))),
    }
}

fn usage() -> &'static str {
    "usage:\n  \
     module-release generate --module <id> --out <dir> [--repo <dir>]\n  \
     module-release verify   --artifact <dir>            [--repo <dir>]\n  \
     module-release compare  --left <dir> --right <dir>  [--repo <dir>]"
}

/// Flat `--key value` options.
struct Options {
    repo: Option<PathBuf>,
    values: std::collections::BTreeMap<String, String>,
    paths: std::collections::BTreeMap<String, PathBuf>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, module_release_tool::ToolError> {
        let mut repo = None;
        let mut values = std::collections::BTreeMap::new();
        let mut paths = std::collections::BTreeMap::new();
        let mut index = 0;
        while index < arguments.len() {
            let flag = arguments[index].as_str();
            let Some(key) = flag.strip_prefix("--") else {
                return Err(module_release_tool::ToolError::Usage(format!(
                    "unexpected argument '{flag}'\n\n{}",
                    usage()
                )));
            };
            let Some(value) = arguments.get(index + 1) else {
                return Err(module_release_tool::ToolError::Usage(format!(
                    "'--{key}' needs a value"
                )));
            };
            index += 2;
            match key {
                "repo" => repo = Some(PathBuf::from(value)),
                "out" | "artifact" | "left" | "right" => {
                    paths.insert(key.to_owned(), PathBuf::from(value));
                }
                _ => {
                    values.insert(key.to_owned(), value.clone());
                }
            }
        }
        Ok(Self {
            repo,
            values,
            paths,
        })
    }

    /// A text option value, such as `--module`.
    fn text(&self, key: &str) -> Result<&str, module_release_tool::ToolError> {
        self.values.get(key).map(String::as_str).ok_or_else(|| {
            module_release_tool::ToolError::Usage(format!("'--{key}' is required\n\n{}", usage()))
        })
    }

    /// A path option value, such as `--out` or `--artifact`.
    fn required(&self, key: &str) -> Result<&Path, module_release_tool::ToolError> {
        self.paths
            .get(key)
            .map(std::convert::AsRef::as_ref)
            .ok_or_else(|| {
                module_release_tool::ToolError::Usage(format!(
                    "'--{key}' is required\n\n{}",
                    usage()
                ))
            })
    }
}
