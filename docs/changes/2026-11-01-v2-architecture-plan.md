# Second Brain v2 Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Second Brain v2 headless Rust daemon (`braind`), core in-memory graph/hybrid search library (`brain-core`), CLI/MCP bridge (`brainctl`), and `/remember` + `/recall` skills using small, single-responsibility module hierarchies.

**Architecture:** A multi-crate Rust workspace (`brain-core`, `braind`, `brainctl`) where every subsystem (`config/`, `model/`, `embedding/`, `index/`, `tx/`, `workers/`, `api/`) is decomposed into focused submodule files. Manages a Git-tracked Markdown vault (checked out on `manual`, canonical ledger on `main`) with session-scoped ephemeral Git worktree transactions (`.brain/tx/<tx_id>`), an in-memory `petgraph` + BM25 + vector RRF index backed by per-provider/model embedding caches, pluggable supervised workers (`observer:jetski`, `sync:buganizer`, `sync:critique`, `synthesizer`), and a decoupled OpenAPI REST/SSE + MCP boundary.

**Tech Stack:** Rust 2021 (`tokio`, `axum`, `petgraph`, `serde`, `serde_yaml`, `serde_json`, `toml`, `blake3`, `notify`, `uuid`, `chrono`, `thiserror`, `anyhow`, `clap`, `tempfile`, `tower`), Git worktrees.

---

## File Structure Overview

```
second-brain-v2/
├── Cargo.toml
├── config.toml
├── crates/
│   ├── brain-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config/
│   │       │   ├── mod.rs              # BrainConfig loader
│   │       │   ├── server.rs           # ServerConfig & EmbeddingConfig
│   │       │   └── workers.rs          # HarnessConfig, ObserverConfig, SyncSourceConfig
│   │       ├── model/
│   │       │   ├── mod.rs              # Module exports
│   │       │   ├── types.rs            # NodeType, TaskStatus, RelationshipKind, EdgeMap
│   │       │   ├── frontmatter.rs      # NodeFrontmatter & schema validation rules
│   │       │   ├── document.rs         # NodeDocument YAML parser/serializer & parse_error recovery
│   │       │   └── wikilink.rs         # Inline [[WikiLink]] parser & normalizer
│   │       ├── embedding/
│   │       │   ├── mod.rs              # EmbeddingProvider trait & deterministic fallback
│   │       │   ├── cache.rs            # Per-provider/model .brain/embeddings/<provider>_<model>.bin store
│   │       │   └── gemini.rs           # GeminiEmbeddingProvider adapter
│   │       ├── index/
│   │       │   ├── mod.rs              # BrainIndex coordinator
│   │       │   ├── chunker.rs          # Markdown # / ## heading chunker & blake3 hashing
│   │       │   ├── graph.rs            # petgraph::StableDiGraph builder & 1-hop neighborhood walk
│   │       │   ├── bm25.rs             # In-memory BM25 inverted index & tokenizer
│   │       │   ├── vector.rs           # Cosine similarity scanner
│   │       │   ├── rrf.rs              # Reciprocal Rank Fusion merger & SearchHit
│   │       │   └── timeline.rs         # TimelineProjection & ExternalCache (bug P-level / change join)
│   │       ├── tx/
│   │       │   ├── mod.rs              # WorktreeCoordinator struct & TxError
│   │       │   ├── vault.rs            # Vault validator (.second-brain.json), init-if-empty, abort-if-non-empty-non-vault
│   │       │   ├── git.rs              # Low-level git CLI runner & branch helpers (manual vs main)
│   │       │   ├── session.rs          # TxHandle & best-effort owner_session_id guard
│   │       │   ├── lifecycle.rs        # begin_tx, stage_upsert, stage_delete, commit_tx, abort_tx
│   │       │   └── audit.rs            # daily_stray_checkpoint, list_audit_commits, revert_commit
│   │       └── workers/
│   │           ├── mod.rs              # WorkerSupervisor & WorkerRunState
│   │           ├── harness/
│   │           │   ├── mod.rs          # HeadlessHarness trait
│   │           │   └── jetski.rs       # JetskiHeadlessHarness (`jetski -p`)
│   │           ├── observer/
│   │           │   ├── mod.rs          # ObserverHarnessAdapter trait, ObserverState & turn evaluator
│   │           │   └── jetski.rs       # JetskiObserverAdapter (transcript.jsonl + agentapi push)
│   │           ├── sync/
│   │           │   ├── mod.rs          # ExternalSyncSource trait
│   │           │   ├── buganizer.rs    # BuganizerSyncSource (`issue` CLI)
│   │           │   └── critique.rs     # CritiqueSyncSource (`critique`/`g4` CLI)
│   │           └── synthesizer.rs      # Daily checkpoint + HeadlessHarness consolidation runner
│   ├── braind/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs                 # Daemon entrypoint & TCP listener
│   │       ├── state.rs                # AppState (main_index, manual_index, coord, supervisor, events_tx)
│   │       └── api/
│   │           ├── mod.rs              # Router assembly & /api/v1/openapi.json
│   │           ├── tx.rs               # Agent session-scoped /api/v1/tx/* handlers
│   │           ├── search.rs           # GET /api/v1/search & GET /api/v1/search/deep handlers
│   │           ├── nodes.rs            # GET /api/v1/nodes, UI-only PUT/DELETE /api/v1/nodes/:id, /api/v1/graph, /api/v1/timeline
│   │           ├── audit.rs            # UI-only GET /api/v1/audit & POST /api/v1/audit/:sha/revert
│   │           └── workers.rs          # UI-only /api/v1/workers & /api/v1/events SSE stream
│   └── brainctl/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # CLI entrypoint
│           └── mcp.rs                  # Restricted Agent MCP tool definitions & stdio bridge
└── skills/
    ├── remember/SKILL.md
    └── recall/SKILL.md
```

