pub mod cache;
pub mod gemini;

pub use cache::EmbeddingCache;
pub use gemini::GeminiEmbeddingProvider;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
    fn dimensions(&self) -> usize;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

pub fn deterministic_embed(text: &str, dims: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; dims.max(1)];
    for token in text.split_whitespace() {
        let hash = blake3::hash(token.to_lowercase().as_bytes());
        let bytes = hash.as_bytes();
        let idx =
            (u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize) % vec.len();
        vec[idx] += 1.0;
    }
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut vec {
            *x /= norm;
        }
    }
    vec
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct MockProvider {
        provider: String,
        model: String,
        val: f32,
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockProvider {
        fn provider_id(&self) -> &str {
            &self.provider
        }
        fn model_id(&self) -> &str {
            &self.model
        }
        fn dimensions(&self) -> usize {
            4
        }
        async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![self.val; 4]).collect())
        }
    }

    #[tokio::test]
    async fn test_multi_provider_cache_isolation() {
        let tmp = TempDir::new().unwrap();
        let p1 = MockProvider {
            provider: "gemini".into(),
            model: "v1".into(),
            val: 1.0,
        };
        let p2 = MockProvider {
            provider: "local".into(),
            model: "bge".into(),
            val: 0.5,
        };

        let mut cache1 = EmbeddingCache::load_or_create(tmp.path(), &p1).unwrap();
        let v1 = cache1
            .get_or_embed(&p1, "knowledge:a", 0, "hello world")
            .await
            .unwrap();
        cache1.save(tmp.path()).unwrap();
        assert_eq!(v1, vec![1.0; 4]);

        let mut cache2 = EmbeddingCache::load_or_create(tmp.path(), &p2).unwrap();
        let v2 = cache2
            .get_or_embed(&p2, "knowledge:a", 0, "hello world")
            .await
            .unwrap();
        cache2.save(tmp.path()).unwrap();
        assert_eq!(v2, vec![0.5; 4]);

        let cache1_reloaded = EmbeddingCache::load_or_create(tmp.path(), &p1).unwrap();
        assert_eq!(cache1_reloaded.entries.len(), 1);
    }

    #[test]
    fn test_cache_file_path_sanitizes_traversal_chars() {
        let tmp = TempDir::new().unwrap();
        let path = EmbeddingCache::cache_file_path(
            tmp.path(),
            "../evil/provider",
            "models/gemini:1.5\\pro",
        );
        assert_eq!(
            path,
            tmp.path()
                .join("embeddings")
                .join("___evil_provider_models_gemini_1_5_pro.bin")
        );
    }

    #[tokio::test]
    async fn test_gemini_embedding_provider_deterministic() {
        let provider = GeminiEmbeddingProvider {
            model: "gemini-embedding-2-preview".into(),
            dimensions: 8,
        };
        assert_eq!(provider.provider_id(), "gemini");
        assert_eq!(provider.model_id(), "gemini-embedding-2-preview");
        assert_eq!(provider.dimensions(), 8);

        let vecs = provider
            .embed_batch(&["rust async", "rust async"])
            .await
            .unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0].len(), 8);
        assert_eq!(vecs[0], vecs[1]);
    }
}
