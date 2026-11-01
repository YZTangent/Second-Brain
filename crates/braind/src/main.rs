pub mod api;
pub mod state;

use anyhow::{Context, Result};
use brain_core::config::BrainConfig;
use brain_core::tx::WorktreeCoordinator;
use clap::Parser;
use state::AppState;
use std::path::PathBuf;

const DEFAULT_CONFIG_PATH: &str = "config.toml";

#[derive(Debug, Parser)]
#[command(name = "braind", about = "Second Brain v2 Headless Daemon")]
struct CliArgs {
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    config: PathBuf,
    #[arg(long)]
    vault: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();
    let config = BrainConfig::load(&args.config)
        .with_context(|| format!("failed to load config from {}", args.config.display()))?;
    let coord = WorktreeCoordinator::new(&args.vault).with_context(|| {
        format!(
            "failed to open or initialize vault at {}",
            args.vault.display()
        )
    })?;
    let state = AppState::new(config.clone(), coord)
        .context("failed to initialize braind application state")?;

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind TCP listener on {}", addr))?;
    axum::serve(listener, api::build_router(state))
        .await
        .context("braind server error")?;
    Ok(())
}
