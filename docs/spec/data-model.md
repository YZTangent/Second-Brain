# Data Model Specification: Second Brain v2

## 1. Node Taxonomy (5 Durable Types)

All durable entities are stored as Markdown files with YAML frontmatter and indexed into an in-memory `petgraph::stable_graph::StableDiGraph` per branch (`main` and `manual`).

| Type (`type`) | Path Pattern | Allowed Frontmatter Fields | Invariants |
|---|---|---|---|
| `knowledge` | `knowledge/<slug>.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Durable technical concepts, learnings, patterns, formulas, snippets. No `status` or `priority`. |
| `goal` | `goals/<slug>.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Long-term milestones and directions. No `status` or `priority`. |
| `task` | `tasks/<slug>.md` | `id`, `type`, `title`, `status`, `due?`, `bug?`, `change?`, `tags`, `edges`, `updated_at` | **Only** type with `status` (`open \| in_progress \| blocked \| done`). Optional `due` (`YYYY-MM-DD`), `bug` (`b/<id>`), and `change` (Critique `cl/<id>` or Gerrit change ID). No `priority` field; inherits live P-level from tracked `bug`. |
| `journal` | `journal/YYYY-MM-DD.md` | `id`, `type`, `title`, `tags`, `edges`, `updated_at` | Daily episodic log and reflections. No `status` or `priority`. |
| `relationship` | `relationships/<slug>.md` | `id`, `type`, `title`, `kind?`, `tags`, `edges`, `updated_at` | People and organizations interacted with (`kind`: `person \| org`). No `status` or `priority`. |

## 2. Frontmatter Schemas

### 2.1 Task Node (`tasks/<slug>.md`)
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

### 2.2 Relationship Node (`relationships/<slug>.md`)
```yaml
---
id: relationship:alice-smith
type: relationship
title: Alice Smith
kind: person
tags: [robotics, controls]
edges:
  affiliated_with: [relationship:mit-biomimetic-robotics-lab]
updated_at: 2026-11-01T14:15:00Z
---
```

### 2.3 Knowledge / Goal / Journal Nodes
```yaml
---
id: knowledge:actuator-pd-gain-schedule
type: knowledge
title: Actuator PD Gain Scheduling
tags: [robotics, controls, actuators]
edges:
  depends_on: []
updated_at: 2026-11-01T14:15:00Z
---
```

## 3. Graph Edges & WikiLink Resolution

1. **Typed Frontmatter Edges**:
   - `implements`: `task` $\to$ `goal`
   - `depends_on`: `task` $\to$ `task` | `knowledge`
   - `affiliated_with`: `relationship` $\to$ `relationship`
2. **Inline `[[WikiLinks]]` (`references` edge)**:
   - Any `[[<type>:<slug>]]` or unambiguous `[[<slug>]]` in a Markdown body resolves to the target vault node and registers a directed `references` edge in `petgraph`.
3. **Read-Only External Hydration (`.brain/external_cache.json`)**:
   - External IDs (`bug: b/123456`, `change: cl/654321`) never create stub nodes in `petgraph`.
   - At read/timeline query time, `braind` joins `task.bug` and `task.change` against `.brain/external_cache.json` (populated by `sync:buganizer` and `sync:critique`) to attach live bug P-level (`P0`–`P4`), bug status, and CL review status.
