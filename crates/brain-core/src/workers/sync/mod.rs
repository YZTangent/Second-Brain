pub mod buganizer;
pub mod critique;

pub use buganizer::BuganizerSyncSource;
pub use critique::CritiqueSyncSource;

use crate::index::{ExternalBug, ExternalCache, ExternalChange};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const EXTERNAL_CACHE_FILE: &str = "external_cache.json";
const SOURCE_BUGANIZER: &str = "buganizer";
const SOURCE_CRITIQUE: &str = "critique";
const DEFAULT_BUG_PRIORITY: &str = "P2";
const GCERT_ERROR_MARKERS: [&str; 6] = [
    "gcert",
    "loas",
    "ticket expired",
    "credentials expired",
    "reauthenticate",
    "unauthenticated",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: Option<String>,
}

#[async_trait]
pub trait ExternalSyncSource: Send + Sync {
    fn source_id(&self) -> &str;
    async fn poll(&self) -> Result<Vec<ExternalItem>>;
}

pub fn external_cache_path(brain_dir: &Path) -> PathBuf {
    brain_dir.join(EXTERNAL_CACHE_FILE)
}

pub fn apply_sync_items(cache: &mut ExternalCache, source_id: &str, items: Vec<ExternalItem>) {
    match source_id {
        SOURCE_BUGANIZER => {
            cache.bugs.clear();
            for item in items {
                cache.bugs.insert(
                    item.id.clone(),
                    ExternalBug {
                        id: item.id,
                        title: item.title,
                        priority: item
                            .priority
                            .unwrap_or_else(|| DEFAULT_BUG_PRIORITY.to_string()),
                        status: item.status,
                    },
                );
            }
        }
        SOURCE_CRITIQUE => {
            cache.changes.clear();
            for item in items {
                cache.changes.insert(
                    item.id.clone(),
                    ExternalChange {
                        id: item.id,
                        title: item.title,
                        status: item.status,
                    },
                );
            }
        }
        _ => {}
    }
}

pub fn load_external_cache(brain_dir: &Path) -> Result<ExternalCache> {
    let path = external_cache_path(brain_dir);
    if path.exists() {
        let bytes = std::fs::read(&path)?;
        if let Ok(cache) = serde_json::from_slice::<ExternalCache>(&bytes) {
            return Ok(cache);
        }
    }
    Ok(ExternalCache::default())
}

pub fn save_external_cache(brain_dir: &Path, cache: &ExternalCache) -> Result<()> {
    let path = external_cache_path(brain_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(cache)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn is_gcert_expired_error(err_msg: &str) -> bool {
    let lower = err_msg.to_ascii_lowercase();
    GCERT_ERROR_MARKERS.iter().any(|m| lower.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_external_sync_cache_hydration_and_gcert_detection() {
        let tmp = TempDir::new().unwrap();
        let mut cache = load_external_cache(tmp.path()).unwrap();
        assert!(cache.bugs.is_empty());
        assert!(cache.changes.is_empty());

        let bug_items = BuganizerSyncSource::parse_output(
            "b/101 P1 ASSIGNED Fix ankle torque saturation\n102 P2 NEW Tune hip damping\n",
        );
        assert_eq!(bug_items.len(), 2);
        apply_sync_items(&mut cache, "buganizer", bug_items);

        let cl_items = CritiqueSyncSource::parse_output(
            "cl/500 PENDING_REVIEW feat: add biquad notch filter\n501 SUBMITTED fix: joint limit\n",
        );
        assert_eq!(cl_items.len(), 2);
        apply_sync_items(&mut cache, "critique", cl_items);

        save_external_cache(tmp.path(), &cache).unwrap();
        let reloaded = load_external_cache(tmp.path()).unwrap();
        assert_eq!(reloaded.bugs.len(), 2);
        assert_eq!(reloaded.bugs.get("b/101").unwrap().priority, "P1");
        assert_eq!(reloaded.bugs.get("b/102").unwrap().status, "NEW");
        assert_eq!(reloaded.changes.len(), 2);
        assert_eq!(
            reloaded.changes.get("cl/500").unwrap().status,
            "PENDING_REVIEW"
        );

        assert!(is_gcert_expired_error(
            "LOAS credentials expired; please run gcert"
        ));
        assert!(is_gcert_expired_error("Error: ticket expired"));
        assert!(!is_gcert_expired_error("network connection reset by peer"));
    }
}
