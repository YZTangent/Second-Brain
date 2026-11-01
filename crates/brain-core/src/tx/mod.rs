pub mod audit;
pub mod git;
pub mod lifecycle;
pub mod session;
pub mod vault;

pub use audit::AuditCommit;
pub use session::{TxError, TxHandle};

use crate::model::NodeDocument;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorktreeCoordinator {
    vault_root: PathBuf,
    sessions: session::SessionRegistry,
}

impl WorktreeCoordinator {
    pub fn new(vault_root: impl Into<PathBuf>) -> Result<Self, TxError> {
        let vault_root = vault_root.into();
        vault::ensure_or_init_vault(&vault_root)?;
        Ok(Self {
            vault_root,
            sessions: session::SessionRegistry::default(),
        })
    }

    pub fn vault_root(&self) -> &Path {
        &self.vault_root
    }

    pub fn current_branch(&self) -> Result<String, TxError> {
        git::branch_of(&self.vault_root)
    }

    pub fn begin_tx(&self, owner_session_id: &str) -> Result<TxHandle, TxError> {
        lifecycle::begin_tx(&self.vault_root, &self.sessions, owner_session_id)
    }

    pub fn stage_upsert(
        &self,
        tx_id: &str,
        caller_session_id: &str,
        rel_path: &str,
        doc: &NodeDocument,
    ) -> Result<(), TxError> {
        lifecycle::stage_upsert(&self.sessions, tx_id, caller_session_id, rel_path, doc)
    }

    pub fn stage_delete(
        &self,
        tx_id: &str,
        caller_session_id: &str,
        rel_path: &str,
    ) -> Result<(), TxError> {
        lifecycle::stage_delete(&self.sessions, tx_id, caller_session_id, rel_path)
    }

    pub fn commit_tx(
        &self,
        tx_id: &str,
        caller_session_id: &str,
        actor: &str,
        summary: &str,
    ) -> Result<String, TxError> {
        lifecycle::commit_tx(
            &self.vault_root,
            &self.sessions,
            tx_id,
            caller_session_id,
            actor,
            summary,
        )
    }

    pub fn abort_tx(&self, tx_id: &str, caller_session_id: &str) -> Result<(), TxError> {
        lifecycle::abort_tx(&self.vault_root, &self.sessions, tx_id, caller_session_id)
    }

    pub fn direct_manual_upsert(&self, rel_path: &str, doc: &NodeDocument) -> Result<(), TxError> {
        lifecycle::direct_manual_upsert(&self.vault_root, rel_path, doc)
    }

    pub fn direct_manual_delete(&self, rel_path: &str) -> Result<(), TxError> {
        lifecycle::direct_manual_delete(&self.vault_root, rel_path)
    }

    pub fn daily_stray_checkpoint(&self) -> Result<Option<String>, TxError> {
        audit::daily_stray_checkpoint(&self.vault_root)
    }

    pub fn list_audit_commits(
        &self,
        branch: &str,
        limit: usize,
    ) -> Result<Vec<AuditCommit>, TxError> {
        audit::list_audit_commits(&self.vault_root, branch, limit)
    }

    pub fn revert_commit(&self, sha: &str) -> Result<String, TxError> {
        audit::revert_commit(&self.vault_root, sha)
    }

