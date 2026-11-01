use super::chunker::ChunkRecord;
use crate::model::NodeDocument;
use std::collections::HashMap;

const BM25_IDF_SMOOTHING: f32 = 0.5;
const BM25_K1: f32 = 1.2;

#[derive(Default, Clone)]
pub struct Bm25Index {
    pub inverted: HashMap<String, Vec<(String, usize)>>, // token -> Vec<(node_id, tf)>
    pub total_chunks: usize,
}

impl Bm25Index {
    pub fn rebuild(&mut self, docs: &[NodeDocument], chunks: &[ChunkRecord]) {
        self.inverted.clear();
        self.total_chunks = chunks.len().max(1);
        let doc_map: HashMap<&str, &NodeDocument> =
            docs.iter().map(|d| (d.meta.id.as_str(), d)).collect();

        for chunk in chunks {
            let meta_str = doc_map
                .get(chunk.node_id.as_str())
                .map(|d| format!("{} {} {}", d.meta.id, d.meta.title, d.meta.tags.join(" ")))
                .unwrap_or_default();
            let text = format!("{} {} {}", meta_str, chunk.heading, chunk.text);
            let mut freqs: HashMap<String, usize> = HashMap::new();
            for tok in tokenize(&text) {
                *freqs.entry(tok).or_insert(0) += 1;
            }
            for (tok, tf) in freqs {
                self.inverted
                    .entry(tok)
                    .or_default()
                    .push((chunk.node_id.clone(), tf));
            }
        }
    }

    pub fn score_query(&self, query: &str) -> HashMap<String, f32> {
        let mut scores = HashMap::new();
        let n = self.total_chunks as f32;
        for tok in tokenize(query) {
            if let Some(postings) = self.inverted.get(&tok) {
                let df = postings.len() as f32;
                let idf = ((n - df + BM25_IDF_SMOOTHING) / (df + BM25_IDF_SMOOTHING) + 1.0).ln();
                for (node_id, tf) in postings {
                    let tf_f = *tf as f32;
                    let s = idf * ((tf_f * (BM25_K1 + 1.0)) / (tf_f + BM25_K1));
                    *scores.entry(node_id.clone()).or_insert(0.0) += s;
                }
            }
        }
        scores
    }
}

pub fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}
