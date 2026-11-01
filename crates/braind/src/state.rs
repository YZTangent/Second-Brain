use brain_core::config::BrainConfig;
use brain_core::index::{BrainIndex, ExternalCache};
use brain_core::tx::session::{validate_branch, BRANCH_MAIN, BRANCH_MANUAL};
use brain_core::tx::{TxError, WorktreeCoordinator};
use brain_core::workers::harness::{HeadlessHarness, JetskiHeadlessHarness};
use brain_core::workers::sync::load_external_cache;
use brain_core::workers::{WorkerRunState, WorkerSupervisor};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

const EVENT_CHANNEL_CAPACITY: usize = 256;
const BRAIN_RUNTIME_DIR: &str = ".brain";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BrainEvent {
    TxCommitted {
        tx_id: String,
        sha: String,
        summary: String,
    },
    NodeUpdated {
        branch: String,
        node_id: String,
    },
    MemoryPushed {
        session_id: String,
        node_id: String,
    },
    WorkerStateChanged {
        worker_id: String,
        state: WorkerRunState,
    },
    ParseError {
        branch: String,
        path: String,
    },
}

#[derive(Clone)]
pub struct AppState {
    pub config: BrainConfig,
    pub coord: WorktreeCoordinator,
    pub main_index: Arc<RwLock<BrainIndex>>,
    pub manual_index: Arc<RwLock<BrainIndex>>,
    pub external_cache: Arc<RwLock<ExternalCache>>,
    pub supervisor: WorkerSupervisor,
    pub harness: Arc<dyn HeadlessHarness>,
    pub events_tx: broadcast::Sender<BrainEvent>,
}

impl AppState {
    pub fn new(config: BrainConfig, coord: WorktreeCoordinator) -> Result<Self, TxError> {
        let harness = Arc::new(JetskiHeadlessHarness::new(
            &config.harness.command,
            config.harness.args.clone(),
        ));
        Self::with_harness(config, coord, harness)
    }

    pub fn with_harness(
        config: BrainConfig,
        coord: WorktreeCoordinator,
        harness: Arc<dyn HeadlessHarness>,
    ) -> Result<Self, TxError> {
        let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let supervisor = WorkerSupervisor::from_config(&config);
        let brain_dir = coord.vault_root().join(BRAIN_RUNTIME_DIR);
        let initial_ext = load_external_cache(&brain_dir).unwrap_or_default();

        let state = Self {
            config,
            coord,
            main_index: Arc::new(RwLock::new(BrainIndex::default())),
            manual_index: Arc::new(RwLock::new(BrainIndex::default())),
            external_cache: Arc::new(RwLock::new(initial_ext)),
            supervisor,
            harness,
            events_tx,
        };
        state.reload_indexes()?;
        Ok(state)
    }

    pub fn reload_indexes(&self) -> Result<(), TxError> {
        let main_docs = self.coord.load_branch_documents(BRANCH_MAIN)?;
        let manual_docs = self.coord.load_branch_documents(BRANCH_MANUAL)?;

        for doc in &main_docs {
            if doc.parse_error {
                self.emit_event(BrainEvent::ParseError {
                    branch: BRANCH_MAIN.to_string(),
                    path: doc.path.clone(),
                });
            }
        }
        for doc in &manual_docs {
            if doc.parse_error {
                self.emit_event(BrainEvent::ParseError {
                    branch: BRANCH_MANUAL.to_string(),
                    path: doc.path.clone(),
                });
            }
        }

        let empty_vecs = HashMap::new();
        {
            let mut guard = self.main_index.write().unwrap_or_else(|e| e.into_inner());
            guard.rebuild_from_docs(&main_docs, &empty_vecs);
        }
        {
            let mut guard = self.manual_index.write().unwrap_or_else(|e| e.into_inner());
            guard.rebuild_from_docs(&manual_docs, &empty_vecs);
        }
        Ok(())
    }

    pub fn resolve_branch(branch: Option<&str>) -> Result<&'static str, TxError> {
        let b = branch.unwrap_or(BRANCH_MAIN);
        validate_branch(b)?;
        if b == BRANCH_MANUAL {
            Ok(BRANCH_MANUAL)
        } else {
            Ok(BRANCH_MAIN)
        }
    }

    pub fn index_for_branch(
        &self,
        branch: Option<&str>,
    ) -> Result<&Arc<RwLock<BrainIndex>>, TxError> {
        let resolved = Self::resolve_branch(branch)?;
        if resolved == BRANCH_MANUAL {
            Ok(&self.manual_index)
        } else {
            Ok(&self.main_index)
        }
    }

    pub fn emit_event(&self, event: BrainEvent) {
        let _ = self.events_tx.send(event);
    }
}
