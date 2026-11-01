# Retrieval, Providers & Workers Specification: Second Brain v2

## 1. Swappable Embedding Provider (`EmbeddingProvider`)

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
    fn dimensions(&self) -> usize;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}
```
- **Per-Provider/Model Cache Files**: Vectors are persisted in `.brain/embeddings/<provider_id>_<model_id>.bin` keyed by `(node_id, chunk_idx, blake3_hash)`.
- Switching `[embedding]` in `config.toml` loads or creates the corresponding `.bin` file without invalidating or deleting other providers' caches, preventing full-vault re-embedding when toggling models.
- Initial implementation: `GeminiEmbeddingProvider`.

## 2. Two-Tier Retrieval Engine

### 2.1 Tier-1 Heuristic Search (<5 ms In-Memory)
1. **Heading Chunking**: Splits Markdown bodies on `#` / `##` boundaries, computing `blake3` hashes per chunk.
2. **BM25 Lexical Index**: In-memory inverted index over `id`, `title`, `tags`, and chunk text.
3. **SIMD Vector Index**: Flat `f32` cosine similarity scan across active chunk vectors from `.brain/embeddings/<provider>_<model>.bin`.
4. **Graph Expansion**: 1–2 hop walk across `implements`, `depends_on`, `affiliated_with`, and `references` edges in `petgraph`.
5. **Reciprocal Rank Fusion (RRF)**:
   $$\text{score}(n) = \sum_{m \in \{\text{bm25}, \text{vec}, \text{graph}\}} \frac{w_m}{60 + \text{rank}_m(n)}$$

### 2.2 Tier-2 Agentic Deep Search (`HeadlessHarness` & `/recall`)
```rust
#[async_trait]
pub trait HeadlessHarness: Send + Sync {
    fn harness_id(&self) -> &str;
    async fn run_prompt(&self, prompt: &str, timeout: Duration) -> Result<String>;
}
```
- Configured via `[harness]` in `config.toml` (`default = "jetski"`). Initial implementation: `JetskiHeadlessHarness` (`jetski -p`), extensible to `pi`, `claude`, `opencode`.
- Triggered by explicit `/recall [query]` or `GET /api/v1/search/deep?q=<query>&branch=<branch>`.
- Seeds the `HeadlessHarness` with top Tier-1 RRF results and 1-hop graph neighbors; the headless agent reads full markdown notes across multi-hop links and returns a synthesized response with `[[node_id]]` citations.

## 3. Pluggable Supervised Workers (`WorkerSupervisor`)

### 3.1 Per-Harness Transcript Observers (`observer:<harness>`)
```rust
#[async_trait]
pub trait ObserverHarnessAdapter: Send + Sync {
    fn harness_id(&self) -> &str;
    fn watch_paths(&self) -> &[PathBuf];
    fn parse_new_turns(&self, raw_bytes: &[u8]) -> Result<Vec<ObservedTurn>>;
    async fn push_memory(&self, session_id: &str, message: &str) -> Result<()>;
}
```
- One supervised worker per harness (`observer:jetski` implemented initially; extensible to `observer:pi`, etc.), independently togglable in `config.toml` (`[observers.<harness>] enabled = true|false`).
- Watches active transcript files via `notify`, runs `<5ms` Tier-1 Heuristic Search on new `USER_INPUT` turns, and pushes high-confidence matches (deduplicated per session in `.brain/observer_state.json`) via `push_memory`.
- Strictly read-only: never writes memories passively.

### 3.2 Per-Source External Sync Workers (`sync:<source>`)
```rust
#[async_trait]
pub trait ExternalSyncSource: Send + Sync {
    fn source_id(&self) -> &str;
    async fn poll(&self) -> Result<Vec<ExternalItem>>;
}
```
- One supervised worker per source (`sync:buganizer` and `sync:critique` implemented initially; extensible to `sync:gerrit`, etc.), independently togglable in `config.toml` (`[sync.<source>] enabled = true|false`).
- Writes polled items into `.brain/external_cache.json` under `sources.<source_id>`.

### 3.3 Daily Synthesizer & Stray Checkpoint (`synthesizer`)
- Runs once daily:
  1. Commits any uncommitted stray edits on `manual` (`chore(vault): daily stray checkpoint`) and merges `manual` $\to$ `main`.
  2. Opens an ephemeral worktree `.brain/tx/<tx_id>` off `main` and runs `HeadlessHarness` to deduplicate `knowledge/` notes and normalize links as 1 atomic commit.
