use crate::model::NodeType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const RRF_K: f32 = 60.0;
const PRIMARY_RANK_WEIGHT: f32 = 1.0;
const GRAPH_WEIGHT: f32 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub node_id: String,
    pub title: String,
    pub node_type: NodeType,
    pub rrf_score: f32,
    pub max_cosine: f32,
    pub snippet: String,
}

pub fn fuse_rrf(
    bm25_scores: &HashMap<String, f32>,
    vec_scores: &HashMap<String, f32>,
    graph_scores: &HashMap<String, f32>,
) -> HashMap<String, f32> {
    let mut totals: HashMap<String, f32> = HashMap::new();
    for (scores, weight) in [
        (bm25_scores, PRIMARY_RANK_WEIGHT),
        (vec_scores, PRIMARY_RANK_WEIGHT),
        (graph_scores, GRAPH_WEIGHT),
    ] {
        for (rank, id) in rank_descending(scores).iter().enumerate() {
            if scores.get(id).copied().unwrap_or(0.0) > 0.0 {
                *totals.entry(id.clone()).or_insert(0.0) += weight / (RRF_K + (rank + 1) as f32);
            }
        }
    }
    totals
}

pub fn rank_descending(scores: &HashMap<String, f32>) -> Vec<String> {
    let mut pairs: Vec<(&String, &f32)> = scores.iter().collect();
    pairs.sort_by(|a, b| {
        b.1.partial_cmp(a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    pairs.into_iter().map(|(k, _)| k.clone()).collect()
}
