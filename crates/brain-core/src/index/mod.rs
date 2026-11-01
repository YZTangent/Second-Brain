pub mod bm25;
pub mod chunker;
pub mod graph;
pub mod rrf;
pub mod timeline;
pub mod vector;

pub use rrf::SearchHit;
pub use timeline::{
    ExternalBug, ExternalCache, ExternalChange, HydratedTask, TimelineProjection,
};

use crate::embedding::deterministic_embed;
use crate::model::NodeDocument;
use std::collections::HashMap;

const SEED_EXPANSION_LIMIT: usize = 5;

#[derive(Default, Clone)]
pub struct BrainIndex {
    pub graph_store: graph::GraphStore,
    pub bm25: bm25::Bm25Index,
    pub chunks: Vec<chunker::ChunkRecord>,
}

impl BrainIndex {
    pub fn rebuild_from_docs(
        &mut self,
        docs: &[NodeDocument],
        precomputed_vecs: &HashMap<String, Vec<f32>>,
    ) {
        let mut all_chunks = Vec::new();
        for doc in docs {
            all_chunks.extend(chunker::split_chunks(doc, precomputed_vecs));
        }
        self.graph_store.rebuild(docs);
        self.bm25.rebuild(docs, &all_chunks);
        self.chunks = all_chunks;
    }

    pub fn search_rrf(&self, query: &str, limit: usize) -> Vec<rrf::SearchHit> {
        let bm25_scores = self.bm25.score_query(query);
        let dims = self
            .chunks
            .first()
            .map(|c| c.embedding.len())
            .unwrap_or(chunker::DEFAULT_FALLBACK_EMBED_DIMS);
        let query_vec = deterministic_embed(query, dims);
        let (vec_scores, snippets) = vector::score_chunks_cosine(&query_vec, &self.chunks);

        let mut seeds: Vec<String> = Vec::new();
        for scores in [&bm25_scores, &vec_scores] {
            for id in rrf::rank_descending(scores)
                .into_iter()
                .filter(|id| scores.get(id).copied().unwrap_or(0.0) > 0.0)
                .take(SEED_EXPANSION_LIMIT)
            {
                if !seeds.contains(&id) {
                    seeds.push(id);
                }
            }
        }

        let graph_scores = self.graph_store.expand_1hop(&seeds);
        let fused = rrf::fuse_rrf(&bm25_scores, &vec_scores, &graph_scores);
        let ranked = rrf::rank_descending(&fused);

        ranked
            .into_iter()
            .take(limit)
            .filter_map(|id| {
                let doc = self.get_node(&id)?;
                Some(rrf::SearchHit {
                    node_id: doc.meta.id.clone(),
                    title: doc.meta.title.clone(),
                    node_type: doc.meta.node_type,
                    rrf_score: fused.get(&id).copied().unwrap_or(0.0),
                    max_cosine: vec_scores.get(&id).copied().unwrap_or(0.0),
                    snippet: snippets.get(&id).cloned().unwrap_or_else(|| {
                        doc.body.lines().next().unwrap_or("").to_string()
                    }),
                })
            })
            .collect()
    }

    pub fn build_timeline(&self, external: &timeline::ExternalCache) -> timeline::TimelineProjection {
        timeline::project_timeline(&self.graph_store, external)
    }

    pub fn get_node(&self, node_id: &str) -> Option<&NodeDocument> {
        self.graph_store
            .node_indices
            .get(node_id)
            .and_then(|&idx| self.graph_store.graph.node_weight(idx))
    }

    pub fn list_nodes(&self) -> Vec<NodeDocument> {
        let mut nodes: Vec<NodeDocument> =
            self.graph_store.graph.node_weights().cloned().collect();
        nodes.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        nodes
    }