---

### Task 1: Workspace Scaffold & Modular `config/` Hierarchy

**Files:**
- Create: `Cargo.toml`
- Create: `config.toml`
- Create: `crates/brain-core/Cargo.toml`
- Create: `crates/brain-core/src/lib.rs`
- Create: `crates/brain-core/src/config/mod.rs`
- Create: `crates/brain-core/src/config/server.rs`
- Create: `crates/brain-core/src/config/workers.rs`
- Create: `crates/braind/Cargo.toml`
- Create: `crates/braind/src/main.rs`
- Create: `crates/brainctl/Cargo.toml`
- Create: `crates/brainctl/src/main.rs`

- [x] **Step 1: Write the failing test**

In `crates/brain-core/src/config/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_load_default_config_toml() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = manifest_dir.parent().unwrap().parent().unwrap().join("config.toml");
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
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p brain-core`
Expected: FAIL because workspace manifests and `config/` files do not exist yet.

- [x] **Step 3: Write minimal implementation**

Write `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [
    "crates/brain-core",
    "crates/braind",
    "crates/brainctl",
]

[workspace.dependencies]
anyhow = "1.0"
async-trait = "0.1"
axum = { version = "0.7", features = ["macros", "ws"] }
blake3 = "1.5"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4.5", features = ["derive"] }
notify = "6.1"
petgraph = "0.6"
regex = "1.10"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
tempfile = "3.10"
thiserror = "1.0"
tokio = { version = "1.38", features = ["full"] }
tokio-stream = { version = "0.1", features = ["sync"] }
toml = "0.8"
tower = { version = "0.4", features = ["util"] }
uuid = { version = "1.8", features = ["v4", "serde"] }
```

Write `config.toml`:
```toml
[server]
host = "127.0.0.1"
port = 7420

[embedding]
provider = "gemini"
model = "text-embedding-004"
dimensions = 768

[harness]
default = "jetski"
command = "jetski"
args = ["-p"]

[observers.jetski]
enabled = true
watch_glob = "~/.gemini/jetski/brain/*/transcript.jsonl"
rrf_threshold = 0.025
cosine_threshold = 0.72

[observers.pi]
enabled = false
watch_glob = "~/.pi/sessions/*.jsonl"
rrf_threshold = 0.025
cosine_threshold = 0.72

[sync.buganizer]
enabled = true
interval_secs = 1800
query = "assignee:me status:open"

[sync.critique]
enabled = true
interval_secs = 1800
```

Write `crates/brain-core/src/config/server.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
}
```

Write `crates/brain-core/src/config/workers.rs`:
```rust
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSourceConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    #[serde(default)]
    pub query: Option<String>,
}
```

Write `crates/brain-core/src/config/mod.rs`:
```rust
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
```

Write `crates/brain-core/src/lib.rs`:
```rust
pub mod config;
```

