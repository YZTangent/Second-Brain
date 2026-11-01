pub mod jetski;

pub use jetski::JetskiObserverAdapter;

use crate::index::BrainIndex;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedTurn {
    pub session_id: String,
    pub user_text: String,
}

#[async_trait]
pub trait ObserverHarnessAdapter: Send + Sync {
    fn harness_id(&self) -> &str;
    fn watch_paths(&self) -> &[PathBuf];
    fn parse_new_turns(&self, raw_bytes: &[u8]) -> Result<Vec<ObservedTurn>>;
    async fn push_memory(&self, session_id: &str, message: &str) -> Result<()>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObserverState {
    pub byte_offsets: BTreeMap<String, u64>,
    pub pushed_nodes_by_session: BTreeMap<String, BTreeSet<String>>,
}

const OBSERVER_STATE_FILE: &str = "observer_state.json";
const DEFAULT_OBSERVER_SEARCH_LIMIT: usize = 5;

impl ObserverState {
    pub fn state_file_path(brain_dir: &Path) -> PathBuf {
        brain_dir.join(OBSERVER_STATE_FILE)
    }

    pub fn load(brain_dir: &Path) -> Result<Self> {
        let path = Self::state_file_path(brain_dir);
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            if let Ok(state) = serde_json::from_slice::<Self>(&bytes) {
                return Ok(state);
            }
        }
        Ok(Self::default())
    }

    pub fn save(&self, brain_dir: &Path) -> Result<()> {
        let path = Self::state_file_path(brain_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

pub async fn evaluate_and_push_turns(
    adapter: &dyn ObserverHarnessAdapter,
    index: &BrainIndex,
    state: &mut ObserverState,
    raw_bytes: &[u8],
    rrf_threshold: f32,
    cosine_threshold: f32,
) -> Result<Vec<String>> {
    let turns = adapter.parse_new_turns(raw_bytes)?;
    let mut pushed_ids = Vec::new();

    for turn in turns {
        if turn.session_id.trim().is_empty() || turn.user_text.trim().is_empty() {
            continue;
        }
        let hits = index.search_rrf(&turn.user_text, DEFAULT_OBSERVER_SEARCH_LIMIT);
        for hit in hits {
            if hit.rrf_score < rrf_threshold || hit.max_cosine < cosine_threshold {
                continue;
            }
            let already_pushed = state
                .pushed_nodes_by_session
                .get(&turn.session_id)
                .is_some_and(|set| set.contains(&hit.node_id));
            if already_pushed {
                continue;
            }

            let message = format!(
                "[Second Brain Recall] [[{}]] {} — {}",
                hit.node_id, hit.title, hit.snippet
            );
            adapter.push_memory(&turn.session_id, &message).await?;
            state
                .pushed_nodes_by_session
                .entry(turn.session_id.clone())
                .or_default()
                .insert(hit.node_id.clone());
            pushed_ids.push(hit.node_id);
        }
    }

    Ok(pushed_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeDocument;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    struct MockObserverAdapter {
        paths: Vec<PathBuf>,
        pushed: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait]
    impl ObserverHarnessAdapter for MockObserverAdapter {
        fn harness_id(&self) -> &str {
            "mock"
        }

        fn watch_paths(&self) -> &[PathBuf] {
            &self.paths
        }

        fn parse_new_turns(&self, raw_bytes: &[u8]) -> Result<Vec<ObservedTurn>> {
            let adapter = JetskiObserverAdapter::new(vec![]);
            adapter.parse_new_turns(raw_bytes)
        }

        async fn push_memory(&self, session_id: &str, message: &str) -> Result<()> {
            self.pushed
                .lock()
                .unwrap()
                .push((session_id.to_string(), message.to_string()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_observer_pushes_high_confidence_memory_once_per_session() {
        let doc = NodeDocument::parse(
            "knowledge/biquad.md",
            "---\nid: knowledge:biquad\ntype: knowledge\ntitle: Biquad Filter\ntags: [dsp]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\n# Biquad Filter\nUse biquad notch filter for resonance.\n",
        );
        let mut index = BrainIndex::default();
        index.rebuild_from_docs(&[doc], &HashMap::new());

        let pushed_records = Arc::new(Mutex::new(Vec::new()));
        let adapter = MockObserverAdapter {
            paths: vec![],
            pushed: pushed_records.clone(),
        };

        let tmp = TempDir::new().unwrap();
        let mut state = ObserverState::load(tmp.path()).unwrap();

        let raw_jsonl =
            br#"{"session_id":"sess-42","role":"user","text":"biquad notch filter resonance"}
{"session_id":"sess-42","role":"assistant","text":"assistant reply ignored"}
"#;

        // First evaluation pushes knowledge:biquad
        let pushed_first =
            evaluate_and_push_turns(&adapter, &index, &mut state, raw_jsonl, 0.025, 0.5)
                .await
                .unwrap();
        assert_eq!(pushed_first, vec!["knowledge:biquad".to_string()]);
        assert_eq!(pushed_records.lock().unwrap().len(), 1);
        assert_eq!(pushed_records.lock().unwrap()[0].0, "sess-42");
        assert!(pushed_records.lock().unwrap()[0]
            .1
            .contains("[[knowledge:biquad]]"));

        // Persist and reload observer state
        state.save(tmp.path()).unwrap();
        let mut reloaded_state = ObserverState::load(tmp.path()).unwrap();

        // Second evaluation in the same session must deduplicate and not push again
        let pushed_second =
            evaluate_and_push_turns(&adapter, &index, &mut reloaded_state, raw_jsonl, 0.025, 0.5)
                .await
                .unwrap();
        assert!(pushed_second.is_empty());
        assert_eq!(pushed_records.lock().unwrap().len(), 1);

        // Evaluation in a different session pushes once for that new session
        let raw_other_session =
            br#"{"session_id":"sess-99","type":"USER_INPUT","content":"biquad notch filter resonance"}"#;
        let pushed_other = evaluate_and_push_turns(
            &adapter,
            &index,
            &mut reloaded_state,
            raw_other_session,
            0.025,
            0.5,
        )
        .await
        .unwrap();
        assert_eq!(pushed_other, vec!["knowledge:biquad".to_string()]);
        assert_eq!(pushed_records.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_jetski_observer_parses_turns_and_validates_session_id() {
        let adapter = JetskiObserverAdapter::with_agentapi_bin(vec![], "true");
        let raw = br#"
{"session_id":"abc_123-def","role":"user","text":"hello world"}
{"session_id":"abc_123-def","role":"model","text":"ignored model output"}
{"conversation_id":"conv-2","type":"USER_INPUT","content":"second turn"}
"#;
        let turns = adapter.parse_new_turns(raw).unwrap();
        assert_eq!(
            turns,
            vec![
                ObservedTurn {
                    session_id: "abc_123-def".to_string(),
                    user_text: "hello world".to_string(),
                },
                ObservedTurn {
                    session_id: "conv-2".to_string(),
                    user_text: "second turn".to_string(),
                },
            ]
        );

        // Valid session_id succeeds with `true` binary
        adapter
            .push_memory("abc_123-def", "recalled note")
            .await
            .unwrap();

        // Invalid session_ids must be rejected before spawning subprocess
        for bad_session in ["", "-flag-injection", "bad space", "semi;colon", "../path"] {
            assert!(adapter.push_memory(bad_session, "msg").await.is_err());
        }
    }
}
