use crate::embedding::deterministic_embed;
use crate::model::NodeDocument;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_FALLBACK_EMBED_DIMS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub node_id: String,
    pub chunk_idx: usize,
    pub heading: String,
    pub text: String,
    pub blake3_hash: String,
    pub embedding: Vec<f32>,
}

pub fn split_chunks(
    doc: &NodeDocument,
    precomputed_vecs: &HashMap<String, Vec<f32>>,
) -> Vec<ChunkRecord> {
    let mut chunks = Vec::new();
    let mut current_heading = doc.meta.title.clone();
    let mut current_lines = Vec::new();

    for line in doc.body.lines() {
        if line.starts_with("# ") || line.starts_with("## ") {
            if !current_lines.is_empty() {
                let text = current_lines.join("\n");
                chunks.push(make_chunk(
                    &doc.meta.id,
                    chunks.len(),
                    &current_heading,
                    &text,
                    precomputed_vecs,
                ));
                current_lines.clear();
            }
            current_heading = line.trim_start_matches('#').trim().to_string();
        } else {
            current_lines.push(line);
        }
    }
    if !current_lines.is_empty() || chunks.is_empty() {
        let text = current_lines.join("\n");
        chunks.push(make_chunk(
            &doc.meta.id,
            chunks.len(),
            &current_heading,
            &text,
            precomputed_vecs,
        ));
    }
    chunks
}

fn make_chunk(
    node_id: &str,
    chunk_idx: usize,
    heading: &str,
    text: &str,
    precomputed_vecs: &HashMap<String, Vec<f32>>,
) -> ChunkRecord {
    let full_text = format!("{}\n{}", heading, text);
    let hash = blake3::hash(full_text.as_bytes()).to_hex().to_string();
    let key = format!("{}::{}::{}", node_id, chunk_idx, hash);
    let embedding = precomputed_vecs
        .get(&key)
        .cloned()
        .unwrap_or_else(|| deterministic_embed(&full_text, DEFAULT_FALLBACK_EMBED_DIMS));
    ChunkRecord {
        node_id: node_id.to_string(),
        chunk_idx,
        heading: heading.to_string(),
        text: text.to_string(),
        blake3_hash: hash,
        embedding,
    }
}
