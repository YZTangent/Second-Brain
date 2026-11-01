# Second Brain v2 Architecture — Requirements

## 1. Scope & Objectives

Clean-slate redesign of Second Brain (`second-brain-v2`) replacing v1's `inbox -> proposed` manual review pipeline, frontmatter-only CLI search, and cron sidecars with:
1. **Headless Rust Daemon (`braind`)**: Single process managing canonical Markdown files on disk, an in-memory property graph (`petgraph`), in-memory lexical + vector hybrid search, Git worktree transactions, and pluggable supervised background workers.
2. **Worktree-Backed Atomic Save (`manual` vs `main`)**: Every explicit `/remember` call executes inside an isolated ephemeral Git worktree and merges into `main` as **one atomic commit**, synced to the local disk `manual` branch while serving `main` by default to Web UIs (with branch selection support).
3. **Dual-Path Recollection**:
   - **Passive**: Per-harness background heuristic pushers (`observer:<harness>`) watch active agent transcripts via `inotify`, run `<5ms` in-memory BM25 + SIMD cosine matching, and push high-confidence context into the active session.
   - **Active**: Explicit `/recall` triggers Tier-2 Agentic Deep Search via a pluggable headless harness runner (`HeadlessHarness`, implemented for `jetski -p` initially) for multi-hop synthesis.
4. **Decoupled Contract Boundary**: OpenAPI 3.1 REST + WebSocket/SSE API for human UIs (Web UI, Jetski Aux Pane, CLI) strictly separated from a minimal, session-scoped MCP surface for agents.

---

## 2. Storage, Configuration & Git Branch Topology

### 2.1 Directory Layout
```
second-brain-v2/                # Working directory checked out on branch `manual`
├── config.toml                 # Daemon config: providers, harness adapters, observer & sync toggles
├── knowledge/                  # Durable concepts, insights, patterns, snippets (<slug>.md)
├── goals/                      # Long-term milestones and directions (<slug>.md)
├── tasks/                      # Actionable items (<slug>.md)
├── journal/                    # Daily episodic logs (YYYY-MM-DD.md)
├── relationships/              # People and organizations interacted with (<slug>.md)
├── .brain/                     # Git-ignored daemon runtime state & caches
│   ├── embeddings/             # Per-provider/model embedding caches (<provider>_<model>.bin)
│   ├── external_cache.json     # Polled read-only external items namespaced by source (buganizer, critique, ...)
│   ├── observer_state.json     # Per-harness transcript byte offsets & per-session pushed-node dedup set
│   └── tx/                     # Ephemeral Git worktrees (.brain/tx/<tx_id>)
├── crates/
│   ├── braind/                 # Headless Rust daemon (axum, tokio, petgraph, notify)
│   └── brainctl/               # Thin CLI & MCP stdio bridge
├── skills/
│   ├── remember/               # Explicit atomic capture skill
│   └── recall/                 # Explicit long-form deep search skill
└── docs/
    ├── spec/
    ├── adr/
    └── changes/
```

### 2.2 Branch & Worktree Lifecycle (`manual` vs `main`)
1. **Disk Working Branch (`manual`) vs Default Served Branch (`main`)**:
   - The repository root working directory on disk remains checked out on branch `manual` so manual local file edits never block `main` updates.
   - **Web UIs and read APIs serve `main` by default**, while supporting an optional `?branch=main|manual` query parameter so users can switch between viewing `main` and `manual` in the UI.
2. **Canonical Ledger Branch (`main`)**: Not checked out by any persistent worktree, allowing `braind` to advance `main` from ephemeral transaction worktrees without Git checked-out branch conflicts.
3. **Ephemeral Transaction Worktrees (`.brain/tx/<tx_id>`)**:
   - `POST /api/v1/tx` creates a detached worktree at `.brain/tx/<tx_id>` rooted at `main` and binds `owner_session_id`.
   - Mutations (`PUT / DELETE /api/v1/tx/:tx_id/nodes`) write files exclusively inside `.brain/tx/<tx_id>` after verifying `caller_session_id == owner_session_id` (best-effort session ownership; returns `403 Forbidden` on mismatch).
   - `POST /api/v1/tx/:tx_id/commit` stages all worktree changes, creates **1 atomic commit** (`remember: <summary>` or `synthesize: <summary>`) with trailers (`Brain-Actor`, `Brain-Tx`, `Brain-Session`), advances `main`, merges `main` into `manual` (auto-committing overlapping dirty files on `manual` first if needed), removes `.brain/tx/<tx_id>`, and batch-updates the in-memory index.
   - `POST /api/v1/tx/:tx_id/abort` removes `.brain/tx/<tx_id>` without modifying `main` or `manual`.
   - Stale `.brain/tx/*` worktrees older than 15 minutes are garbage-collected by `braind`.
