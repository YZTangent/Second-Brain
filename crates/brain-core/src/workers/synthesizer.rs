use super::harness::HeadlessHarness;
use crate::model::NodeDocument;
use crate::tx::session::BRANCH_MAIN;
use crate::tx::WorktreeCoordinator;
use anyhow::Result;
use std::time::Duration;

const SYNTHESIZER_SESSION_ID: &str = "worker:synthesizer";
const SYNTHESIZE_COMMIT_SUMMARY: &str = "synthesize: daily knowledge consolidation";
const TEMP_PARSE_PATH: &str = "knowledge/synthesized.md";
const NO_CHANGES_SENTINEL: &str = "NO_CHANGES";

pub async fn run_daily_checkpoint_and_synthesize(
    coord: &WorktreeCoordinator,
    harness: &dyn HeadlessHarness,
    timeout: Duration,
) -> Result<Option<String>> {
    let checkpoint_sha = coord.daily_stray_checkpoint()?;
    let docs = coord.load_branch_documents(BRANCH_MAIN)?;
    if docs.is_empty() {
        return Ok(checkpoint_sha);
    }

    let mut prompt = String::from(
        "Consolidate and normalize the following Second Brain notes. Return a single updated Markdown note with YAML frontmatter, or NO_CHANGES:\n\n",
    );
    for doc in &docs {
        prompt.push_str(&format!(
            "- {} ({}): {}\n",
            doc.meta.id, doc.path, doc.meta.title
        ));
    }

    let tx = coord.begin_tx(SYNTHESIZER_SESSION_ID)?;
    let raw_response = match harness.run_prompt(&prompt, timeout).await {
        Ok(resp) => resp,
        Err(err) => {
            let _ = coord.abort_tx(&tx.tx_id, SYNTHESIZER_SESSION_ID);
            return Err(err);
        }
    };

    let trimmed = raw_response.trim();
    if trimmed.is_empty() || trimmed == NO_CHANGES_SENTINEL {
        coord.abort_tx(&tx.tx_id, SYNTHESIZER_SESSION_ID)?;
        return Ok(checkpoint_sha);
    }

    let parsed = NodeDocument::parse(TEMP_PARSE_PATH, trimmed);
    if parsed.parse_error || parsed.meta.id.is_empty() || parsed.validate().is_err() {
        coord.abort_tx(&tx.tx_id, SYNTHESIZER_SESSION_ID)?;
        return Ok(checkpoint_sha);
    }

    let slug = parsed
        .meta
        .id
        .split_once(':')
        .map(|(_, s)| s)
        .unwrap_or(&parsed.meta.id);
    let rel_path = format!("{}/{}.md", parsed.meta.node_type.folder_name(), slug);

    if let Err(err) = coord.stage_upsert(&tx.tx_id, SYNTHESIZER_SESSION_ID, &rel_path, &parsed) {
        let _ = coord.abort_tx(&tx.tx_id, SYNTHESIZER_SESSION_ID);
        return Err(err.into());
    }

    let actor = format!("harness:{}", harness.harness_id());
    match coord.commit_tx(
        &tx.tx_id,
        SYNTHESIZER_SESSION_ID,
        &actor,
        SYNTHESIZE_COMMIT_SUMMARY,
    ) {
        Ok(sha) => Ok(Some(sha)),
        Err(err) => {
            let _ = coord.abort_tx(&tx.tx_id, SYNTHESIZER_SESSION_ID);
            Err(err.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeMap, NodeDocument, NodeFrontmatter, NodeType};
    use async_trait::async_trait;
    use tempfile::TempDir;

    struct MockSynthHarness {
        response: String,
    }

    #[async_trait]
    impl HeadlessHarness for MockSynthHarness {
        fn harness_id(&self) -> &str {
            "mock-synth"
        }

        async fn run_prompt(&self, _prompt: &str, _timeout: Duration) -> Result<String> {
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn test_run_daily_checkpoint_and_synthesize_with_mock_harness() {
        let tmp = TempDir::new().unwrap();
        let coord = WorktreeCoordinator::new(tmp.path().join("vault")).unwrap();

        // Create an uncommitted stray manual edit on `manual`
        let stray_doc = NodeDocument {
            path: "knowledge/raw-note.md".to_string(),
            meta: NodeFrontmatter {
                id: "knowledge:raw-note".to_string(),
                node_type: NodeType::Knowledge,
                title: "Raw Note".to_string(),
                status: None,
                due: None,
                bug: None,
                change: None,
                kind: None,
                tags: vec!["robotics".to_string()],
                edges: EdgeMap::default(),
                updated_at: "2026-11-01T00:00:00Z".to_string(),
            },
            body: "Uncommitted stray note.".to_string(),
            parse_error: false,
        };
        coord
            .direct_manual_upsert("knowledge/raw-note.md", &stray_doc)
            .unwrap();

        let harness = MockSynthHarness {
            response: "---\nid: knowledge:consolidated\ntype: knowledge\ntitle: Consolidated Note\ntags: [robotics]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\nConsolidated from [[knowledge:raw-note]].\n".to_string(),
        };

        let sha = run_daily_checkpoint_and_synthesize(&coord, &harness, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(sha.is_some());

        let main_docs = coord.load_branch_documents("main").unwrap();
        assert_eq!(main_docs.len(), 2);
        assert!(main_docs.iter().any(|d| d.meta.id == "knowledge:raw-note"));
        assert!(main_docs
            .iter()
            .any(|d| d.meta.id == "knowledge:consolidated"));
    }
}
