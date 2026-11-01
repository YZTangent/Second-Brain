use crate::model::NodeDocument;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ONE_HOP_NEIGHBOR_WEIGHT: f32 = 1.0;

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
                    self.graph
                        .add_edge(src_idx, dst_idx, EdgeKind::AffiliatedWith);
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
                        *scores.entry(doc.meta.id.clone()).or_insert(0.0) +=
                            ONE_HOP_NEIGHBOR_WEIGHT;
                    }
                }
            }
        }
        scores
    }
}
