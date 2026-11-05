use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessConfig {
    pub default: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverConfig {
    pub enabled: bool,
    pub watch_glob: String,
    pub rrf_threshold: f32,
    pub cosine_threshold: f32,
    #[serde(default)]
    pub agentapi_bin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSourceConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    #[serde(default)]
    pub query: Option<String>,
}
