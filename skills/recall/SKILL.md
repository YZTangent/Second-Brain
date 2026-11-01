---
name: recall
description: Perform Tier-2 Agentic Deep Search and multi-hop graph synthesis across Second Brain v2. Use when the user invokes /recall or asks a complex historical, architectural, or cross-cutting question requiring deep memory retrieval.
---

# `/recall` — Agentic Deep Search & Multi-Hop Synthesis (Second Brain v2)

Second Brain v2 separates memory retrieval into two paths:
- **Passive Push (`observer:<harness>`)**: Background daemon worker watches active transcripts and automatically pushes high-confidence Tier-1 matches (`<5ms` BM25 + SIMD cosine + 1-hop graph). Do **not** run speculative `brain_search` calls on routine turns.
- **Explicit Deep Recall (`/recall`)**: Invokes Tier-2 Agentic Deep Search across multi-hop graph links and full Markdown notes.

## 1. Deep Recall Workflow

1. **Seed via `brain_deep_search`**:
   Call `brain_deep_search` (`GET /api/v1/search/deep?q=<query>&branch=main`) with the user's question or topic.
   - `braind` runs Tier-1 Reciprocal Rank Fusion (BM25 + vector cosine + 1-hop `petgraph` expansion) to gather seed nodes and invokes the configured `HeadlessHarness` (`jetski -p` by default) to synthesize multi-hop context.
2. **Traverse 1-Hop & Multi-Hop Neighbors (`brain_get_node`)**:
   - If specific cited nodes (`[[knowledge:<slug>]]`, `[[goal:<slug>]]`, `[[task:<slug>]]`, `[[journal:YYYY-MM-DD]]`, `[[relationship:<slug>]]`) or their `neighbors_1hop` require deeper inspection, call `brain_get_node` to inspect full Markdown bodies and hydrated Buganizer (`external_bug`) / Critique (`external_change`) metadata.
3. **Timeline Context (`brain_timeline`)**:
   - When the query concerns active work, priorities, bugs, CLs, or recent progress, call `brain_timeline` (`GET /api/v1/timeline?branch=main`) to retrieve open `task` nodes (ordered by inherited Buganizer P-level and `due` date), unlinked bugs/CLs, and recent `journal` entries.
4. **Cite Canonical Node IDs**:
   - Present a concise, synthesized response grounded in the vault and cite every referenced note using `[[<type>:<slug>]]`.
