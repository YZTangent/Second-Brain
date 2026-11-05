use super::{ObservedTurn, ObserverHarnessAdapter};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const HARNESS_ID_JETSKI: &str = "jetski";
const DEFAULT_AGENTAPI_BIN: &str = "agentapi";
const JETSKI_HOME_AGENTAPI_REL: &str = ".gemini/jetski/bin/agentapi";
const ANTIGRAVITY_SOURCE_METADATA_ENV: &str = "ANTIGRAVITY_SOURCE_METADATA";
const DEFAULT_SESSION_ID: &str = "default";
const MAX_SESSION_ID_LEN: usize = 128;
const DEFAULT_PUSH_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct JetskiObserverAdapter {
    pub watch_paths: Vec<PathBuf>,
    pub agentapi_bin: String,
}

impl JetskiObserverAdapter {
    pub fn new(watch_paths: Vec<PathBuf>) -> Self {
        Self {
            watch_paths,
            agentapi_bin: resolve_default_agentapi_bin(),
        }
    }

    pub fn with_agentapi_bin(watch_paths: Vec<PathBuf>, agentapi_bin: impl Into<String>) -> Self {
        Self {
            watch_paths,
            agentapi_bin: agentapi_bin.into(),
        }
    }
}

fn resolve_default_agentapi_bin() -> String {
    if let Ok(home) = std::env::var("HOME") {
        let candidate = Path::new(&home).join(JETSKI_HOME_AGENTAPI_REL);
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    DEFAULT_AGENTAPI_BIN.to_string()
}

pub fn extract_session_id_from_path(path: &Path) -> Option<String> {
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Canonical Jetski layout: .../brain/<conversation-id>/.system_generated/logs/transcript.jsonl
    for (idx, comp) in components.iter().enumerate() {
        if *comp == "brain" {
            if let Some(candidate) = components.get(idx + 1) {
                if validate_session_id(candidate).is_ok() {
                    return Some((*candidate).to_string());
                }
            }
        }
    }

    // Fallback for .../<conversation-id>/.system_generated/logs/transcript.jsonl
    if components.len() >= 4
        && components[components.len() - 3] == ".system_generated"
        && components[components.len() - 2] == "logs"
    {
        let candidate = components[components.len() - 4];
        if validate_session_id(candidate).is_ok() {
            return Some(candidate.to_string());
        }
    }

    // Fallback for .../<conversation-id>/transcript.jsonl
    if components.len() >= 2 {
        let candidate = components[components.len() - 2];
        if validate_session_id(candidate).is_ok() {
            return Some(candidate.to_string());
        }
    }

    None
}

pub fn validate_session_id(session_id: &str) -> Result<()> {
    if session_id.is_empty()
        || session_id.len() > MAX_SESSION_ID_LEN
        || session_id.starts_with('-')
        || !session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid session_id '{}'", session_id);
    }
    Ok(())
}

fn is_user_turn(val: &Value) -> bool {
    let role_or_type = val
        .get("role")
        .and_then(Value::as_str)
        .or_else(|| val.get("type").and_then(Value::as_str));

    match role_or_type {
        Some(kind) => matches!(
            kind.to_ascii_lowercase().as_str(),
            "user" | "user_input" | "human"
        ),
        None => val.get("user_text").and_then(Value::as_str).is_some(),
    }
}

fn unwrap_user_request_tag(raw: &str) -> &str {
    const OPEN: &str = "<USER_REQUEST>";
    const CLOSE: &str = "</USER_REQUEST>";
    if let Some(start) = raw.find(OPEN) {
        let after_open = &raw[start + OPEN.len()..];
        if let Some(end) = after_open.find(CLOSE) {
            let inner = after_open[..end].trim();
            if !inner.is_empty() {
                return inner;
            }
        }
    }
    raw
}

fn extract_turn_text(val: &Value) -> Option<String> {
    for key in ["user_text", "text", "content", "message"] {
        if let Some(s) = val.get(key).and_then(Value::as_str) {
            let trimmed = unwrap_user_request_tag(s.trim()).trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[async_trait]
impl ObserverHarnessAdapter for JetskiObserverAdapter {
    fn harness_id(&self) -> &str {
        HARNESS_ID_JETSKI
    }

    fn watch_paths(&self) -> &[PathBuf] {
        &self.watch_paths
    }

    fn parse_new_turns(&self, raw_bytes: &[u8]) -> Result<Vec<ObservedTurn>> {
        self.parse_new_turns_for_path(None, raw_bytes)
    }

    fn parse_new_turns_for_path(
        &self,
        source_path: Option<&Path>,
        raw_bytes: &[u8],
    ) -> Result<Vec<ObservedTurn>> {
        let fallback_session = source_path
            .and_then(extract_session_id_from_path)
            .unwrap_or_else(|| DEFAULT_SESSION_ID.to_string());
        let content = String::from_utf8_lossy(raw_bytes);
        let mut turns = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if !is_user_turn(&val) {
                continue;
            }
            let Some(user_text) = extract_turn_text(&val) else {
                continue;
            };
            let session_id = val
                .get("session_id")
                .and_then(Value::as_str)
                .or_else(|| val.get("conversation_id").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| fallback_session.clone());

            turns.push(ObservedTurn {
                session_id,
                user_text,
            });
        }

        Ok(turns)
    }

    async fn push_memory(&self, session_id: &str, message: &str) -> Result<()> {
        validate_session_id(session_id)?;
        if message.trim().is_empty() {
            bail!("observer push message cannot be empty");
        }

        let mut cmd = Command::new(&self.agentapi_bin);
        cmd.args(["send-message", session_id, message])
            .env_remove(ANTIGRAVITY_SOURCE_METADATA_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = timeout(DEFAULT_PUSH_TIMEOUT, cmd.output())
            .await
            .context("agentapi send-message timed out")?
            .with_context(|| format!("failed to execute '{}'", self.agentapi_bin))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "agentapi send-message exited with {}: {}",
                output.status,
                stderr.trim()
            );
        }

        Ok(())
    }
}
