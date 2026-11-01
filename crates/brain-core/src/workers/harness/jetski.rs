use super::HeadlessHarness;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const HARNESS_ID_JETSKI: &str = "jetski";
const DEFAULT_JETSKI_COMMAND: &str = "jetski";
const DEFAULT_JETSKI_FLAG: &str = "-p";

#[derive(Debug, Clone)]
pub struct JetskiHeadlessHarness {
    pub command: String,
    pub args: Vec<String>,
}

impl JetskiHeadlessHarness {
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
        }
    }
}

impl Default for JetskiHeadlessHarness {
    fn default() -> Self {
        Self {
            command: DEFAULT_JETSKI_COMMAND.to_string(),
            args: vec![DEFAULT_JETSKI_FLAG.to_string()],
        }
    }
}

#[async_trait]
impl HeadlessHarness for JetskiHeadlessHarness {
    fn harness_id(&self) -> &str {
        HARNESS_ID_JETSKI
    }

    async fn run_prompt(&self, prompt: &str, max_duration: Duration) -> Result<String> {
        let trimmed_cmd = self.command.trim();
        if trimmed_cmd.is_empty() {
            bail!("headless harness command cannot be empty");
        }
        if prompt.trim().is_empty() {
            bail!("headless harness prompt cannot be empty");
        }

        let mut cmd = Command::new(trimmed_cmd);
        cmd.args(&self.args)
            .arg(prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = timeout(max_duration, cmd.output())
            .await
            .with_context(|| {
                format!(
                    "headless harness '{}' timed out after {:?}",
                    self.harness_id(),
                    max_duration
                )
            })?
            .with_context(|| {
                format!(
                    "failed to spawn headless harness command '{}'",
                    self.command
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "headless harness '{}' exited with {}: {}",
                self.harness_id(),
                output.status,
                stderr.trim()
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
