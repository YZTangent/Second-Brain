use super::git::{
    create_detached_worktree, remove_worktree, rev_parse, run_git, sync_main_into_manual,
};
use super::session::{
    ensure_no_symlink_components, SessionRegistry, TxError, TxHandle, REF_MAIN, TRAILER_ACTOR,
    TRAILER_SESSION, TRAILER_TX,
};
use crate::model::NodeDocument;
use std::path::Path;
use uuid::Uuid;

pub fn begin_tx(
    vault_root: &Path,
    sessions: &SessionRegistry,
    owner_session_id: &str,
) -> Result<TxHandle, TxError> {
    if owner_session_id.trim().is_empty() {
        return Err(TxError::Validation(
            "owner_session_id must not be empty".to_string(),
        ));
    }

    let tx_id = format!("tx-{}", Uuid::new_v4().simple());
    let worktree_path = create_detached_worktree(vault_root, &tx_id)?;

    let handle = TxHandle {
        tx_id,
        owner_session_id: owner_session_id.to_string(),
        worktree_path,
    };
    sessions.insert(handle.clone());
    Ok(handle)
}

pub fn stage_upsert(
    sessions: &SessionRegistry,
    tx_id: &str,
    caller_session_id: &str,
    rel_path: &str,
    doc: &NodeDocument,
) -> Result<(), TxError> {
    let handle = sessions.verify_owner(tx_id, caller_session_id)?;
    write_validated_document(&handle.worktree_path, rel_path, doc)
}

pub fn stage_delete(
    sessions: &SessionRegistry,
    tx_id: &str,
    caller_session_id: &str,
    rel_path: &str,
) -> Result<(), TxError> {
    let handle = sessions.verify_owner(tx_id, caller_session_id)?;
    delete_validated_document(&handle.worktree_path, rel_path)
}

pub fn commit_tx(
    vault_root: &Path,
    sessions: &SessionRegistry,
    tx_id: &str,
    caller_session_id: &str,
    actor: &str,
    summary: &str,
) -> Result<String, TxError> {
    let handle = sessions.verify_owner(tx_id, caller_session_id)?;

    let commit_result = (|| -> Result<String, TxError> {
        run_git(&handle.worktree_path, &["add", "-A"])?;

        let clean_summary = sanitize_single_line(summary, "remember: update");
        let clean_actor = sanitize_single_line(actor, "agent");
        let clean_session = sanitize_single_line(caller_session_id, "unknown");
        let msg = format!(
            "{}\n\n{} {}\n{} {}\n{} {}",
            clean_summary,
            TRAILER_ACTOR,
            clean_actor,
            TRAILER_TX,
            tx_id,
            TRAILER_SESSION,
            clean_session
        );

        run_git(
            &handle.worktree_path,
            &["commit", "--allow-empty", "-m", &msg],
        )?;
        let sha = rev_parse(&handle.worktree_path, "HEAD")?;
        run_git(vault_root, &["update-ref", REF_MAIN, &sha])?;
        Ok(sha)
    })();

    sessions.remove(tx_id);
    let cleanup_res = remove_worktree(vault_root, &handle.worktree_path);

    let sha = commit_result?;
    cleanup_res?;
    sync_main_into_manual(vault_root)?;
    Ok(sha)
}

pub fn abort_tx(
    vault_root: &Path,
    sessions: &SessionRegistry,
    tx_id: &str,
    caller_session_id: &str,
) -> Result<(), TxError> {
    let handle = sessions.verify_owner(tx_id, caller_session_id)?;
    sessions.remove(tx_id);
    remove_worktree(vault_root, &handle.worktree_path)
}

pub fn direct_manual_upsert(
    vault_root: &Path,
    rel_path: &str,
    doc: &NodeDocument,
) -> Result<(), TxError> {
    write_validated_document(vault_root, rel_path, doc)
}

pub fn direct_manual_delete(vault_root: &Path, rel_path: &str) -> Result<(), TxError> {
    delete_validated_document(vault_root, rel_path)
}

fn write_validated_document(
    base_dir: &Path,
    rel_path: &str,
    doc: &NodeDocument,
) -> Result<(), TxError> {
    let target = ensure_no_symlink_components(base_dir, rel_path)?;
    let expected_prefix = format!("{}/", doc.meta.node_type.folder_name());
    if !rel_path.starts_with(&expected_prefix) {
        return Err(TxError::Validation(format!(
            "node type {:?} requires folder prefix '{}', got '{}'",
            doc.meta.node_type, expected_prefix, rel_path
        )));
    }
    let markdown = doc
        .to_markdown()
        .map_err(|e| TxError::Validation(e.to_string()))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, markdown)?;
    Ok(())
}

fn delete_validated_document(base_dir: &Path, rel_path: &str) -> Result<(), TxError> {
    let target = ensure_no_symlink_components(base_dir, rel_path)?;
    if target.exists() {
        std::fs::remove_file(&target)?;
    }
    Ok(())
}

fn sanitize_single_line(input: &str, fallback: &str) -> String {
    let cleaned: String = input
        .chars()
        .filter(|&c| c != '\r' && c != '\n' && c != '\0')
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}