Create stub `crates/braind/Cargo.toml` (`src/main.rs`) and `crates/brainctl/Cargo.toml` (`src/main.rs`).

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p brain-core`
Expected: PASS (1 test passing).

- [x] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [x] **Step 6: Commit**

```bash
git add Cargo.toml config.toml crates/
git commit -m "feat(config): scaffold workspace and modular config hierarchy"
```

---

### Task 2: Modular `model/` Hierarchy (5 Node Types, Frontmatter, Document Parser, WikiLinks)

**Files:**
- Create: `crates/brain-core/src/model/mod.rs`
- Create: `crates/brain-core/src/model/types.rs`
- Create: `crates/brain-core/src/model/frontmatter.rs`
- Create: `crates/brain-core/src/model/document.rs`
- Create: `crates/brain-core/src/model/wikilink.rs`
- Modify: `crates/brain-core/src/lib.rs`

- [x] **Step 1: Write the failing test**

In `crates/brain-core/src/model/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_task_node_with_bug_and_change() {
        let raw = r#"---
id: task:port-bimanual-rl
type: task
title: Port bimanual RL controller
status: in_progress
due: 2026-11-15
bug: b/345678901
change: cl/678901234
tags: [robotics, rl]
edges:
  implements: [goal:q4-bimanual]
  depends_on: [knowledge:pd-gains]
updated_at: "2026-11-01T14:15:00Z"
---
Body referencing [[knowledge:mit-motor-control]] and [[relationship:alice-smith]].
"#;
        let doc = NodeDocument::parse("tasks/port-bimanual-rl.md", raw);
        assert!(!doc.parse_error);
        assert_eq!(doc.meta.id, "task:port-bimanual-rl");
        assert_eq!(doc.meta.node_type, NodeType::Task);
        assert_eq!(doc.meta.status, Some(TaskStatus::InProgress));
        assert_eq!(doc.meta.bug.as_deref(), Some("b/345678901"));
        assert_eq!(doc.meta.change.as_deref(), Some("cl/678901234"));
        assert_eq!(
            doc.extract_wikilinks(),
            vec![
                "knowledge:mit-motor-control".to_string(),
                "relationship:alice-smith".to_string()
            ]
        );
    }

    #[test]
    fn test_non_task_rejects_status_in_validation() {
        let raw = r#"---
id: knowledge:pd-gains
type: knowledge
title: PD Gains
status: open
tags: [controls]
updated_at: "2026-11-01T14:15:00Z"
---
Content
"#;
        let doc = NodeDocument::parse("knowledge/pd-gains.md", raw);
        assert!(doc.validate().is_err());
    }

    #[test]
    fn test_relationship_node_roundtrip() {
        let meta = NodeFrontmatter {
            id: "relationship:alice-smith".to_string(),
            node_type: NodeType::Relationship,
            title: "Alice Smith".to_string(),
            status: None,
            due: None,
            bug: None,
            change: None,
            kind: Some(RelationshipKind::Person),
            tags: vec!["robotics".to_string()],
            edges: EdgeMap {
                affiliated_with: vec!["relationship:mit-lab".to_string()],
                ..Default::default()
            },
            updated_at: "2026-11-01T14:15:00Z".to_string(),
        };
        let doc = NodeDocument {
            path: "relationships/alice-smith.md".to_string(),
            meta,
            body: "Worked together on actuator control.".to_string(),
            parse_error: false,
        };
        assert!(doc.validate().is_ok());
        let serialized = doc.to_markdown().unwrap();
        let reparsed = NodeDocument::parse("relationships/alice-smith.md", &serialized);
        assert!(!reparsed.parse_error);
        assert_eq!(reparsed.meta.kind, Some(RelationshipKind::Person));
        assert_eq!(
            reparsed.meta.edges.affiliated_with,
            vec!["relationship:mit-lab".to_string()]
        );
    }

    #[test]
    fn test_malformed_frontmatter_sets_parse_error_without_panic() {
        let raw = "---\ninvalid: [unclosed yaml\n---\nFallback body text.";
        let doc = NodeDocument::parse("knowledge/broken.md", raw);
        assert!(doc.parse_error);
        assert!(doc.body.contains("Fallback body text."));
    }
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p brain-core`
Expected: FAIL because `model/` submodules do not exist yet.

- [x] **Step 3: Write minimal implementation**

Write `crates/brain-core/src/model/types.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Knowledge,
    Goal,
    Task,
    Journal,
    Relationship,
}

impl NodeType {
    pub fn folder_name(&self) -> &'static str {
        match self {
            NodeType::Knowledge => "knowledge",
            NodeType::Goal => "goals",
            NodeType::Task => "tasks",
            NodeType::Journal => "journal",
            NodeType::Relationship => "relationships",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    InProgress,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Person,
    Org,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeMap {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affiliated_with: Vec<String>,
}
```

Write `crates/brain-core/src/model/frontmatter.rs`:
```rust
use super::types::{EdgeMap, NodeType, RelationshipKind, TaskStatus};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeFrontmatter {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<RelationshipKind>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub edges: EdgeMap,
    pub updated_at: String,
}

impl NodeFrontmatter {
    pub fn validate(&self) -> Result<()> {
        if self.node_type != NodeType::Task {
            if self.status.is_some()
                || self.due.is_some()
                || self.bug.is_some()
                || self.change.is_some()
            {
                bail!("only task nodes may contain status, due, bug, or change fields");
            }
        } else if self.status.is_none() {
            bail!("task node must specify a status field");
        }
        if self.node_type != NodeType::Relationship && self.kind.is_some() {
            bail!("only relationship nodes may specify kind");
        }
        Ok(())
    }
}
```

Write `crates/brain-core/src/model/wikilink.rs`:
```rust
use regex::Regex;
use std::sync::OnceLock;

pub fn extract_wikilinks(markdown_body: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"\[\[([^\[\]#|]+)(?:#[^\[\]|]+)?(?:\|[^\[\]]+)?\]\]").unwrap()
    });
    let mut out = Vec::new();
    for cap in re.captures_iter(markdown_body) {
        let link = cap[1].trim().to_string();
        if !out.contains(&link) {
            out.push(link);
        }
    }
    out
}
```

Write `crates/brain-core/src/model/document.rs`:
```rust
use super::frontmatter::NodeFrontmatter;
use super::types::{EdgeMap, NodeType};
use super::wikilink::extract_wikilinks;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDocument {
    pub path: String,
    pub meta: NodeFrontmatter,
    pub body: String,
    pub parse_error: bool,
}

