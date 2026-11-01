---
name: remember
description: Save durable knowledge, goals, tasks, journal entries, or relationships into Second Brain v2 as a single atomic Git worktree transaction. Use when the user invokes /remember or asks to record durable learnings, milestones, todos, or contacts.
---

# `/remember` — Atomic Worktree Memory Capture (Second Brain v2)

Every `/remember` invocation executes inside an isolated, session-scoped ephemeral Git worktree (`.brain/tx/<tx_id>`) rooted at `main` and commits all staged nodes as **one atomic commit** (`remember: <summary>`) before syncing `manual` and updating the in-memory `petgraph` + hybrid index.

## 1. Atomic Transaction Workflow

Never write directly to vault files or leave partial edits. Always execute the 3-step MCP transaction protocol:

1. **Begin Transaction**:
   Call `brain_begin_tx` (`POST /api/v1/tx` with `X-Brain-Session-Id`) to create `.brain/tx/<tx_id>` off `main` bound to your active session ID.
2. **Stage Node Upserts / Deletions**:
   - Inspect existing related nodes first via `brain_search` or `brain_get_node` so you update existing slugs rather than creating duplicates.
   - Stage one or more nodes via `brain_tx_upsert` (`PUT /api/v1/tx/:tx_id/nodes`) or `brain_tx_delete` (`DELETE /api/v1/tx/:tx_id/nodes`).
3. **Commit (or Abort on Error)**:
   - Call `brain_commit_tx` (`POST /api/v1/tx/:tx_id/commit`) with `"summary": "remember: <concise summary>"` to produce a single revertible commit on `main` and sync `manual`.
   - If any validation or staging step fails, call `brain_abort_tx` (`POST /api/v1/tx/:tx_id/abort`) to clean up `.brain/tx/<tx_id>`.

## 2. Node Taxonomy (5 Durable Types)

| Type (`type`) | Vault Path | Allowed Frontmatter Fields | Rules |
|---|---|---|---|
| `knowledge` | `knowledge/<slug>.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Durable concepts, architecture insights, patterns, formulas, snippets. **No** `status` or `priority`. |
| `goal` | `goals/<slug>.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Long-term milestones and directions. **No** `status` or `priority`. |
| `task` | `tasks/<slug>.md` | `id`, `type`, `title`, `status`, `due?`, `bug?`, `change?`, `tags`, `edges`, `updated_at` | **Only** node type with `status` (`open \| in_progress \| blocked \| done`). Optional `due` (`YYYY-MM-DD`), `bug` (`b/<id>`), `change` (`cl/<id>`). **No** static `priority` (inherits live P-level from `bug`). |
| `journal` | `journal/YYYY-MM-DD.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Daily episodic log and reflections. **No** `status` or `priority`. |
| `relationship` | `relationships/<slug>.md` | `id`, `type`, `title`, `kind?`, `tags`, `edges`, `updated_at` | People and organizations interacted with (`kind`: `person \| org`). **No** `status` or `priority`. |

## 3. Frontmatter & Graph Linking Rules

- **Typed Edges (`edges`)**:
  - `implements`: `task` $\to$ `goal` (`[goal:<slug>]`)
  - `depends_on`: `task` $\to$ `task` or `knowledge` (`[knowledge:<slug>]`)
  - `affiliated_with`: `relationship` $\to$ `relationship` (`[relationship:<slug>]`)
- **Inline `[[WikiLinks]]`**:
  - Reference related nodes in the Markdown body using `[[<type>:<slug>]]` (e.g., `[[knowledge:actuator-pd-gain-schedule]]` or `[[relationship:alice-smith]]`), which `braind` indexes as directed `references` edges in `petgraph`.