    pub fn load_branch_documents(&self, branch: &str) -> Result<Vec<NodeDocument>, TxError> {
        audit::load_branch_documents(&self.vault_root, branch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeMap, NodeFrontmatter, NodeType};
    use tempfile::TempDir;

    fn sample_knowledge_doc(slug: &str, title: &str, body: &str) -> NodeDocument {
        NodeDocument {
            path: format!("knowledge/{}.md", slug),
            meta: NodeFrontmatter {
                id: format!("knowledge:{}", slug),
                node_type: NodeType::Knowledge,
                title: title.to_string(),
                status: None,
                due: None,
                bug: None,
                change: None,
                kind: None,
                tags: vec!["robotics".to_string()],
                edges: EdgeMap::default(),
                updated_at: "2026-11-01T00:00:00Z".to_string(),
            },
            body: body.to_string(),
            parse_error: false,
        }
    }

    #[test]
    fn test_vault_init_empty_reopen_and_abort_on_non_empty_non_vault() {
        let tmp = TempDir::new().unwrap();

        // 1. Initialize a missing vault directory
        let missing_vault = tmp.path().join("fresh-vault");
        let coord = WorktreeCoordinator::new(&missing_vault).unwrap();
        assert!(missing_vault.join(".second-brain.json").is_file());
        for dir in ["knowledge", "goals", "tasks", "journal", "relationships"] {
            assert!(missing_vault.join(dir).is_dir());
        }
        assert_eq!(coord.current_branch().unwrap(), "manual");
        let main_commits = coord.list_audit_commits("main", 10).unwrap();
        assert_eq!(main_commits.len(), 1);

        // 2. Re-opening an initialized vault succeeds and stays on manual
        let reopened = WorktreeCoordinator::new(&missing_vault).unwrap();
        assert_eq!(reopened.current_branch().unwrap(), "manual");

        // 2b. Initializing an existing empty directory succeeds
        let empty_dir = tmp.path().join("empty-dir");
        std::fs::create_dir_all(&empty_dir).unwrap();
        let from_empty = WorktreeCoordinator::new(&empty_dir).unwrap();
        assert_eq!(from_empty.current_branch().unwrap(), "manual");
        assert!(empty_dir.join(".second-brain.json").is_file());

        // 3. Non-empty non-vault directory aborts with InvalidVaultDirectory without modifying files
        let non_vault = tmp.path().join("existing-project");
        std::fs::create_dir_all(&non_vault).unwrap();
        let existing_file = non_vault.join("README.md");
        std::fs::write(&existing_file, "# Existing Repo\n").unwrap();

        let err = WorktreeCoordinator::new(&non_vault).unwrap_err();
        assert!(matches!(err, TxError::InvalidVaultDirectory(_, _)));
        assert!(!non_vault.join(".second-brain.json").exists());
        assert!(!non_vault.join(".git").exists());
        assert_eq!(
            std::fs::read_to_string(&existing_file).unwrap(),
            "# Existing Repo\n"
        );
    }

    #[test]
    fn test_worktree_atomic_commit_session_guard_and_revert() {
        let tmp = TempDir::new().unwrap();
        let vault_dir = tmp.path().join("vault");
        let coord = WorktreeCoordinator::new(&vault_dir).unwrap();
        assert_eq!(coord.current_branch().unwrap(), "manual");

        // Stray edit written directly onto manual branch (uncommitted)
        let stray_doc =
            sample_knowledge_doc("stray-note", "Stray Note", "Uncommitted manual edit.");
        coord
            .direct_manual_upsert("knowledge/stray-note.md", &stray_doc)
            .unwrap();

        // Overlapping manual edit that will also be written by the worktree transaction
        let overlap_manual = sample_knowledge_doc("pd-gains", "PD Gains Draft", "Manual draft.");
        coord
            .direct_manual_upsert("knowledge/pd-gains.md", &overlap_manual)
            .unwrap();

        // Begin transaction for session-A
        let tx = coord.begin_tx("session-A").unwrap();
        assert!(tx.worktree_path.exists());
        assert!(tx
            .worktree_path
            .starts_with(vault_dir.join(".brain").join("tx")));

        let doc = sample_knowledge_doc("pd-gains", "PD Gains", "Actuator gain scheduling.");

        // Mismatched session-B must fail on stage_upsert, stage_delete, commit_tx, and abort_tx
        assert!(matches!(
            coord.stage_upsert(&tx.tx_id, "session-B", "knowledge/pd-gains.md", &doc),
            Err(TxError::SessionMismatch { .. })
        ));
        assert!(matches!(
            coord.stage_delete(&tx.tx_id, "session-B", "knowledge/pd-gains.md"),
            Err(TxError::SessionMismatch { .. })
        ));
        assert!(matches!(
            coord.commit_tx(&tx.tx_id, "session-B", "agent:jetski", "remember: pd gains"),
            Err(TxError::SessionMismatch { .. })
        ));
        assert!(matches!(
            coord.abort_tx(&tx.tx_id, "session-B"),
            Err(TxError::SessionMismatch { .. })
        ));

        // Owner session-A stages and commits atomically (sync_main_into_manual auto-commits overlapping pd-gains on manual while preserving uncommitted stray-note)
        coord
            .stage_upsert(&tx.tx_id, "session-A", "knowledge/pd-gains.md", &doc)
            .unwrap();
        let commit_sha = coord
            .commit_tx(&tx.tx_id, "session-A", "agent:jetski", "remember: pd gains")
            .unwrap();
        assert!(!tx.worktree_path.exists());
        assert_eq!(coord.current_branch().unwrap(), "manual");

        // Verify branch isolation: main has pd-gains only; manual has both pd-gains (synced from main) and uncommitted stray-note
        let main_docs = coord.load_branch_documents("main").unwrap();
        assert_eq!(main_docs.len(), 1);
        assert_eq!(main_docs[0].meta.id, "knowledge:pd-gains");
        assert_eq!(main_docs[0].meta.title, "PD Gains");

        let manual_docs = coord.load_branch_documents("manual").unwrap();
        assert_eq!(manual_docs.len(), 2);
        assert!(manual_docs
            .iter()
            .any(|d| d.meta.id == "knowledge:pd-gains" && d.meta.title == "PD Gains"));
        assert!(manual_docs
            .iter()
            .any(|d| d.meta.id == "knowledge:stray-note"));

        // Verify commit trailers in audit history on main
        let commits = coord.list_audit_commits("main", 10).unwrap();
        assert_eq!(commits[0].sha, commit_sha);
        assert_eq!(commits[0].summary, "remember: pd gains");
        assert_eq!(commits[0].actor.as_deref(), Some("agent:jetski"));
        assert_eq!(commits[0].tx_id.as_deref(), Some(tx.tx_id.as_str()));
        assert_eq!(commits[0].session_id.as_deref(), Some("session-A"));

        // Daily stray checkpoint commits stray-note on manual and merges into main
        let checkpoint_sha = coord.daily_stray_checkpoint().unwrap();
        assert!(checkpoint_sha.is_some());
        let main_docs_after_ckpt = coord.load_branch_documents("main").unwrap();
        assert_eq!(main_docs_after_ckpt.len(), 2);

        // Revert the pd-gains transaction commit and verify it is removed from both main and manual while stray-note remains
        coord.revert_commit(&commit_sha).unwrap();
        let main_after_revert = coord.load_branch_documents("main").unwrap();
        assert_eq!(main_after_revert.len(), 1);
        assert_eq!(main_after_revert[0].meta.id, "knowledge:stray-note");

        let manual_after_revert = coord.load_branch_documents("manual").unwrap();
        assert_eq!(manual_after_revert.len(), 1);
        assert_eq!(manual_after_revert[0].meta.id, "knowledge:stray-note");

        // Verify stage_delete in a new transaction removes stray-note
        let tx_del = coord.begin_tx("session-A").unwrap();
        coord
            .stage_delete(&tx_del.tx_id, "session-A", "knowledge/stray-note.md")
            .unwrap();
        coord
            .commit_tx(
                &tx_del.tx_id,
                "session-A",
                "agent:jetski",
                "remember: delete stray-note",
            )
            .unwrap();
        assert!(coord.load_branch_documents("main").unwrap().is_empty());
        assert!(coord.load_branch_documents("manual").unwrap().is_empty());
    }

    #[test]
    fn test_security_validation_for_paths_branches_and_shas() {
        let tmp = TempDir::new().unwrap();
        let coord = WorktreeCoordinator::new(tmp.path().join("vault")).unwrap();
        let doc = sample_knowledge_doc("safe", "Safe", "Body");

        let tx = coord.begin_tx("sess-1").unwrap();
        for bad_path in [
            "../escape.md",
            "knowledge/../../escape.md",
            "/etc/passwd.md",
            ".brain/secret.md",
            "other/file.md",
            "knowledge/not-markdown.txt",
        ] {
            assert!(matches!(
                coord.stage_upsert(&tx.tx_id, "sess-1", bad_path, &doc),
                Err(TxError::Validation(_))
            ));
        }
        coord.abort_tx(&tx.tx_id, "sess-1").unwrap();

        assert!(matches!(
            coord.load_branch_documents("--evil-flag"),
            Err(TxError::Validation(_))
        ));
        assert!(matches!(
            coord.revert_commit("--no-commit"),
            Err(TxError::Validation(_))
        ));
        assert!(matches!(
            coord.revert_commit("HEAD~1"),
            Err(TxError::Validation(_))
        ));
    }
}