4. **Daily Stray-Edit Checkpoint**: Once daily (prior to running the `synthesizer` worker), `braind` commits any uncommitted stray edits on `manual` (`chore(vault): daily stray checkpoint`) and merges `manual` into `main`.
5. **Commit-Granular Audit & Revert**:
   - Auditing inspects commits on `main` or `manual` (`GET /api/v1/audit?branch=main|manual`).
   - Reverting (`POST /api/v1/audit/:sha/revert`) executes `git revert --no-edit <sha>` inside a temporary worktree off `main`, advances `main`, syncs `manual`, and hot-reloads affected nodes in memory.

---

## 3. Data Model & In-Memory Graph Schema

### 3.1 Node Taxonomy (5 Durable Types)

| Type (`type`) | Path | Allowed Fields | Rules |
|---|---|---|---|
| `knowledge` | `knowledge/<slug>.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Durable technical concepts, learnings, patterns, formulas, snippets. No `status` or `priority`. |
| `goal` | `goals/<slug>.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Long-term milestones and directions. No `status` or `priority`. |
| `task` | `tasks/<slug>.md` | `id`, `type`, `title`, `status`, `due?`, `bug?`, `change?`, `tags`, `edges`, `updated_at` | **Only** node type with `status` (`open \| in_progress \| blocked \| done`). Optional `due` (`YYYY-MM-DD`), `bug` (`b/<id>`), and `change` (Critique `cl/<id>` or Gerrit change ID). No `priority` field; inherits live P-level from tracked `bug` in `.brain/external_cache.json`. |
| `journal` | `journal/YYYY-MM-DD.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Daily episodic log and reflections. No `status` or `priority`. |
| `relationship` | `relationships/<slug>.md` | `id`, `type`, `title`, `kind?`, `tags`, `edges`, `updated_at` | Tracks people and organizations interacted with (`kind`: `person \| org`). Links to related `knowledge`, `goal`, `task`, `journal`, or parent `relationship` nodes. |

### 3.2 Frontmatter & Edge Schema
```yaml
---
id: task:port-bimanual-rl-controller
type: task
title: Port humanoid RL stack to bimanual manipulation
status: in_progress
due: 2026-11-15
bug: b/345678901
change: cl/678901234
tags: [robotics, rl, manipulation]
edges:
  implements: [goal:2026-q4-bimanual-demo]
  depends_on: [knowledge:actuator-pd-gain-schedule]
updated_at: 2026-11-01T14:15:00Z
---
```
- **In-Memory Graph (`petgraph::stable_graph::StableDiGraph`)**:
  - Maintains indexed branch projections (`main` primary, `manual` overlay/secondary) for sub-millisecond reads.
  - Directed typed edges: `implements` (`task` $\to$ `goal`), `depends_on` (`task` $\to$ `task` | `knowledge`), `affiliated_with` (`relationship` $\to$ `relationship`), and `references` (extracted from inline `[[WikiLinks]]` resolving to existing vault nodes).
  - External references (`bug: b/123` or `change: cl/456`) do **not** create stub nodes in `petgraph`. Instead, `task` reads join against `.brain/external_cache.json` at query time.

---

## 4. Retrieval, Swappable Embeddings & Pluggable Harnesses

### 4.1 Swappable Embedding Provider (`EmbeddingProvider` Trait)
```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
    fn dimensions(&self) -> usize;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}
```
- Configured via `[embedding]` in `config.toml` (`provider = "gemini"`, `model = "..."`).
- Embeddings are persisted in per-provider/model cache files under `.brain/embeddings/<provider_id>_<model_id>.bin`, keyed by `(node_id, chunk_idx, blake3_hash)`. Switching providers or models in `config.toml` creates or loads that provider's dedicated cache file while preserving existing caches on disk, avoiding redundant re-embedding across the vault.
- Initial implementation: `GeminiEmbeddingProvider` (extensible to local ONNX/Ollama/OpenAI providers).

### 4.2 Tier-1 Heuristic Search (<5 ms In-Memory)
1. **Chunking**: Markdown bodies split by `#` / `##` headings into semantic chunks.
2. **BM25 Lexical Index**: In-memory inverted index over `id`, `title`, `tags`, and chunk text.
3. **SIMD Vector Index**: Flat `f32` cosine similarity scan over cached chunk embeddings.
4. **Graph Walk**: 1–2 hop expansion across `implements`, `depends_on`, `affiliated_with`, and `references` edges.
5. **Reciprocal Rank Fusion (RRF)**:
   $$\text{score}(n) = \sum_{m \in \{\text{bm25}, \text{vec}, \text{graph}\}} \frac{w_m}{60 + \text{rank}_m(n)}$$

