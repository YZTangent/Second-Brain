use super::frontmatter::NodeFrontmatter;
use super::types::{EdgeMap, NodeType};
use super::wikilink::extract_wikilinks;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

const FRONTMATTER_OPEN: &str = "---\n";
const FRONTMATTER_CLOSE: &str = "\n---";

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
        if let Some(rest) = trimmed.strip_prefix(FRONTMATTER_OPEN) {
            if let Some(end_idx) = rest.find(FRONTMATTER_CLOSE) {
                let yaml_str = &rest[..end_idx];
                let after = &rest[end_idx + FRONTMATTER_CLOSE.len()..];
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
