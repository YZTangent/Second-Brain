use super::git::{branch_of, run_git, tx_worktrees_dir};
use super::session::{
    TxError, BRANCH_MAIN, BRANCH_MANUAL, REF_MAIN, REF_MANUAL, VAULT_NODE_FOLDERS,
};
use std::path::Path;

pub const VAULT_MARKER_FILE: &str = ".second-brain.json";
const VAULT_GITIGNORE_CONTENT: &str = "/.brain/\n";
const VAULT_MARKER_CONTENT: &str =
    "{\n  \"version\": 2,\n  \"layout\": [\"knowledge\", \"goals\", \"tasks\", \"journal\", \"relationships\"]\n}\n";

pub fn ensure_or_init_vault(vault_root: &Path) -> Result<(), TxError> {
    if vault_root.exists() {
        if !vault_root.is_dir() {
            return Err(TxError::InvalidVaultDirectory(
                vault_root.display().to_string(),
                "path exists and is not a directory".to_string(),
            ));
        }

        let mut entries = std::fs::read_dir(vault_root)?;
        if entries.next().is_some() {
            return open_existing_vault(vault_root);
        }
    }

    init_empty_vault(vault_root)
}

fn open_existing_vault(vault_root: &Path) -> Result<(), TxError> {
    let marker_path = vault_root.join(VAULT_MARKER_FILE);
    let git_dir = vault_root.join(".git");

    if !marker_path.is_file() || !git_dir.exists() {
        return Err(TxError::InvalidVaultDirectory(
            vault_root.display().to_string(),
            format!(
                "non-empty directory missing '{}' or '.git'",
                VAULT_MARKER_FILE
            ),
        ));
    }

    let marker_raw = std::fs::read_to_string(&marker_path)?;
    if serde_json::from_str::<serde_json::Value>(&marker_raw).is_err() {
        return Err(TxError::InvalidVaultDirectory(
            vault_root.display().to_string(),
            format!("'{}' is not valid JSON", VAULT_MARKER_FILE),
        ));
    }

    for (branch, ref_name) in [(BRANCH_MAIN, REF_MAIN), (BRANCH_MANUAL, REF_MANUAL)] {
        if run_git(vault_root, &["rev-parse", "--verify", ref_name]).is_err() {
            return Err(TxError::InvalidVaultDirectory(
                vault_root.display().to_string(),
                format!("missing required git branch '{}'", branch),
            ));
        }
    }

    for folder in VAULT_NODE_FOLDERS {
        std::fs::create_dir_all(vault_root.join(folder))?;
    }
    std::fs::create_dir_all(tx_worktrees_dir(vault_root))?;

    if branch_of(vault_root)? != BRANCH_MANUAL {
        run_git(vault_root, &["checkout", BRANCH_MANUAL])?;
    }

    Ok(())
}

fn init_empty_vault(vault_root: &Path) -> Result<(), TxError> {
    std::fs::create_dir_all(vault_root)?;
    run_git(vault_root, &["init"])?;
    run_git(vault_root, &["checkout", "-B", BRANCH_MAIN])?;

    std::fs::write(vault_root.join(VAULT_MARKER_FILE), VAULT_MARKER_CONTENT)?;
    std::fs::write(vault_root.join(".gitignore"), VAULT_GITIGNORE_CONTENT)?;

    for folder in VAULT_NODE_FOLDERS {
        let dir = vault_root.join(folder);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(".gitkeep"), "")?;
    }
    std::fs::create_dir_all(tx_worktrees_dir(vault_root))?;

    run_git(vault_root, &["add", "."])?;
    run_git(
        vault_root,
        &[
            "commit",
            "-m",
            "chore(vault): initialize second brain v2 vault",
        ],
    )?;
    run_git(vault_root, &["checkout", "-b", BRANCH_MANUAL])?;

    Ok(())
}
