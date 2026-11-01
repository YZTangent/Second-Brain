use super::session::{TxError, BRANCH_MAIN, BRANCH_MANUAL, REF_MAIN, REF_MANUAL};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const GIT_USER_NAME_CFG: &str = "user.name=braind";
const GIT_USER_EMAIL_CFG: &str = "user.email=braind@second-brain.local";
const GIT_GPGSIGN_CFG: &str = "commit.gpgsign=false";

pub fn run_git(cwd: &Path, args: &[&str]) -> Result<String, TxError> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args([
            "-c",
            GIT_USER_NAME_CFG,
            "-c",
            GIT_USER_EMAIL_CFG,
            "-c",
            GIT_GPGSIGN_CFG,
        ])
        .args(args)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(TxError::Git(format!(
            "git {:?} failed (status {}): {}{}",
            args,
            output.status,
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!(" | {}", stdout.trim())
            }
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn branch_of(dir: &Path) -> Result<String, TxError> {
    let out = run_git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(out.trim().to_string())
}

pub fn rev_parse(dir: &Path, rev: &str) -> Result<String, TxError> {
    let out = run_git(dir, &["rev-parse", rev])?;
    Ok(out.trim().to_string())
}

pub fn tx_worktrees_dir(vault_root: &Path) -> PathBuf {
    vault_root.join(".brain").join("tx")
}

pub fn create_detached_worktree(vault_root: &Path, worktree_id: &str) -> Result<PathBuf, TxError> {
    let tx_dir = tx_worktrees_dir(vault_root);
    std::fs::create_dir_all(&tx_dir)?;
    let worktree_path = tx_dir.join(worktree_id);
    let wt_str = worktree_path
        .to_str()
        .ok_or_else(|| TxError::Validation("worktree path is not valid UTF-8".to_string()))?;
    run_git(
        vault_root,
        &["worktree", "add", "--detach", wt_str, BRANCH_MAIN],
    )?;
    Ok(worktree_path)
}

pub fn dirty_paths(vault_root: &Path) -> Result<BTreeSet<String>, TxError> {
    let mut paths = BTreeSet::new();
    for args in [
        &["diff", "--name-only", "HEAD", "--"][..],
        &["diff", "--name-only", "--cached", "--"][..],
        &["ls-files", "--others", "--exclude-standard", "--"][..],
    ] {
        let out = run_git(vault_root, args)?;
        for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
            paths.insert(line.to_string());
        }
    }
    Ok(paths)
}

pub fn sync_main_into_manual(vault_root: &Path) -> Result<(), TxError> {
    if branch_of(vault_root)? != BRANCH_MANUAL {
        run_git(vault_root, &["checkout", BRANCH_MANUAL])?;
    }

    let diff_out = run_git(
        vault_root,
        &["diff", "--name-only", BRANCH_MANUAL, BRANCH_MAIN, "--"],
    )?;
    let changed_in_main: BTreeSet<String> = diff_out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if changed_in_main.is_empty()
        && rev_parse(vault_root, REF_MAIN)? == rev_parse(vault_root, REF_MANUAL)?
    {
        return Ok(());
    }

    let dirty_on_manual = dirty_paths(vault_root)?;
    let overlapping: Vec<String> = dirty_on_manual
        .intersection(&changed_in_main)
        .cloned()
        .collect();

    if !overlapping.is_empty() {
        let mut add_args = vec!["add", "--"];
        for p in &overlapping {
            add_args.push(p.as_str());
        }
        run_git(vault_root, &add_args)?;

        let mut commit_args = vec![
            "commit",
            "-m",
            "chore(vault): auto-commit overlapping manual edits before sync",
            "--",
        ];
        for p in &overlapping {
            commit_args.push(p.as_str());
        }
        run_git(vault_root, &commit_args)?;
    }

    run_git(
        vault_root,
        &["merge", "--no-edit", "-X", "theirs", BRANCH_MAIN],
    )?;
    Ok(())
}

pub fn remove_worktree(vault_root: &Path, worktree_path: &Path) -> Result<(), TxError> {
    if let Some(wt_str) = worktree_path.to_str() {
        let _ = run_git(vault_root, &["worktree", "remove", "--force", wt_str]);
    }
    if worktree_path.exists() {
        std::fs::remove_dir_all(worktree_path)?;
    }
    let _ = run_git(vault_root, &["worktree", "prune"]);
    Ok(())
}
