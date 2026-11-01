pub mod bm25;
pub mod chunker;
pub mod graph;
pub mod vector;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeDocument;
    use std::collections::HashMap;

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
}
