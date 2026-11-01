pub mod harness;
pub mod observer;
pub mod sync;
pub mod synthesizer;

use crate::config::BrainConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerRunState {
    Running,
    Paused,
    Idle,
    GcertExpired,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub id: String,
    pub state: WorkerRunState,
    pub logs: VecDeque<String>,
}

const MAX_WORKER_LOGS: usize = 200;
const WORKER_SYNTHESIZER: &str = "synthesizer";

#[derive(Debug, Clone, Default)]
pub struct WorkerSupervisor {
    workers: Arc<Mutex<BTreeMap<String, WorkerInfo>>>,
}

impl WorkerSupervisor {
    pub fn from_config(cfg: &BrainConfig) -> Self {
        let mut map = BTreeMap::new();

        for (name, obs) in &cfg.observers {
            let id = format!("observer:{}", name);
            let state = if obs.enabled {
                WorkerRunState::Running
            } else {
                WorkerRunState::Paused
            };
            map.insert(
                id.clone(),
                WorkerInfo {
                    id,
                    state,
                    logs: VecDeque::new(),
                },
            );
        }

        for (name, src) in &cfg.sync {
            let id = format!("sync:{}", name);
            let state = if src.enabled {
                WorkerRunState::Running
            } else {
                WorkerRunState::Paused
            };
            map.insert(
                id.clone(),
                WorkerInfo {
                    id,
                    state,
                    logs: VecDeque::new(),
                },
            );
        }

        map.insert(
            WORKER_SYNTHESIZER.to_string(),
            WorkerInfo {
                id: WORKER_SYNTHESIZER.to_string(),
                state: WorkerRunState::Idle,
                logs: VecDeque::new(),
            },
        );

        Self {
            workers: Arc::new(Mutex::new(map)),
        }
    }

    pub fn list_workers(&self) -> Vec<WorkerInfo> {
        let guard = self.workers.lock().unwrap_or_else(|e| e.into_inner());
        guard.values().cloned().collect()
    }

    pub fn get_worker(&self, worker_id: &str) -> Option<WorkerInfo> {
        let guard = self.workers.lock().unwrap_or_else(|e| e.into_inner());
        guard.get(worker_id).cloned()
    }

    pub fn set_worker_state(&self, worker_id: &str, state: WorkerRunState) -> bool {
        let mut guard = self.workers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(info) = guard.get_mut(worker_id) {
            info.state = state;
            true
        } else {
            false
        }
    }

    pub fn append_log(&self, worker_id: &str, line: impl Into<String>) -> bool {
        let mut guard = self.workers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(info) = guard.get_mut(worker_id) {
            while info.logs.len() >= MAX_WORKER_LOGS {
                info.logs.pop_front();
            }
            info.logs.push_back(line.into());
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_supervisor_registers_per_harness_and_per_source_workers() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("config.toml");
        let cfg = BrainConfig::load(&config_path).unwrap();

        let sup = WorkerSupervisor::from_config(&cfg);
        let workers = sup.list_workers();
        let by_id: BTreeMap<String, WorkerInfo> =
            workers.into_iter().map(|w| (w.id.clone(), w)).collect();

        assert_eq!(
            by_id.get("observer:jetski").map(|w| &w.state),
            Some(&WorkerRunState::Running)
        );
        assert_eq!(
            by_id.get("observer:pi").map(|w| &w.state),
            Some(&WorkerRunState::Paused)
        );
        assert_eq!(
            by_id.get("sync:buganizer").map(|w| &w.state),
            Some(&WorkerRunState::Running)
        );
        assert_eq!(
            by_id.get("sync:critique").map(|w| &w.state),
            Some(&WorkerRunState::Running)
        );
        assert_eq!(
            by_id.get("synthesizer").map(|w| &w.state),
            Some(&WorkerRunState::Idle)
        );

        assert!(sup.set_worker_state("sync:buganizer", WorkerRunState::GcertExpired));
        assert!(sup.append_log("sync:buganizer", "LOAS ticket expired"));
        let updated = sup.get_worker("sync:buganizer").unwrap();
        assert_eq!(updated.state, WorkerRunState::GcertExpired);
        assert_eq!(updated.logs.len(), 1);
        assert_eq!(updated.logs[0], "LOAS ticket expired");

        assert!(!sup.set_worker_state("unknown:worker", WorkerRunState::Paused));
        assert!(!sup.append_log("unknown:worker", "ignored"));
    }
}
