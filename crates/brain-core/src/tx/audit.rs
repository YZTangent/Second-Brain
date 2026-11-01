use super::git::{
    branch_of, create_detached_worktree, remove_worktree, rev_parse, run_git, sync_main_into_manual,
};
use super::session::{
    validate_branch, validate_rel_path, validate_sha, TxError, BRANCH_MANUAL, REF_MAIN, REF_MANUAL,
    TRAILER_ACTOR, TRAILER_SESSION, TRAILER_TX, VAULT_NODE_FOLDERS,
};
use crate::model::NodeDocument;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditCommit {
    pub sha: String,
    pub summary: String,
    pub author: String,
    pub timestamp: String,
    pub actor: Option<String>,
    pub tx_id: Option<String>,
    pub session_id: Option<String>,
}

pub fn daily_stray_checkpoint(vault_root: &Path) -> Result<Option<String>, TxError> {
    if branch_of(vault_root)? != BRANCH_MANUAL {
        run_git(vault_root, &["checkout", BRANCH_MANUAL])?;
    }

    let mut add_args = vec!["add", "-A", "--"];
    add_args.extend_from_slice(&VAULT_NODE_FOLDERS);
    run_git(vault_root, &add_args)?;

    let staged = run_git(vault_root, &["diff", "--cached", "--name-only", "--"])?;
    let had_staged_strays = !staged.trim().is_empty();
    if had_staged_strays {
        let msg = format!(
            "chore(vault): daily stray checkpoint\n\n{} system:checkpoint",
            TRAILER_ACTOR
        );
        run_git(vault_root, &["commit", "-m", &msg])?;
    }

    sync_main_into_manual(vault_root)?;

    let manual_sha = rev_parse(vault_root, REF_MANUAL)?;
    let main_sha = rev_parse(vault_root, REF_MAIN)?;

    if manual_sha != main_sha {
        run_git(vault_root, &["update-ref", REF_MAIN, &manual_sha])?;
        return Ok(Some(manual_sha));
    }

    if had_staged_strays {
        Ok(Some(manual_sha))
    } else {
        Ok(None)
    }
}

pub fn list_audit_commits(
    vault_root: &Path,
    branch: &str,
    limit: usize,
) -> Result<Vec<AuditCommit>, TxError> {
    validate_branch(branch)?;
    if limit == 0 {
        return Ok(Vec::new());
    }

    let limit_arg = format!("-n{}", limit);
    let ref_arg = if branch == BRANCH_MANUAL {
        REF_MANUAL
    } else {
        REF_MAIN
    };
    let out = run_git(
        vault_root,
        &[
            "log",
            &limit_arg,
            "--format=%H%x1f%s%x1f%an%x1f%aI%x1f%B%x1e",
            ref_arg,
            "--",
        ],
    )?;

    let mut commits = Vec::new();
    for record in out.split('\x1e') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let parts: Vec<&str> = record.splitn(5, '\x1f').collect();
        if parts.len() < 5 {
            continue;
        }
        let body = parts[4];
        let mut actor = None;
        let mut tx_id = None;
        let mut session_id = None;
        for line in body.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix(TRAILER_ACTOR) {
                actor = Some(val.trim().to_string());
            } else if let Some(val) = trimmed.strip_prefix(TRAILER_TX) {
                tx_id = Some(val.trim().to_string());
            } else if let Some(val) = trimmed.strip_prefix(TRAILER_SESSION) {
                session_id = Some(val.trim().to_string());
            }
        }
        commits.push(AuditCommit {
            sha: parts[0].trim().to_string(),
            summary: parts[1].trim().to_string(),
            author: parts[2].trim().to_string(),
            timestamp: parts[3].trim().to_string(),
            actor,
            tx_id,
            session_id,
        });
    }

    Ok(commits)
}

pub fn revert_commit(vault_root: &Path, sha: &str) -> Result<String, TxError> {
    validate_sha(sha)?;

    let revert_id = format!("revert-{}", Uuid::new_v4().simple());
    let worktree_path = create_detached_worktree(vault_root, &revert_id)?;

    let revert_result = (|| -> Result<String, TxError> {
        run_git(&worktree_path, &["revert", "--no-edit", sha])?;
        let revert_sha = rev_parse(&worktree_path, "HEAD")?;
        run_git(vault_root, &["update-ref", REF_MAIN, &revert_sha])?;
        Ok(revert_sha)
    })();

    let cleanup_res = remove_worktree(vault_root, &worktree_path);
    let revert_sha = revert_result?;
    cleanup_res?;
    sync_main_into_manual(vault_root)?;
    Ok(revert_sha)
}

pub fn load_branch_documents(
    vault_root: &Path,
    branch: &str,
) -> Result<Vec<NodeDocument>, TxError> {
    validate_branch(branch)?;
    if branch == BRANCH_MANUAL {
        load_manual_documents_from_disk(vault_root)
    } else {
        load_main_documents_from_git(vault_root)
    }
}

fn load_manual_documents_from_disk(vault_root: &Path) -> Result<Vec<NodeDocument>, TxError> {
    let mut docs = Vec::new();
    for folder in VAULT_NODE_FOLDERS {
        let dir = vault_root.join(folder);
        if !dir.is_dir() {
            continue;
        }
        collect_markdown_files_in_dir(vault_root, &dir, &mut docs)?;
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

fn collect_markdown_files_in_dir(
    vault_root: &Path,
    dir: &Path,
    out: &mut Vec<NodeDocument>,
) -> Result<(), TxError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_markdown_files_in_dir(vault_root, &path, out)?;
        } else if file_type.is_file() {
            let Ok(rel) = path.strip_prefix(vault_root) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if validate_rel_path(&rel_str).is_ok() {
                let raw = std::fs::read_to_string(&path)?;
                out.push(NodeDocument::parse(&rel_str, &raw));
            }
        }
    }
    Ok(())
}

fn load_main_documents_from_git(vault_root: &Path) -> Result<Vec<NodeDocument>, TxError> {
    let mut ls_args = vec!["ls-tree", "-r", "--name-only", REF_MAIN, "--"];
    ls_args.extend_from_slice(&VAULT_NODE_FOLDERS);
    let out = run_git(vault_root, &ls_args)?;

    let mut docs = Vec::new();
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if validate_rel_path(line).is_ok() {
            let spec = format!("{}:{}", REF_MAIN, line);
            let raw = run_git(vault_root, &["show", &spec])?;
            docs.push(NodeDocument::parse(line, &raw));
        }
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}
