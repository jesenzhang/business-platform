//! PLAN-0009 rehearsal CLI.
//!
//! PLAN-0009 exposes only bounded rehearsal commands. There is no production
//! mode, import command, upload command, or source write path.

use std::path::PathBuf;
use std::process::ExitCode;

use plan_0009_rehearsal::{run_inventory, run_stage2, run_stage3, InventoryConfig};

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
    let command = arguments
        .next()
        .ok_or("usage: plan-0009-rehearsal inventory|stage2|stage3")?;
    if arguments.any(|argument| argument.eq_ignore_ascii_case("production")) {
        return Err("production_mode_rejected");
    }

    let isolation_root = PathBuf::from(ISOLATION_ROOT);
    let target_root = match command.as_str() {
        "inventory" => isolation_root.join("stage-1-inventory-v9"),
        "stage2" => isolation_root.join("stage-2-rehearsal-v2"),
        "stage3" => isolation_root.join("stage-3-rehearsal-v1"),
        _ => return Err("usage: plan-0009-rehearsal inventory|stage2|stage3"),
    };
    std::fs::create_dir_all(&target_root).map_err(|_| "target_write_failed")?;
    let config =
        InventoryConfig::from_env_file(LEGACY_ROOT, ENV_FILE, &isolation_root, &target_root)
            .map_err(|error| error.code())?;
    if command == "inventory" {
        let summary = run_inventory(&config).await.map_err(|error| error.code())?;
        let counts = summary
            .classification_counts
            .iter()
            .map(|count| format!("{}={}", count.classification, count.count))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "stage=1 status={} selected={} replayed={} canonical_manifest_sha256={} file_bytes_sha256={} classifications={counts}",
            if summary.replayed { "replayed" } else { "frozen" },
            summary.selected_contracts,
            summary.replayed,
            summary.canonical_manifest_sha256,
            summary.file_bytes_sha256,
        );
    } else if command == "stage2" {
        let summary = run_stage2(&config).await.map_err(|error| error.code())?;
        println!(
            "stage=2 status={} selected={} exact_eligible={} exact_materialized={} review={} quarantine={} replayed={} mapping_plan_sha256={}",
            if summary.replayed { "replayed" } else { "frozen" },
            summary.selected_contracts,
            summary.exact_eligible,
            summary.exact_materialized,
            summary.review_count,
            summary.quarantine_count,
            summary.replayed,
            summary.mapping_plan_sha256,
        );
    } else {
        let summary = run_stage3(&config).await.map_err(|error| {
            eprintln!("stage3_error={error}");
            error.code()
        })?;
        println!(
            "stage=3 status=replayed selected={} replay_equal={} quarantine={} object_files={} object_bytes={} input_manifest_sha256={}",
            summary.selected_contracts,
            summary.replay_equal,
            summary.quarantine_count,
            summary.object_files,
            summary.object_bytes,
            summary.input_manifest_sha256,
        );
    }
    Ok(())
}
