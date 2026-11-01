use super::EmbeddingProvider;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddingCache {
    pub provider_id: String,
    pub model_id: String,
    pub dimensions: usize,
    pub entries: BTreeMap<String, Vec<f32>>,
}

impl EmbeddingCache {
    fn sanitize_id(value: &str) -> String {
        value
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub fn cache_file_path(brain_dir: &Path, provider_id: &str, model_id: &str) -> PathBuf {
        let safe_provider = Self::sanitize_id(provider_id);
        let safe_model = Self::sanitize_id(model_id);
        brain_dir
            .join("embeddings")
            .join(format!("{}_{}.bin", safe_provider, safe_model))
    }

    pub fn load_or_create(brain_dir: &Path, provider: &dyn EmbeddingProvider) -> Result<Self> {
        let path = Self::cache_file_path(brain_dir, provider.provider_id(), provider.model_id());
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            if let Ok(cache) = serde_json::from_slice::<Self>(&bytes) {
                return Ok(cache);
            }
        }
        Ok(Self {
            provider_id: provider.provider_id().to_string(),
            model_id: provider.model_id().to_string(),
            dimensions: provider.dimensions(),
            entries: BTreeMap::new(),
        })
    }

    pub fn save(&self, brain_dir: &Path) -> Result<()> {
        let path = Self::cache_file_path(brain_dir, &self.provider_id, &self.model_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec(&self)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub async fn get_or_embed(
        &mut self,
        provider: &dyn EmbeddingProvider,
        node_id: &str,
        chunk_idx: usize,
        text: &str,
    ) -> Result<Vec<f32>> {
        let hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        let key = format!("{}::{}::{}", node_id, chunk_idx, hash);
        if let Some(vec) = self.entries.get(&key) {
            return Ok(vec.clone());
        }
        let mut batch = provider.embed_batch(&[text]).await?;
        let vec = batch.pop().unwrap_or_default();
        self.entries.insert(key, vec.clone());
        Ok(vec)
    }
}
