pub mod server;
pub mod workers;

pub use server::{EmbeddingConfig, ServerConfig};
pub use workers::{HarnessConfig, ObserverConfig, SyncSourceConfig};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainConfig {
    pub server: ServerConfig,
    pub embedding: EmbeddingConfig,
    pub harness: HarnessConfig,
    #[serde(default)]
    pub observers: BTreeMap<String, ObserverConfig>,
    #[serde(default)]
    pub sync: BTreeMap<String, SyncSourceConfig>,
}

impl BrainConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&raw)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_load_default_config_toml() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("config.toml");
        let cfg = BrainConfig::load(&config_path).unwrap();
        assert_eq!(cfg.server.port, 7420);
        assert_eq!(cfg.embedding.provider, "gemini");
        assert_eq!(cfg.harness.default, "jetski");
        assert!(cfg.observers.get("jetski").unwrap().enabled);
        assert!(!cfg.observers.get("pi").unwrap().enabled);
        assert!(cfg.sync.get("buganizer").unwrap().enabled);
        assert!(cfg.sync.get("critique").unwrap().enabled);
    }
}
