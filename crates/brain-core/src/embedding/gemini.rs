use super::{deterministic_embed, EmbeddingProvider};
use anyhow::Result;
use async_trait::async_trait;

pub struct GeminiEmbeddingProvider {
    pub model: String,
    pub dimensions: usize,
}

#[async_trait]
impl EmbeddingProvider for GeminiEmbeddingProvider {
    fn provider_id(&self) -> &str {
        "gemini"
    }
    fn model_id(&self) -> &str {
        &self.model
    }
    fn dimensions(&self) -> usize {
        self.dimensions
    }
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| deterministic_embed(t, self.dimensions))
            .collect())
    }
}