### 4.3 Pluggable Headless Harness (`HeadlessHarness` Trait) & Tier-2 Deep Search
```rust
#[async_trait]
pub trait HeadlessHarness: Send + Sync {
    fn harness_id(&self) -> &str;
    async fn run_prompt(&self, prompt: &str, timeout: Duration) -> Result<String>;
}
```
- Configured via `[harness]` in `config.toml` (`default = "jetski"`).
- Initial implementation: `JetskiHeadlessHarness` (`jetski -p`), designed for drop-in addition of `PiHarness`, `ClaudeCodeHarness`, or `OpenCodeHarness`.
- **Tier-2 Agentic Deep Search (`/recall` & `GET /api/v1/search/deep`)**:
  - Seeds the configured `HeadlessHarness` with top Tier-1 RRF results and their 1-hop graph neighborhood.
  - The headless agent iteratively reads full markdown files across multi-hop links and returns a synthesized markdown response with `[[node_id]]` citations.

---

## 5. Pluggable Supervised Background Workers (`braind`)

All workers are managed by `WorkerSupervisor` with independent `config.toml` toggles, pause/resume/trigger API controls, and ring-buffer telemetry:

### 5.1 Per-Harness Transcript Observers (`observer:<harness>`)
```rust
#[async_trait]
pub trait ObserverHarnessAdapter: Send + Sync {
    fn harness_id(&self) -> &str;
    fn watch_paths(&self) -> &[PathBuf];
    fn parse_new_turns(&self, raw_bytes: &[u8]) -> Result<Vec<ObservedTurn>>;
    async fn push_memory(&self, session_id: &str, message: &str) -> Result<()>;
}
```
- **One observer worker per harness**, independently togglable in `config.toml` (`[observers.jetski] enabled = true`, `[observers.pi] enabled = false`, etc.).
- **Initial implementation**: `observer:jetski` (watches `~/.gemini/jetski/brain/*/transcript.jsonl` via `notify` and pushes via `agentapi send-message <conv_id>`).
- **Behavior**:
  - On new user turns, runs Tier-1 Heuristic Search (`BM25 + SIMD Cosine + 1-hop Graph`, `<5ms`, zero LLM polling cost).
  - If top nodes pass configured `cosine / RRF` thresholds and are not in `observer_state.json`'s per-session sent set, pushes a compact memory brief via `adapter.push_memory(...)`.
  - Strictly read-only: never scrapes or writes memories from transcripts.

### 5.2 Per-Source External Sync Workers (`sync:<source>`)
```rust
#[async_trait]
pub trait ExternalSyncSource: Send + Sync {
    fn source_id(&self) -> &str;
    async fn poll(&self) -> Result<Vec<ExternalItem>>;
}
```
- **One sync worker per external source**, independently togglable and configurable in `config.toml` (`[sync.buganizer]`, `[sync.critique]`, extensible to `sync:gerrit`, `sync:github`, etc.).
- **Initial implementations**:
  1. `sync:buganizer` — Polls assigned Buganizer issues (`issue` CLI) into `.brain/external_cache.json` (`sources.buganizer`).
  2. `sync:critique` — Polls active Critique CLs (`critique`/`g4` CLI) into `.brain/external_cache.json` (`sources.critique`).
- Used to hydrate `task` `bug` (live P-level/status) and `change` (CL review status) fields and populate the Daily/Weekly Timeline projection.

### 5.3 Daily Synthesizer & Stray Checkpoint (`synthesizer`)
- Runs once daily using the configured `HeadlessHarness`:
  1. Commits any uncommitted stray edits on `manual` (`chore(vault): daily stray checkpoint`) and merges `manual` $\to$ `main`.
  2. Opens a transaction worktree off `main` and invokes `HeadlessHarness` to merge duplicate `knowledge/` nodes and normalize links as a single revertible commit.

---

## 6. Decoupled API & Consumer Access Matrix

Read endpoints accept optional `?branch=main|manual` (defaulting to `main`).

