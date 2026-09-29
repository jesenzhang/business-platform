//! `module-release` — local release-candidate CLI (PLAN-0014 Wave 5A).
//!
//! ```text
//! module-release generate --module <id> --out <dir>            [--repo <dir>]
//! module-release verify   --artifact <dir>                     [--repo <dir>]
//! module-release compare  --left <dir> --right <dir>           [--repo <dir>]
//! module-release guard    --metadata <file> --consumer <dir> --repo <dir>
//!                       [--artifact <dir>]… [--forbid <dir>]…
//! ```
//!
//! `guard` decides whether a consumer really resolved outside the source
//! workspace, from a `cargo metadata --format-version 1` document produced by the
//! orchestration script. Judging isolation from the resolved graph — rather than
//! by grepping a manifest for a path — is what catches a forward-slash path, a
//! relative path and a transitive path dependency.
//!
//! Any failure exits non-zero. There is no path in this binary that reports a
//! failed check as success, and no command publishes, registers or installs
//! anything.

use std::path::PathBuf;
use std::process::ExitCode;

use module_release_tool::isolation::{self, IsolationPolicy};
use module_release_tool::packaging;
use module_release_tool::repo::RepoContext;
use module_release_tool::ToolError;

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

fn run(arguments: &[String]) -> Result<(), ToolError> {
    let Some(command) = arguments.first() else {
        return Err(ToolError::Usage(usage().to_owned()));
    };
    let options = Options::parse(&arguments[1..])?;
    match command.as_str() {
        "generate" => generate(&options),
        "verify" => verify(&options),
        "compare" => compare(&options),
        "guard" => guard(&options),
        other => Err(ToolError::Usage(format!(
            "unknown command '{other}'; expected generate, verify, compare or guard\n\n{}",
            usage()
        ))),
    }
}

/// The repository the command operates on: `--repo`, or the one containing the
/// current directory.
fn context(options: &Options) -> Result<RepoContext, ToolError> {
    match &options.repo {
        Some(root) => RepoContext::with_root(root.clone()),
        None => RepoContext::discover(),
    }
}

fn generate(options: &Options) -> Result<(), ToolError> {
    let context = context(options)?;
    let module = options.text("module")?;
    let out = options.required("out")?;
    let artifact = packaging::generate(&context, module, &out)?;
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

fn verify(options: &Options) -> Result<(), ToolError> {
    let context = context(options)?;
    let artifact = options.required("artifact")?;
    let index = packaging::verify(&context, &artifact)?;
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

fn compare(options: &Options) -> Result<(), ToolError> {
    let left = options.required("left")?;
    let right = options.required("right")?;
    let comparison = packaging::compare(&left, &right)?;
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
    println!("scope={}", comparison.scope);
    for difference in &comparison.differences {
        println!("difference={difference}");
    }
    if comparison.identical {
        Ok(())
    } else {
        Err(ToolError::Verification {
            check: "repeat-build-consistency".to_owned(),
            reason: format!(
                "{} difference(s) between the two candidates",
                comparison.differences.len()
            ),
        })
    }
}

fn guard(options: &Options) -> Result<(), ToolError> {
    let metadata = options.required("metadata")?;
    let consumer = options.required("consumer")?;
    let artifact_roots = options.list("artifact");
    if artifact_roots.is_empty() {
        return Err(ToolError::Usage(format!(
            "'--artifact' is required (repeat it once per artifact directory the consumer may \
             resolve into)\n\n{}",
            usage()
        )));
    }
    // The source repository is the root that must never appear in a consumer's
    // graph, so it is a default rather than something the caller may forget.
    let forbidden = match options.list("forbid") {
        roots if roots.is_empty() => vec![context(options)?.root().to_path_buf()],
        roots => roots,
    };
    let text = std::fs::read_to_string(&metadata).map_err(|source| ToolError::Io {
        path: metadata.display().to_string(),
        source,
    })?;
    let policy = IsolationPolicy {
        consumer_root: consumer,
        allowed_roots: artifact_roots,
        forbidden_roots: forbidden,
    };
    for line in isolation::describe_policy(&policy) {
        println!("{line}");
    }
    let report = isolation::check_metadata(&text, &policy)?;
    println!("status=isolated");
    println!("check[{}] PASS", isolation::CHECK_ISOLATION);
    println!("workspace_root={}", report.workspace_root);
    println!("registry_packages={}", report.registry_packages);
    for package in &report.local_packages {
        println!(
            "local_package={} manifest={} resolved={} verdict={}",
            package.package, package.reported_path, package.resolved_path, package.verdict
        );
    }
    Ok(())
}
fn usage() -> &'static str {
    "usage:\n  \
     module-release generate --module <id> --out <dir>            [--repo <dir>]\n  \
     module-release verify   --artifact <dir>                     [--repo <dir>]\n  \
     module-release compare  --left <dir> --right <dir>           [--repo <dir>]\n  \
     module-release guard    --metadata <file> --consumer <dir> --repo <dir>\n    \
                       [--artifact <dir>]… [--forbid <dir>]…"
}

/// Flat `--key value` options; `--out`, `--artifact` and `--forbid` may repeat.
struct Options {
    repo: Option<PathBuf>,
    values: std::collections::BTreeMap<String, String>,
    paths: std::collections::BTreeMap<String, Vec<PathBuf>>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, ToolError> {
        let mut repo = None;
        let mut values = std::collections::BTreeMap::new();
        let mut paths: std::collections::BTreeMap<String, Vec<PathBuf>> =
            std::collections::BTreeMap::new();
        let mut index = 0;
        while index < arguments.len() {
            let flag = arguments[index].as_str();
            let Some(key) = flag.strip_prefix("--") else {
                return Err(ToolError::Usage(format!(
                    "unexpected argument '{flag}'\n\n{}",
                    usage()
                )));
            };
            let Some(value) = arguments.get(index + 1) else {
                return Err(ToolError::Usage(format!("'--{key}' needs a value")));
            };
            index += 2;
            if key == "repo" {
                repo = Some(PathBuf::from(value));
                continue;
            }
            paths
                .entry(key.to_owned())
                .or_default()
                .push(PathBuf::from(value));
            if key == "module" {
                values.insert(key.to_owned(), value.clone());
            }
        }
        Ok(Self {
            repo,
            values,
            paths,
        })
    }

    /// A text option value, such as `--module`.
    fn text(&self, key: &str) -> Result<&str, ToolError> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| ToolError::Usage(format!("'--{key}' is required\n\n{}", usage())))
    }

    /// The first value of a path option, such as `--out` or `--artifact`.
    fn required(&self, key: &str) -> Result<PathBuf, ToolError> {
        self.list(key)
            .into_iter()
            .next()
            .ok_or_else(|| ToolError::Usage(format!("'--{key}' is required\n\n{}", usage())))
    }

    /// Every value of a repeatable path option.
    fn list(&self, key: &str) -> Vec<PathBuf> {
        self.paths.get(key).cloned().unwrap_or_default()
    }
}
