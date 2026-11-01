pub mod document;
pub mod frontmatter;
pub mod types;
pub mod wikilink;

pub use document::NodeDocument;
pub use frontmatter::NodeFrontmatter;
pub use types::{EdgeMap, NodeType, RelationshipKind, TaskStatus};
pub use wikilink::extract_wikilinks;

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

    #[test]
    fn test_all_five_node_types_folder_and_roundtrip() {
        let cases = vec![
            (
                NodeType::Knowledge,
                "knowledge",
                "knowledge/pd-gains.md",
                NodeFrontmatter {
                    id: "knowledge:pd-gains".to_string(),
                    node_type: NodeType::Knowledge,
                    title: "PD Gains".to_string(),
                    status: None,
                    due: None,
                    bug: None,
                    change: None,
                    kind: None,
                    tags: vec!["controls".to_string()],
                    edges: EdgeMap::default(),
                    updated_at: "2026-11-01T14:15:00Z".to_string(),
                },
            ),
            (
                NodeType::Goal,
                "goals",
                "goals/q4-bimanual.md",
                NodeFrontmatter {
                    id: "goal:q4-bimanual".to_string(),
                    node_type: NodeType::Goal,
                    title: "Q4 Bimanual Milestone".to_string(),
                    status: None,
                    due: None,
                    bug: None,
                    change: None,
                    kind: None,
                    tags: vec!["q4".to_string()],
                    edges: EdgeMap::default(),
                    updated_at: "2026-11-01T14:15:00Z".to_string(),
                },
            ),
            (
                NodeType::Task,
                "tasks",
                "tasks/port-rl.md",
                NodeFrontmatter {
                    id: "task:port-rl".to_string(),
                    node_type: NodeType::Task,
                    title: "Port RL".to_string(),
                    status: Some(TaskStatus::Open),
                    due: Some("2026-12-01".to_string()),
                    bug: Some("b/123456".to_string()),
                    change: Some("cl/654321".to_string()),
                    kind: None,
                    tags: vec!["rl".to_string()],
                    edges: EdgeMap {
                        implements: vec!["goal:q4-bimanual".to_string()],
                        depends_on: vec!["knowledge:pd-gains".to_string()],
                        affiliated_with: vec![],
                    },
                    updated_at: "2026-11-01T14:15:00Z".to_string(),
                },
            ),
            (
                NodeType::Journal,
                "journal",
                "journal/2026-11-01.md",
                NodeFrontmatter {
                    id: "journal:2026-11-01".to_string(),
                    node_type: NodeType::Journal,
                    title: "2026-11-01 Daily Log".to_string(),
                    status: None,
                    due: None,
                    bug: None,
                    change: None,
                    kind: None,
                    tags: vec!["daily".to_string()],
                    edges: EdgeMap::default(),
                    updated_at: "2026-11-01T14:15:00Z".to_string(),
                },
            ),
            (
                NodeType::Relationship,
                "relationships",
                "relationships/mit-lab.md",
                NodeFrontmatter {
                    id: "relationship:mit-lab".to_string(),
                    node_type: NodeType::Relationship,
                    title: "MIT Biomimetic Robotics Lab".to_string(),
                    status: None,
                    due: None,
                    bug: None,
                    change: None,
                    kind: Some(RelationshipKind::Org),
                    tags: vec!["lab".to_string()],
                    edges: EdgeMap::default(),
                    updated_at: "2026-11-01T14:15:00Z".to_string(),
                },
            ),
        ];

        for (expected_type, expected_folder, path, meta) in cases {
            assert_eq!(expected_type.folder_name(), expected_folder);
            let doc = NodeDocument {
                path: path.to_string(),
                meta: meta.clone(),
                body: format!("Body for {}", meta.id),
                parse_error: false,
            };
            assert!(doc.validate().is_ok());
            let md = doc.to_markdown().expect("serialize to markdown");
            let reparsed = NodeDocument::parse(path, &md);
            assert!(!reparsed.parse_error);
            assert_eq!(reparsed.meta, meta);
            assert_eq!(reparsed.body.trim_end(), format!("Body for {}", meta.id));
        }
    }

    #[test]
    fn test_wikilink_anchors_aliases_and_deduplication() {
        let body = "See [[knowledge:pd-gains#tuning|PD Tuning]] and [[knowledge:pd-gains]] plus [[goal:q4-bimanual|Q4 Goal]].";
        assert_eq!(
            extract_wikilinks(body),
            vec![
                "knowledge:pd-gains".to_string(),
                "goal:q4-bimanual".to_string(),
            ]
        );
    }

    #[test]
    fn test_validation_rejects_task_without_status_and_invalid_kind() {
        let task_without_status = r#"---
id: task:missing-status
type: task
title: Missing Status
updated_at: "2026-11-01T14:15:00Z"
---
Body
"#;
        let doc = NodeDocument::parse("tasks/missing-status.md", task_without_status);
        assert!(doc.validate().is_err());

        let goal_with_kind = r#"---
id: goal:invalid-kind
type: goal
title: Invalid Kind
kind: org
updated_at: "2026-11-01T14:15:00Z"
---
Body
"#;
        let doc2 = NodeDocument::parse("goals/invalid-kind.md", goal_with_kind);
        assert!(doc2.validate().is_err());
    }
}
