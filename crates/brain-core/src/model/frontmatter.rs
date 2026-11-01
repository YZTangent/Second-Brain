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
