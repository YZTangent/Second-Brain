use super::graph::GraphStore;
use crate::model::{NodeDocument, NodeType, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

const DEFAULT_PRIORITY: &str = "P9";

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
    #[serde(default)]
    pub bugs: BTreeMap<String, ExternalBug>,
    #[serde(default)]
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
        let pa = a.inherited_priority.as_deref().unwrap_or(DEFAULT_PRIORITY);
        let pb = b.inherited_priority.as_deref().unwrap_or(DEFAULT_PRIORITY);
        pa.cmp(pb)
            .then_with(|| a.due.cmp(&b.due))
            .then_with(|| a.id.cmp(&b.id))
    });

    journals.sort_by(|a, b| b.meta.id.cmp(&a.meta.id));

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