    pub fn neighbors_1hop(&self, node_id: &str) -> Vec<String> {
        let mut neighbors: Vec<String> = self
            .graph_store
            .expand_1hop(&[node_id.to_string()])
            .into_keys()
            .collect();
        neighbors.sort();
        neighbors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunker_bm25_and_graph_edges() {
        let doc1 = NodeDocument::parse(
            "goals/g1.md",
            "---\nid: goal:g1\ntype: goal\ntitle: Goal 1\ntags: [robotics]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\n# Intro\nAlpha\n## Details\nBeta biquad\n",
        );
        let doc2 = NodeDocument::parse(
            "tasks/t1.md",
            "---\nid: task:t1\ntype: task\ntitle: Task 1\nstatus: open\ntags: [robotics]\nedges:\n  implements: [goal:g1]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\nLinks to [[goal:g1]].\n",
        );
        let chunks = chunker::split_chunks(&doc1, &HashMap::new());
        assert_eq!(chunks.len(), 2);

        let mut g = graph::GraphStore::default();
        g.rebuild(&[doc1.clone(), doc2.clone()]);
        let neighbors = g.expand_1hop(&["goal:g1".to_string()]);
        assert!(neighbors.contains_key("task:t1"));

        let mut bm = bm25::Bm25Index::default();
        bm.rebuild(&[doc1], &chunks);
        let scores = bm.score_query("biquad");
        assert!(scores.get("goal:g1").copied().unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn test_vector_cosine_and_chunk_scoring() {
        let doc = NodeDocument::parse(
            "knowledge/k1.md",
            "---\nid: knowledge:k1\ntype: knowledge\ntitle: Filters\ntags: [dsp]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\n# Lowpass\nButterworth design\n## Notch\nBiquad notch filter\n",
        );
        let chunks = chunker::split_chunks(&doc, &HashMap::new());
        assert_eq!(chunks.len(), 2);

        let query_vec = chunks[1].embedding.clone();
        let (scores, snippets) = vector::score_chunks_cosine(&query_vec, &chunks);
        let score = scores.get("knowledge:k1").copied().unwrap_or(0.0);
        assert!((score - 1.0).abs() < 1e-5);
        assert_eq!(
            snippets.get("knowledge:k1").map(String::as_str),
            Some("Notch: Biquad notch filter")
        );
        assert_eq!(vector::cosine_similarity(&[], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn test_hybrid_rrf_search_and_timeline_projection() {
        let doc1 = NodeDocument::parse(
            "goals/g1.md",
            "---\nid: goal:g1\ntype: goal\ntitle: Bimanual Manipulation\ntags: [robotics]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\n# Overview\nBimanual humanoid control.\n",
        );
        let doc2 = NodeDocument::parse(
            "tasks/t1.md",
            "---\nid: task:t1\ntype: task\ntitle: Port RL controller\nstatus: in_progress\ndue: \"2026-11-15\"\nbug: b/100\nchange: cl/200\ntags: [robotics]\nedges:\n  implements: [goal:g1]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\nTune biquad filter for [[goal:g1]].\n",
        );
        let doc3 = NodeDocument::parse(
            "journal/2026-11-01.md",
            "---\nid: journal:2026-11-01\ntype: journal\ntitle: Daily Log\ntags: [log]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\nWorked on [[task:t1]].\n",
        );

        let mut idx = BrainIndex::default();
        idx.rebuild_from_docs(&[doc1, doc2, doc3], &HashMap::new());

        let hits = idx.search_rrf("biquad filter", 5);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].node_id, "task:t1");
        // 1-hop neighbor goal:g1 or journal:2026-11-01 should also appear in hits via graph expansion
        assert!(hits.iter().any(|h| h.node_id == "goal:g1"));

        let mut ext = timeline::ExternalCache::default();
        ext.bugs.insert(
            "b/100".to_string(),
            timeline::ExternalBug {
                id: "b/100".to_string(),
                title: "RL tracking instability".to_string(),
                priority: "P1".to_string(),
                status: "ASSIGNED".to_string(),
            },
        );
        ext.bugs.insert(
            "b/999".to_string(),
            timeline::ExternalBug {
                id: "b/999".to_string(),
                title: "Unlinked bug".to_string(),
                priority: "P2".to_string(),
                status: "NEW".to_string(),
            },
        );
        ext.changes.insert(
            "cl/200".to_string(),
            timeline::ExternalChange {
                id: "cl/200".to_string(),
                title: "feat: biquad filter".to_string(),
                status: "PENDING_REVIEW".to_string(),
            },
        );

        let tl = idx.build_timeline(&ext);
        assert_eq!(tl.tasks.len(), 1);
        assert_eq!(tl.tasks[0].id, "task:t1");
        assert_eq!(tl.tasks[0].inherited_priority.as_deref(), Some("P1"));
        assert_eq!(tl.tasks[0].bug_status.as_deref(), Some("ASSIGNED"));
        assert_eq!(tl.tasks[0].change_status.as_deref(), Some("PENDING_REVIEW"));
        assert_eq!(tl.unlinked_bugs.len(), 1);
        assert_eq!(tl.unlinked_bugs[0].id, "b/999");
        assert_eq!(tl.journals.len(), 1);

        assert!(idx.get_node("task:t1").is_some());
        assert_eq!(idx.list_nodes().len(), 3);
        assert_eq!(
            idx.neighbors_1hop("task:t1"),
            vec!["goal:g1".to_string(), "journal:2026-11-01".to_string()]
        );
    }
}
