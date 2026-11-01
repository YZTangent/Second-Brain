use super::{ExternalItem, ExternalSyncSource};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const SOURCE_ID_BUGANIZER: &str = "buganizer";
const DEFAULT_ISSUE_COMMAND: &str = "issue";
const DEFAULT_BUGANIZER_QUERY: &str = "assignee:me status:open";
const BUG_PREFIX: &str = "b/";
const MIN_BUG_FIELDS: usize = 4;
const DEFAULT_SYNC_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct BuganizerSyncSource {
    pub command: String,
    pub query: String,
}

impl BuganizerSyncSource {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            command: DEFAULT_ISSUE_COMMAND.to_string(),
            query: query.into(),
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
            if parts.len() < MIN_BUG_FIELDS {
                continue;
            }
            let raw_id = parts[0];
            let id = if raw_id.starts_with(BUG_PREFIX) {
                raw_id.to_string()
            } else {
                format!("{}{}", BUG_PREFIX, raw_id)
            };
            let priority = parts[1].to_string();
            let status = parts[2].to_string();
            let title = parts[3..].join(" ");
            items.push(ExternalItem {
                id,
                title,
                status,
                priority: Some(priority),
            });
        }
        items
    }
}

impl Default for BuganizerSyncSource {
    fn default() -> Self {
        Self::new(DEFAULT_BUGANIZER_QUERY)
    }
}

#[async_trait]
impl ExternalSyncSource for BuganizerSyncSource {
    fn source_id(&self) -> &str {
        SOURCE_ID_BUGANIZER
    }

    async fn poll(&self) -> Result<Vec<ExternalItem>> {
        let mut cmd = Command::new(&self.command);
        cmd.args(["list", "--query", &self.query])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = timeout(DEFAULT_SYNC_TIMEOUT, cmd.output())
            .await
            .context("buganizer sync poll timed out")?
            .with_context(|| format!("failed to execute '{}'", self.command))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "buganizer sync exited with {}: {}",
                output.status,
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_output(&stdout))
    }
}