| Endpoint | Method | Consumer Scope | Purpose |
|---|---|---|---|
| `/api/v1/tx` | `POST` | **Agent (MCP)** | Create `.brain/tx/<tx_id>` worktree off `main` bound to `owner_session_id`. |
| `/api/v1/tx/:tx_id/nodes` | `PUT / DELETE` | **Agent (MCP)** | Upsert/delete node in worktree (verifies `owner_session_id`). |
| `/api/v1/tx/:tx_id/commit` | `POST` | **Agent (MCP)** | Commit worktree $\to$ `main`, sync `manual`, update index (verifies `owner_session_id`). |
| `/api/v1/tx/:tx_id/abort` | `POST` | **Agent (MCP)** | Abort and remove `.brain/tx/<tx_id>` (verifies `owner_session_id`). |
| `/api/v1/search` | `GET` | **Both (UI & Agent)** | Tier-1 Heuristic Search (`?q=&types=&tags=&limit=&branch=`). |
| `/api/v1/search/deep` | `GET` | **Both (UI & Agent)** | Tier-2 Agentic Deep Search (`?q=&branch=`, spawns configured `HeadlessHarness`). |
| `/api/v1/nodes` | `GET` | **Both (UI & Agent)** | List/read nodes (`?branch=main\|manual`) + 1-hop edges + hydrated `bug`/`change` metadata. |
| `/api/v1/timeline` | `GET` | **Both (UI & Agent)** | Live Daily/Weekly view (`?branch=`): open `task` nodes (ordered by inherited `bug` P-level & `due`), synced Bugs/Changes, `journal`. |
| `/api/v1/nodes/:id` | `PUT / DELETE` | **Human / UI Only** | Direct single-node edit/delete on `manual` branch. |
| `/api/v1/graph` | `GET` | **Human / UI Only** | Full or filtered subgraph (`?branch=main\|manual`) for UI visualization. |
| `/api/v1/audit` | `GET` | **Human / UI Only** | Commit history (`?branch=main\|manual`) + diff stats + uncommitted stray edits on `manual`. |
| `/api/v1/audit/:sha/revert` | `POST` | **Human / UI Only** | Revert commit via worktree off `main`, sync `manual`, reload index. |
| `/api/v1/workers` | `GET / POST` | **Human / UI Only** | Inspect status/logs or pause/resume/trigger `observer:<harness>`, `sync:<source>`, and `synthesizer`. |
| `/api/v1/events` | `WS / SSE` | **Human / UI Only** | Real-time UI event stream (`TxCommitted`, `NodeUpdated`, `MemoryPushed`, `WorkerStateChanged`). |

### MCP Surface Rules (`/mcp` & `brainctl mcp-stdio`)
- Only **Agent** and **Both** endpoints are bridged to MCP tools (`brain_begin_tx`, `brain_tx_upsert`, `brain_tx_delete`, `brain_commit_tx`, `brain_abort_tx`, `brain_search`, `brain_deep_search`, `brain_get_node`, `brain_timeline`).
- Agent system instructions prohibit speculative `brain_search` calls on routine turns (relying on `observer:<harness>` push and explicit `/recall`).

---

## 7. Error Handling & Verification Requirements

1. **Invalid Frontmatter**: Malformed manual YAML edits mark the node `parse_error: true` in `petgraph`, index raw body text in BM25, and emit a `ParseError` event on `/api/v1/events` without crashing `braind`.
2. **Graceful External Degradation**:
   - `EmbeddingProvider` failure falls back to `BM25 + Graph Walk` and queues dirty chunks for exponential-backoff retry.
   - Expired `gcert` in `sync:buganizer` or `sync:critique` serves cached `.brain/external_cache.json` and reports `gcert_expired` in that worker's status without affecting other sync workers.
3. **Verification Suite**:
   - Unit tests for frontmatter round-trip serialization across all 5 node types (`knowledge`, `goal`, `task`, `journal`, `relationship`), `petgraph` edge resolution, BM25 + SIMD cosine RRF ranking, mock `EmbeddingProvider`/`HeadlessHarness`/`ObserverHarnessAdapter`/`ExternalSyncSource` traits, and session-ownership enforcement (`403 Forbidden` on mismatched `session_id`).
   - Integration tests for `manual` vs `main` worktree transactions, `?branch=main|manual` read isolation, stray-edit preservation, and `git revert` via `/api/v1/audit/:sha/revert`.
   - OpenAPI 3.1 contract snapshot test.
