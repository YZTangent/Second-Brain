use super::chunker::ChunkRecord;
use std::collections::HashMap;

const INITIAL_MIN_COSINE_SCORE: f32 = -1.0;
const MIN_NORM_EPSILON: f32 = 1e-8;

pub fn score_chunks_cosine(
    query_vec: &[f32],
    chunks: &[ChunkRecord],
) -> (HashMap<String, f32>, HashMap<String, String>) {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut snippets: HashMap<String, String> = HashMap::new();

    for chunk in chunks {
        let cos = cosine_similarity(query_vec, &chunk.embedding);
        let entry = scores
            .entry(chunk.node_id.clone())
            .or_insert(INITIAL_MIN_COSINE_SCORE);
        if cos > *entry {
            *entry = cos;
            snippets.insert(
                chunk.node_id.clone(),
                format!(
                    "{}: {}",
                    chunk.heading,
                    chunk.text.lines().next().unwrap_or("")
                ),
            );
        }
    }
    (scores, snippets)
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..len {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    dot / (na.sqrt() * nb.sqrt()).max(MIN_NORM_EPSILON)
}
