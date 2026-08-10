//! PLAN-0009 rehearsal CLI.
//!
//! Stage 1 intentionally exposes only the read-only inventory command. There
//! is no production mode, import command, upload command, or source write path.

use std::path::PathBuf;
use std::process::ExitCode;

use plan_0009_rehearsal::{run_inventory, InventoryConfig};

const LEGACY_ROOT: &str = r"F:\Workspace\git_repo\contract_management";
const ENV_FILE: &str = r"F:\Workspace\git_repo\contract_management\backend\.env.local-test";
const ISOLATION_ROOT: &str = r"F:\Workspace\plan-0009-c-project-migration-rehearsal-20260810";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => {
            eprintln!("PLAN-0009 rehearsal failed: {code}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), &'static str> {
    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() != Some("inventory") {
        return Err("usage: plan-0009-rehearsal inventory");
    }
    if arguments.any(|argument| argument.eq_ignore_ascii_case("production")) {
        return Err("production_mode_rejected");
    }

    let isolation_root = PathBuf::from(ISOLATION_ROOT);
    let target_root = isolation_root.join("stage-1-inventory-v7");
    std::fs::create_dir_all(&target_root).map_err(|_| "target_write_failed")?;
    let config =
        InventoryConfig::from_env_file(LEGACY_ROOT, ENV_FILE, &isolation_root, &target_root)
            .map_err(|error| error.code())?;
    let summary = run_inventory(&config).await.map_err(|error| error.code())?;
    let counts = summary
        .classification_counts
        .iter()
        .map(|count| format!("{}={}", count.classification, count.count))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "stage=1 status={} selected={} replayed={} canonical_manifest_sha256={} file_bytes_sha256={} classifications={counts}",
        if summary.replayed {
            "replayed"
        } else {
            "frozen"
        },
        summary.selected_contracts,
        summary.replayed,
        summary.canonical_manifest_sha256,
        summary.file_bytes_sha256,
    );
    Ok(())
}
