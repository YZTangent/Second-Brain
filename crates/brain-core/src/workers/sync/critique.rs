use super::{ExternalItem, ExternalSyncSource};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const SOURCE_ID_CRITIQUE: &str = "critique";
const DEFAULT_CRITIQUE_COMMAND: &str = "critique";
const CL_PREFIX: &str = "cl/";
const MIN_CL_FIELDS: usize = 3;
const DEFAULT_SYNC_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct CritiqueSyncSource {
    pub command: String,
    pub args: Vec<String>,
}

impl CritiqueSyncSource {
    pub fn new() -> Self {
        Self {
            command: DEFAULT_CRITIQUE_COMMAND.to_string(),
            args: vec!["list".to_string(), "--mine".to_string()],
        }
    }

    pub fn parse_output(stdout: &str) -> Vec<ExternalItem> {
        let mut items = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<ExternalItem>(trimmed) {
                items.push(parsed);
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < MIN_CL_FIELDS {
                continue;
            }
            let raw_id = parts[0];
            let id = if raw_id.starts_with(CL_PREFIX) {
                raw_id.to_string()
            } else {
                format!("{}{}", CL_PREFIX, raw_id)
            };
            let status = parts[1].to_string();
            let title = parts[2..].join(" ");
            items.push(ExternalItem {
                id,
                title,
                status,
                priority: None,
            });
        }
        items
    }
}

impl Default for CritiqueSyncSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExternalSyncSource for CritiqueSyncSource {
    fn source_id(&self) -> &str {
        SOURCE_ID_CRITIQUE
    }

    async fn poll(&self) -> Result<Vec<ExternalItem>> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = timeout(DEFAULT_SYNC_TIMEOUT, cmd.output())
            .await
            .context("critique sync poll timed out")?
            .with_context(|| format!("failed to execute '{}'", self.command))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "critique sync exited with {}: {}",
                output.status,
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_output(&stdout))
    }
}