impl NodeDocument {
    pub fn parse(path: &str, raw: &str) -> Self {
        let trimmed = raw.trim_start();
        if let Some(rest) = trimmed.strip_prefix("---\n") {
            if let Some(end_idx) = rest.find("\n---") {
                let yaml_str = &rest[..end_idx];
                let after = &rest[end_idx + 4..];
                let body = after.strip_prefix('\n').unwrap_or(after).to_string();
                if let Ok(meta) = serde_yaml::from_str::<NodeFrontmatter>(yaml_str) {
                    return Self {
                        path: path.to_string(),
                        meta,
                        body,
                        parse_error: false,
                    };
                }
            }
        }
        Self {
            path: path.to_string(),
            meta: NodeFrontmatter {
                id: format!("corrupt:{}", path),
                node_type: NodeType::Knowledge,
                title: path.to_string(),
                status: None,
                due: None,
                bug: None,
                change: None,
                kind: None,
                tags: vec![],
                edges: EdgeMap::default(),
                updated_at: String::new(),
            },
            body: raw.to_string(),
            parse_error: true,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.parse_error {
            bail!("document has YAML frontmatter parse error");
        }
        self.meta.validate()
    }

    pub fn to_markdown(&self) -> Result<String> {
        self.validate()?;
        let yaml = serde_yaml::to_string(&self.meta)?;
        Ok(format!("---\n{}---\n{}\n", yaml, self.body.trim_end()))
    }

    pub fn extract_wikilinks(&self) -> Vec<String> {
        extract_wikilinks(&self.body)
    }
}
```

Write `crates/brain-core/src/model/mod.rs`:
```rust
pub mod document;
pub mod frontmatter;
pub mod types;
pub mod wikilink;

pub use document::NodeDocument;
pub use frontmatter::NodeFrontmatter;
pub use types::{EdgeMap, NodeType, RelationshipKind, TaskStatus};
pub use wikilink::extract_wikilinks;
```

Update `crates/brain-core/src/lib.rs`:
```rust
pub mod config;
pub mod model;
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p brain-core`
Expected: PASS (5 tests passing).

- [x] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [x] **Step 6: Commit**

```bash
git add crates/brain-core/src/
git commit -m "feat(model): add modular 5-type node model, frontmatter validator, and wikilink parser"
```

---

### Task 3: Modular `embedding/` Hierarchy (Trait, Per-Provider Cache, Gemini Adapter)

**Files:**
- Create: `crates/brain-core/src/embedding/mod.rs`
- Create: `crates/brain-core/src/embedding/cache.rs`
- Create: `crates/brain-core/src/embedding/gemini.rs`
- Modify: `crates/brain-core/src/lib.rs`

- [x] **Step 1: Write the failing test**

In `crates/brain-core/src/embedding/mod.rs`:
```rust
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
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p brain-core`
Expected: FAIL because `embedding/` submodules do not exist yet.

- [x] **Step 3: Write minimal implementation**

Write `crates/brain-core/src/embedding/mod.rs`:
```rust
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
        let idx = (u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize) % vec.len();
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
```

Write `crates/brain-core/src/embedding/cache.rs`:
```rust
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
    pub fn cache_file_path(brain_dir: &Path, provider_id: &str, model_id: &str) -> PathBuf {
        let safe_provider = provider_id.replace(['/', ':'], "_");
        let safe_model = model_id.replace(['/', ':'], "_");
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
```

Write `crates/brain-core/src/embedding/gemini.rs`:
```rust
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
```

Update `crates/brain-core/src/lib.rs`:
```rust
pub mod config;
pub mod embedding;
pub mod model;
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p brain-core`
Expected: PASS (6 tests passing).

- [x] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [x] **Step 6: Commit**

```bash
git add crates/brain-core/src/
git commit -m "feat(embedding): add EmbeddingProvider trait and per-provider binary cache"
```

---

### Task 4: Modular `index/` Part 1 — Chunker, `petgraph` Builder, BM25 & Vector Scanner

**Files:**
- Create: `crates/brain-core/src/index/chunker.rs`
- Create: `crates/brain-core/src/index/graph.rs`
- Create: `crates/brain-core/src/index/bm25.rs`
- Create: `crates/brain-core/src/index/vector.rs`
- Create: `crates/brain-core/src/index/mod.rs`
- Modify: `crates/brain-core/src/lib.rs`

- [x] **Step 1: Write the failing test**

In `crates/brain-core/src/index/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeDocument;
    use std::collections::HashMap;

    #[test]
    fn test_chunker_bm25_and_graph_edges() {
        let doc1 = NodeDocument::parse(
            "goals/g1.md",
            "---\nid: goal:g1\ntype: goal\ntitle: Goal 1\ntags: [robotics]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\n# Intro\nAlpha\n## Details\nBeta biquad\n",
        );
        let doc2 = NodeDocument::parse(
            "tasks/t1.md",
            "---\nid: task:t1\ntype: task\ntitle: Task 1\nstatus: open\ntags: [robotics]\nedges:\n  implements: [goal:g1]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\nLinks to [[goal:g1]].\n",
        );
        let chunks = chunker::split_chunks(&doc1, &HashMap::new());
        assert_eq!(chunks.len(), 2);

        let mut g = graph::GraphStore::default();
        g.rebuild(&[doc1.clone(), doc2.clone()]);
        let neighbors = g.expand_1hop(&["goal:g1".to_string()]);
        assert!(neighbors.contains_key("task:t1"));

        let mut bm = bm25::Bm25Index::default();
        bm.rebuild(&[doc1], &chunks);
        let scores = bm.score_query("biquad");
        assert!(scores.get("goal:g1").copied().unwrap_or(0.0) > 0.0);
    }
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p brain-core`
Expected: FAIL because `index/` submodules do not exist yet.

- [x] **Step 3: Write minimal implementation**

Write `crates/brain-core/src/index/chunker.rs`:
```rust
use crate::embedding::deterministic_embed;
use crate::model::NodeDocument;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        .unwrap_or_else(|| deterministic_embed(&full_text, 64));
    ChunkRecord {
        node_id: node_id.to_string(),
        chunk_idx,
        heading: heading.to_string(),
        text: text.to_string(),
        blake3_hash: hash,
        embedding,
    }
}
```

Write `crates/brain-core/src/index/graph.rs`:
```rust
use crate::model::NodeDocument;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Implements,
    DependsOn,
    AffiliatedWith,
    References,
}

