use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub const VAULT_NODE_FOLDERS: [&str; 5] =
    ["knowledge", "goals", "tasks", "journal", "relationships"];
pub const BRANCH_MAIN: &str = "main";
pub const BRANCH_MANUAL: &str = "manual";
pub const REF_MAIN: &str = "refs/heads/main";
pub const REF_MANUAL: &str = "refs/heads/manual";
pub const TRAILER_ACTOR: &str = "Brain-Actor:";
pub const TRAILER_TX: &str = "Brain-Tx:";
pub const TRAILER_SESSION: &str = "Brain-Session:";
const MIN_SHA_LEN: usize = 4;
const MAX_SHA_LEN: usize = 64;
const MARKDOWN_EXT: &str = ".md";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxHandle {
    pub tx_id: String,
    pub owner_session_id: String,
    pub worktree_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum TxError {
    #[error("invalid vault directory '{0}': {1}")]
    InvalidVaultDirectory(String, String),
    #[error("session mismatch for tx {tx_id}: owner={owner}, caller={caller}")]
    SessionMismatch {
        tx_id: String,
        owner: String,
        caller: String,
    },
    #[error("transaction not found: {0}")]
    NotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("git error: {0}")]
    Git(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Default)]
pub struct SessionRegistry {
    active: Arc<Mutex<HashMap<String, TxHandle>>>,
}

impl SessionRegistry {
    pub fn insert(&self, handle: TxHandle) {
        let mut guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(handle.tx_id.clone(), handle);
    }

    pub fn verify_owner(&self, tx_id: &str, caller_session_id: &str) -> Result<TxHandle, TxError> {
        let guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
        let handle = guard
            .get(tx_id)
            .ok_or_else(|| TxError::NotFound(tx_id.to_string()))?;
        if handle.owner_session_id != caller_session_id {
            return Err(TxError::SessionMismatch {
                tx_id: tx_id.to_string(),
                owner: handle.owner_session_id.clone(),
                caller: caller_session_id.to_string(),
            });
        }
        Ok(handle.clone())
    }

    pub fn remove(&self, tx_id: &str) -> Option<TxHandle> {
        let mut guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(tx_id)
    }
}

pub fn validate_rel_path(rel_path: &str) -> Result<(), TxError> {
    if rel_path.is_empty()
        || rel_path.starts_with('/')
        || rel_path.starts_with('\\')
        || rel_path.starts_with('-')
        || rel_path.contains('\\')
        || rel_path.contains('\0')
        || !rel_path.ends_with(MARKDOWN_EXT)
    {
        return Err(TxError::Validation(format!(
            "invalid relative node path: {}",
            rel_path
        )));
    }

    let mut components = Vec::new();
    for comp in Path::new(rel_path).components() {
        match comp {
            Component::Normal(os_str) => {
                let Some(s) = os_str.to_str() else {
                    return Err(TxError::Validation(format!(
                        "non-utf8 path component in: {}",
                        rel_path
                    )));
                };
                if s.is_empty() || s.starts_with('.') || s.starts_with('-') {
                    return Err(TxError::Validation(format!(
                        "disallowed path segment '{}' in: {}",
                        s, rel_path
                    )));
                }
                components.push(s);
            }
            _ => {
                return Err(TxError::Validation(format!(
                    "path traversal or non-normal component in: {}",
                    rel_path
                )));
            }
        }
    }

    if components.len() < 2 || !VAULT_NODE_FOLDERS.contains(&components[0]) {
        return Err(TxError::Validation(format!(
            "path must reside inside a valid vault node folder: {}",
            rel_path
        )));
    }

    Ok(())
}

pub fn ensure_no_symlink_components(base_dir: &Path, rel_path: &str) -> Result<PathBuf, TxError> {
    validate_rel_path(rel_path)?;
    let mut cur = base_dir.to_path_buf();
    for comp in Path::new(rel_path).components() {
        if let Component::Normal(part) = comp {
            cur.push(part);
            if let Ok(meta) = std::fs::symlink_metadata(&cur) {
                if meta.file_type().is_symlink() {
                    return Err(TxError::Validation(format!(
                        "symlink path component rejected: {}",
                        cur.display()
                    )));
                }
            }
        }
    }
    Ok(cur)
}

pub fn validate_branch(branch: &str) -> Result<(), TxError> {
    if branch == BRANCH_MAIN || branch == BRANCH_MANUAL {
        Ok(())
    } else {
        Err(TxError::Validation(format!(
            "unsupported branch '{}': expected '{}' or '{}'",
            branch, BRANCH_MAIN, BRANCH_MANUAL
        )))
    }
}

pub fn validate_sha(sha: &str) -> Result<(), TxError> {
    if (MIN_SHA_LEN..=MAX_SHA_LEN).contains(&sha.len())
        && sha.chars().all(|c| c.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(TxError::Validation(format!(
            "invalid git commit SHA: {}",
            sha
        )))
    }
}
