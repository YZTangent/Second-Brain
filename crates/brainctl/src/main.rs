pub mod mcp;

use anyhow::{Context, Result};
use brain_core::tx::WorktreeCoordinator;
use clap::{Parser, Subcommand};
use mcp::McpServer;
use std::path::PathBuf;

const DEFAULT_SESSION_ID: &str = "cli-session";

#[derive(Debug, Parser)]
#[command(name = "brainctl", about = "Second Brain v2 CLI and MCP stdio bridge")]
struct Cli {
    #[arg(long)]
    vault: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run restricted Agent MCP JSON-RPC 2.0 server over stdio
    McpStdio {
        #[arg(long, default_value = DEFAULT_SESSION_ID)]
        session_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let coord = WorktreeCoordinator::new(&cli.vault)
        .with_context(|| format!("failed to open vault at {}", cli.vault.display()))?;

    match cli.command {
        Commands::McpStdio { session_id } => {
            let server = McpServer::new(coord, session_id);
            server.run_stdio()?;
        }
    }
    Ok(())
}