#[derive(Default, Clone)]
pub struct GraphStore {
    pub graph: StableDiGraph<NodeDocument, EdgeKind>,
    pub node_indices: HashMap<String, NodeIndex>,
}

impl GraphStore {
    pub fn rebuild(&mut self, docs: &[NodeDocument]) {
        self.graph.clear();
        self.node_indices.clear();
        let mut slug_to_id = HashMap::new();

        for doc in docs {
            let id = doc.meta.id.clone();
            if let Some((_, slug)) = id.split_once(':') {
                slug_to_id.insert(slug.to_string(), id.clone());
            }
            let idx = self.graph.add_node(doc.clone());
            self.node_indices.insert(id, idx);
        }

        for doc in docs {
            let Some(&src_idx) = self.node_indices.get(&doc.meta.id) else {
                continue;
            };
            for target in &doc.meta.edges.implements {
                if let Some(&dst_idx) = self.node_indices.get(target) {
                    self.graph.add_edge(src_idx, dst_idx, EdgeKind::Implements);
                }
            }
            for target in &doc.meta.edges.depends_on {
                if let Some(&dst_idx) = self.node_indices.get(target) {
                    self.graph.add_edge(src_idx, dst_idx, EdgeKind::DependsOn);
                }
            }
            for target in &doc.meta.edges.affiliated_with {
                if let Some(&dst_idx) = self.node_indices.get(target) {
                    self.graph.add_edge(src_idx, dst_idx, EdgeKind::AffiliatedWith);
                }
            }
            for raw_link in doc.extract_wikilinks() {
                let resolved = if self.node_indices.contains_key(&raw_link) {
                    Some(raw_link)
                } else {
                    slug_to_id.get(&raw_link).cloned()
                };
                if let Some(target_id) = resolved {
                    if let Some(&dst_idx) = self.node_indices.get(&target_id) {
                        self.graph.add_edge(src_idx, dst_idx, EdgeKind::References);
                    }
                }
            }
        }
    }

