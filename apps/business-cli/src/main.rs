//! Remote Business Platform CLI.
//!
//! Every command uses `business-api-client`; this binary has no database,
//! object-storage, or repository dependency.

use std::path::PathBuf;

use business_api_client::{BusinessApiClient, ClientConfig, UploadRequest};
use clap::{Parser, Subcommand};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "business-cli",
    version,
    about = "Business Platform remote client"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        env = "BUSINESS_API_URL",
        default_value = "http://localhost:3000"
    )]
    api_url: String,
    #[arg(
        long,
        global = true,
        env = "BUSINESS_API_TOKEN",
        default_value = "dev-only-secret",
        hide_env_values = true
    )]
    token: String,
    #[arg(long, global = true, conflicts_with = "table")]
    json: bool,
    #[arg(long, global = true, conflicts_with = "json")]
    table: bool,
    #[arg(long, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Status,
    Documents {
        #[command(subcommand)]
        command: DocumentsCommand,
    },
    Processing {
        #[command(subcommand)]
        command: ProcessingCommand,
    },
    Candidate {
        #[command(subcommand)]
        command: GetCommand,
    },
    Audit {
        #[command(subcommand)]
        command: ListCommand,
    },
    Findings {
        #[command(subcommand)]
        command: ListCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DocumentsCommand {
    List {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    Get {
        id: Uuid,
    },
    Upload {
        file: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ProcessingCommand {
    List {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    Start {
        document_id: Uuid,
    },
    Get {
        id: Uuid,
    },
}

#[derive(Debug, Subcommand)]
enum ListCommand {
    List {
        #[arg(long, default_value_t = 50)]
        limit: u16,
    },
}

#[derive(Debug, Subcommand)]
enum GetCommand {
    Get { id: Uuid },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli).await {
        eprintln!("business-cli: {error}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let client = BusinessApiClient::new(ClientConfig::new(cli.api_url, cli.token)?)?;
    let value = match cli.command {
        Command::Status => client.status().await?,
        Command::Documents { command } => match command {
            DocumentsCommand::List { limit } => {
                serde_json::to_value(client.documents_list(None, limit).await?)?
            }
            DocumentsCommand::Get { id } => serde_json::to_value(client.document_get(id).await?)?,
            DocumentsCommand::Upload { file } => {
                let body = tokio::fs::read(&file).await?;
                let file_name = file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("upload.bin")
                    .to_string();
                let content_type = content_type_for(&file_name);
                serde_json::to_value(
                    client
                        .upload(UploadRequest {
                            file_name,
                            content_type,
                            body: body.into(),
                            idempotency_key: Uuid::now_v7().to_string(),
                        })
                        .await?,
                )?
            }
        },
        Command::Processing { command } => match command {
            ProcessingCommand::List { limit } => {
                serde_json::to_value(client.processing_list(None, limit).await?)?
            }
            ProcessingCommand::Start { document_id } => {
                let document = client.document_get(document_id).await?;
                serde_json::to_value(
                    client
                        .processing_start(
                            document_id,
                            document.content_revision,
                            &Uuid::now_v7().to_string(),
                        )
                        .await?,
                )?
            }
            ProcessingCommand::Get { id } => {
                serde_json::to_value(client.processing_get(id).await?)?
            }
        },
        Command::Candidate {
            command: GetCommand::Get { id },
        } => serde_json::to_value(client.candidate_get(id).await?)?,
        Command::Audit {
            command: ListCommand::List { limit },
        } => serde_json::to_value(client.audit_list(None, limit).await?)?,
        Command::Findings {
            command: ListCommand::List { limit },
        } => serde_json::to_value(client.findings_list(limit).await?)?,
    };
    if cli.quiet {
        return Ok(());
    }
    if cli.table {
        print_table(&value);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn content_type_for(file_name: &str) -> String {
    match file_name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn print_table(value: &serde_json::Value) {
    match value.get("items").and_then(serde_json::Value::as_array) {
        Some(items) => {
            println!("ID\tNAME/STATUS");
            for item in items {
                let id = item
                    .get("id")
                    .or_else(|| item.get("job_id"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("-");
                let label = item
                    .get("original_filename")
                    .or_else(|| item.get("status"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("-");
                println!("{id}\t{label}");
            }
        }
        None => println!(
            "{}",
            serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_commands_parse_with_stable_remote_shape() {
        assert!(Cli::try_parse_from(["business-cli", "status"]).is_ok());
        assert!(Cli::try_parse_from(["business-cli", "documents", "list"]).is_ok());
        assert!(Cli::try_parse_from([
            "business-cli",
            "documents",
            "get",
            "00000000-0000-0000-0000-000000000001"
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "business-cli",
            "processing",
            "start",
            "00000000-0000-0000-0000-000000000001"
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "business-cli",
            "candidate",
            "get",
            "00000000-0000-0000-0000-000000000001"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["business-cli", "audit", "list", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["business-cli", "findings", "list", "--table"]).is_ok());
    }

    #[test]
    fn invalid_command_and_content_type_fail_closed() {
        assert!(Cli::try_parse_from(["business-cli", "documents", "delete", "x"]).is_err());
        assert_eq!(content_type_for("report.txt"), "text/plain");
        assert_eq!(content_type_for("report.pdf"), "application/pdf");
        assert_eq!(content_type_for("report.exe"), "application/octet-stream");
    }
}