    pub fn expand_1hop(&self, seed_ids: &[String]) -> HashMap<String, f32> {
        let mut scores = HashMap::new();
        for seed_id in seed_ids {
            if let Some(&idx) = self.node_indices.get(seed_id) {
                for edge in self
                    .graph
                    .edges_directed(idx, Direction::Outgoing)
                    .chain(self.graph.edges_directed(idx, Direction::Incoming))
                {
                    let neighbor_idx = if edge.source() == idx {
                        edge.target()
                    } else {
                        edge.source()
                    };
                    if let Some(doc) = self.graph.node_weight(neighbor_idx) {
                        *scores.entry(doc.meta.id.clone()).or_insert(0.0) += 1.0;
                    }
                }
            }
        }
        scores
    }
}
```

Write `crates/brain-core/src/index/bm25.rs`:
```rust
use super::chunker::ChunkRecord;
use crate::model::NodeDocument;
use std::collections::HashMap;

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
                let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                for (node_id, tf) in postings {
                    let tf_f = *tf as f32;
                    let s = idf * ((tf_f * 2.2) / (tf_f + 1.2));
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
```

Write `crates/brain-core/src/index/vector.rs`:
```rust
use super::chunker::ChunkRecord;
use std::collections::HashMap;

pub fn score_chunks_cosine(
    query_vec: &[f32],
    chunks: &[ChunkRecord],
) -> (HashMap<String, f32>, HashMap<String, String>) {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut snippets: HashMap<String, String> = HashMap::new();

    for chunk in chunks {
        let cos = cosine_similarity(query_vec, &chunk.embedding);
        let entry = scores.entry(chunk.node_id.clone()).or_insert(-1.0);
        if cos > *entry {
            *entry = cos;
            snippets.insert(
                chunk.node_id.clone(),
                format!("{}: {}", chunk.heading, chunk.text.lines().next().unwrap_or("")),
            );
        }
    }
    (scores, snippets)
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..len {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    dot / (na.sqrt() * nb.sqrt()).max(1e-8)
}
```

Write initial `crates/brain-core/src/index/mod.rs` exporting `bm25`, `chunker`, `graph`, and `vector`, and add `pub mod index;` to `crates/brain-core/src/lib.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p brain-core`
Expected: PASS (7 tests passing).

- [x] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [x] **Step 6: Commit**

```bash
git add crates/brain-core/src/
git commit -m "feat(index): add modular chunker, petgraph store, BM25, and cosine scanner"
```

---

### Task 5: Modular `index/` Part 2 — RRF Merger, Timeline Projection & `BrainIndex` Coordinator

**Files:**
- Create: `crates/brain-core/src/index/rrf.rs`
- Create: `crates/brain-core/src/index/timeline.rs`
- Modify: `crates/brain-core/src/index/mod.rs`

- [x] **Step 1: Write the failing test**

Add `test_hybrid_rrf_search_and_timeline_projection` in `crates/brain-core/src/index/mod.rs` testing `BrainIndex::search_rrf` and `BrainIndex::build_timeline` with external bug P-level and CL status hydration.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p brain-core`
Expected: FAIL because `rrf.rs`, `timeline.rs`, and `BrainIndex` are not implemented yet.

- [x] **Step 3: Write minimal implementation**

Write `crates/brain-core/src/index/rrf.rs`:
```rust
use crate::model::{NodeDocument, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub node_id: String,
    pub title: String,
    pub node_type: NodeType,
    pub rrf_score: f32,
    pub max_cosine: f32,
    pub snippet: String,
}

pub fn fuse_rrf(
    bm25_scores: &HashMap<String, f32>,
    vec_scores: &HashMap<String, f32>,
    graph_scores: &HashMap<String, f32>,
) -> HashMap<String, f32> {
    let bm25_ranked = rank_descending(bm25_scores);
    let vec_ranked = rank_descending(vec_scores);
    let graph_ranked = rank_descending(graph_scores);

    let mut totals: HashMap<String, f32> = HashMap::new();
    for (rank, id) in bm25_ranked.iter().enumerate() {
        if bm25_scores.get(id).copied().unwrap_or(0.0) > 0.0 {
            *totals.entry(id.clone()).or_insert(0.0) += 1.0 / (60.0 + (rank + 1) as f32);
        }
    }
    for (rank, id) in vec_ranked.iter().enumerate() {
        if vec_scores.get(id).copied().unwrap_or(0.0) > 0.0 {
            *totals.entry(id.clone()).or_insert(0.0) += 1.0 / (60.0 + (rank + 1) as f32);
        }
    }
    for (rank, id) in graph_ranked.iter().enumerate() {
        if graph_scores.get(id).copied().unwrap_or(0.0) > 0.0 {
            *totals.entry(id.clone()).or_insert(0.0) += 0.5 / (60.0 + (rank + 1) as f32);
        }
    }
    totals
}

pub fn rank_descending(scores: &HashMap<String, f32>) -> Vec<String> {
    let mut pairs: Vec<(&String, &f32)> = scores.iter().collect();
    pairs.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
    pairs.into_iter().map(|(k, _)| k.clone()).collect()
}
```

Write `crates/brain-core/src/index/timeline.rs`:
```rust
use super::graph::GraphStore;
use crate::model::{NodeDocument, NodeType, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExternalBug {
    pub id: String,
    pub title: String,
    pub priority: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExternalChange {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExternalCache {
    pub bugs: BTreeMap<String, ExternalBug>,
    pub changes: BTreeMap<String, ExternalChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydratedTask {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub due: Option<String>,
    pub bug: Option<String>,
    pub change: Option<String>,
    pub inherited_priority: Option<String>,
    pub bug_status: Option<String>,
    pub change_status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimelineProjection {
    pub tasks: Vec<HydratedTask>,
    pub unlinked_bugs: Vec<ExternalBug>,
    pub unlinked_changes: Vec<ExternalChange>,
    pub journals: Vec<NodeDocument>,
}

pub fn project_timeline(store: &GraphStore, external: &ExternalCache) -> TimelineProjection {
    let mut tasks = Vec::new();
    let mut journals = Vec::new();
    let mut linked_bugs = HashSet::new();
    let mut linked_changes = HashSet::new();

    for idx in store.graph.node_indices() {
        let doc = &store.graph[idx];
        match doc.meta.node_type {
            NodeType::Task => {
                let status = doc.meta.status.unwrap_or(TaskStatus::Open);
                if status == TaskStatus::Done {
                    continue;
                }
                let mut inherited_priority = None;
                let mut bug_status = None;
                if let Some(bug_id) = &doc.meta.bug {
                    linked_bugs.insert(bug_id.clone());
                    if let Some(b) = external.bugs.get(bug_id) {
                        inherited_priority = Some(b.priority.clone());
                        bug_status = Some(b.status.clone());
                    }
                }
                let mut change_status = None;
                if let Some(cl_id) = &doc.meta.change {
                    linked_changes.insert(cl_id.clone());
                    if let Some(c) = external.changes.get(cl_id) {
                        change_status = Some(c.status.clone());
                    }
                }
                tasks.push(HydratedTask {
                    id: doc.meta.id.clone(),
                    title: doc.meta.title.clone(),
                    status,
                    due: doc.meta.due.clone(),
                    bug: doc.meta.bug.clone(),
                    change: doc.meta.change.clone(),
                    inherited_priority,
                    bug_status,
                    change_status,
                });
            }
            NodeType::Journal => journals.push(doc.clone()),
            _ => {}
        }
    }

    tasks.sort_by(|a, b| {
        let pa = a.inherited_priority.as_deref().unwrap_or("P9");
        let pb = b.inherited_priority.as_deref().unwrap_or("P9");
        pa.cmp(pb).then_with(|| a.due.cmp(&b.due))
    });

    let unlinked_bugs = external
        .bugs
        .values()
        .filter(|b| !linked_bugs.contains(&b.id))
        .cloned()
        .collect();
    let unlinked_changes = external
        .changes
        .values()
        .filter(|c| !linked_changes.contains(&c.id))
        .cloned()
        .collect();

    TimelineProjection {
        tasks,
        unlinked_bugs,
        unlinked_changes,
        journals,
    }
}
```

Update `crates/brain-core/src/index/mod.rs` to wire `BrainIndex` across `chunker`, `graph`, `bm25`, `vector`, `rrf`, and `timeline`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p brain-core`
Expected: PASS (8 tests passing).

- [x] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [x] **Step 6: Commit**

```bash
git add crates/brain-core/src/index/
git commit -m "feat(index): add RRF fusion, timeline external hydration, and BrainIndex coordinator"
```

---

### Task 6: Modular `tx/` Hierarchy (Vault Validator, Git Runner, Session Guard, Worktree Lifecycle, Audit/Revert)

**Files:**
- Create: `crates/brain-core/src/tx/mod.rs`
- Create: `crates/brain-core/src/tx/vault.rs`
- Create: `crates/brain-core/src/tx/git.rs`
- Create: `crates/brain-core/src/tx/session.rs`
- Create: `crates/brain-core/src/tx/lifecycle.rs`
- Create: `crates/brain-core/src/tx/audit.rs`
- Modify: `crates/brain-core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add `test_vault_init_empty_reopen_and_abort_on_non_empty_non_vault` and `test_worktree_atomic_commit_session_guard_and_revert` in `crates/brain-core/src/tx/mod.rs` testing:
1. Initializing a missing or empty directory creates `.second-brain.json`, the 5 node directories, and `main` + `manual` branches.
2. Re-opening an initialized vault succeeds.
3. Passing a non-empty directory that lacks `.second-brain.json` aborts with `TxError::InvalidVaultDirectory` without modifying existing files.
4. `manual` root checkout, `.brain/tx/<tx_id>` creation off `main`, `TxError::SessionMismatch`, atomic commit onto `main` + sync to `manual`, stray-edit preservation, and `revert_commit`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p brain-core`
Expected: FAIL because `tx/` submodules do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Split worktree transaction logic cleanly across:
- `crates/brain-core/src/tx/vault.rs` — `ensure_or_init_vault(vault_root)`: checks if `vault_root` does not exist or is empty (initializes `.second-brain.json`, `.gitignore`, 5 folders, `main` + `manual` branches), or if `.second-brain.json` + `.git` exist (validates and opens), or else returns `Err(TxError::InvalidVaultDirectory)`.
- `crates/brain-core/src/tx/git.rs` — `run_git(cwd, args)`, `branch_of(dir)`, `sync_main_into_manual(vault_root)`.
- `crates/brain-core/src/tx/session.rs` — `TxHandle`, `TxError` (including `InvalidVaultDirectory`), and `SessionRegistry::verify_owner(tx_id, caller_session_id)`.
- `crates/brain-core/src/tx/lifecycle.rs` — `begin_tx`, `stage_upsert`, `stage_delete`, `commit_tx`, `abort_tx`.
- `crates/brain-core/src/tx/audit.rs` — `AuditCommit`, `daily_stray_checkpoint`, `list_audit_commits`, `revert_commit`, `load_branch_documents`.
- `crates/brain-core/src/tx/mod.rs` — `WorktreeCoordinator` facade delegating to `vault`, `git`, `session`, `lifecycle`, and `audit`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p brain-core`
Expected: PASS (9 tests passing).

- [ ] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [ ] **Step 6: Commit**

```bash
git add crates/brain-core/src/
git commit -m "feat(tx): add modular git runner, session guard, worktree lifecycle, and audit revert"
```

---

### Task 7: Modular `workers/` Hierarchy (Harness, Observer, Sync, Synthesizer & Supervisor)

**Files:**
- Create: `crates/brain-core/src/workers/mod.rs`
- Create: `crates/brain-core/src/workers/harness/mod.rs`
- Create: `crates/brain-core/src/workers/harness/jetski.rs`
- Create: `crates/brain-core/src/workers/observer/mod.rs`
- Create: `crates/brain-core/src/workers/observer/jetski.rs`
- Create: `crates/brain-core/src/workers/sync/mod.rs`
- Create: `crates/brain-core/src/workers/sync/buganizer.rs`
- Create: `crates/brain-core/src/workers/sync/critique.rs`
- Create: `crates/brain-core/src/workers/synthesizer.rs`
- Modify: `crates/brain-core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add `test_observer_pushes_high_confidence_memory_once_per_session` in `crates/brain-core/src/workers/observer/mod.rs` and `test_supervisor_registers_per_harness_and_per_source_workers` in `crates/brain-core/src/workers/mod.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p brain-core`
Expected: FAIL because `workers/` submodules do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement each focused worker submodule:
- `workers/harness/mod.rs` & `workers/harness/jetski.rs` (`HeadlessHarness` trait & `JetskiHeadlessHarness`)
- `workers/observer/mod.rs` & `workers/observer/jetski.rs` (`ObserverHarnessAdapter` trait, `ObserverState`, `evaluate_and_push_turns`, `JetskiObserverAdapter`)
- `workers/sync/mod.rs`, `workers/sync/buganizer.rs`, `workers/sync/critique.rs` (`ExternalSyncSource` trait, `BuganizerSyncSource`, `CritiqueSyncSource`)
- `workers/synthesizer.rs` (`run_daily_checkpoint_and_synthesize`)
- `workers/mod.rs` (`WorkerSupervisor`, `WorkerRunState`, `WorkerInfo`)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p brain-core`
Expected: PASS (11 tests passing).

- [ ] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [ ] **Step 6: Commit**

```bash
git add crates/brain-core/src/
git commit -m "feat(workers): add modular harness, per-harness observer, per-source sync, and supervisor"
```

---

### Task 8: Modular `braind` (`state.rs`, `api/*`), `brainctl` (`mcp.rs`), and `/remember` + `/recall` Skills

**Files:**
- Modify: `crates/braind/Cargo.toml`
- Create: `crates/braind/src/state.rs`
- Create: `crates/braind/src/api/mod.rs`
- Create: `crates/braind/src/api/tx.rs`
- Create: `crates/braind/src/api/search.rs`
- Create: `crates/braind/src/api/nodes.rs`
- Create: `crates/braind/src/api/audit.rs`
- Create: `crates/braind/src/api/workers.rs`
- Modify: `crates/braind/src/main.rs`
- Modify: `crates/brainctl/Cargo.toml`
- Create: `crates/brainctl/src/mcp.rs`
- Modify: `crates/brainctl/src/main.rs`
- Create: `skills/remember/SKILL.md`
- Create: `skills/recall/SKILL.md`

- [ ] **Step 1: Write the failing test**

Add `test_api_tx_session_enforcement_branch_selection_and_get_search` in `crates/braind/src/api/mod.rs` and `test_agent_mcp_tools_exclude_ui_only_endpoints` in `crates/brainctl/src/mcp.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --workspace`
Expected: FAIL because `crates/braind/src/state.rs`, `crates/braind/src/api/*`, and `crates/brainctl/src/mcp.rs` do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement each modular handler file in `crates/braind/src/`:
- `state.rs`: `AppState` managing `coord`, `main_index`, `manual_index`, `external_cache`, `supervisor`, and `events_tx`.
- `api/tx.rs`: `begin_tx`, `tx_upsert_node`, `tx_delete_node`, `commit_tx`, `abort_tx` enforcing `X-Brain-Session-Id`.
- `api/search.rs`: `GET /api/v1/search` and `GET /api/v1/search/deep` with `?q=&branch=main|manual&limit=`.
- `api/nodes.rs`: `GET /api/v1/nodes`, `GET /api/v1/nodes/:id`, UI-only `PUT / DELETE /api/v1/nodes/:id`, `GET /api/v1/graph`, and `GET /api/v1/timeline`.
- `api/audit.rs`: UI-only `GET /api/v1/audit` and `POST /api/v1/audit/:sha/revert`.
- `api/workers.rs`: UI-only `GET / POST /api/v1/workers` and `GET /api/v1/events` SSE stream.
- `api/mod.rs`: `build_router` and `GET /api/v1/openapi.json`.
- `crates/brainctl/src/mcp.rs`: `AGENT_MCP_TOOLS` whitelist (`brain_begin_tx`, `brain_tx_upsert`, `brain_tx_delete`, `brain_commit_tx`, `brain_abort_tx`, `brain_search`, `brain_deep_search`, `brain_get_node`, `brain_timeline`) and JSON-RPC tool dispatcher.
- `skills/remember/SKILL.md` & `skills/recall/SKILL.md`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --workspace`
Expected: PASS (all unit and HTTP/MCP integration tests passing across `brain-core`, `braind`, and `brainctl`).

- [ ] **Step 5: Strictly follow the implementer's subagents**

Ensure to strictly follow the implementer's subagents workflow within the SDLC verification loop to handle any code reviews (regressions, sanitation, vulnerability) and ensure quality before committing.

- [ ] **Step 6: Commit**

```bash
git add crates/ skills/
git commit -m "feat(braind): add modular API routes, restricted MCP bridge, and remember/recall skills"
```
