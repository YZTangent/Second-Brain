use super::tx::{DeleteNodeQuery, UpsertNodePayload};
use super::{error_response, map_tx_error, ApiResponse};
use crate::state::{AppState, BrainEvent};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use brain_core::index::{ExternalBug, ExternalCache, ExternalChange};
use brain_core::model::NodeDocument;
use brain_core::tx::session::BRANCH_MANUAL;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BranchQuery {
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HydratedNode {
    pub document: NodeDocument,
    pub neighbors_1hop: Vec<String>,
    pub external_bug: Option<ExternalBug>,
    pub external_change: Option<ExternalChange>,
}

fn hydrate_node(doc: &NodeDocument, neighbors: Vec<String>, ext: &ExternalCache) -> HydratedNode {
    let external_bug = doc
        .meta
        .bug
        .as_ref()
        .and_then(|bug_id| ext.bugs.get(bug_id).cloned());
    let external_change = doc
        .meta
        .change
        .as_ref()
        .and_then(|cl_id| ext.changes.get(cl_id).cloned());
    HydratedNode {
        document: doc.clone(),
        neighbors_1hop: neighbors,
        external_bug,
        external_change,
    }
}

pub async fn list_nodes(
    State(state): State<AppState>,
    Query(params): Query<BranchQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let branch = AppState::resolve_branch(params.branch.as_deref()).map_err(map_tx_error)?;
    let idx_lock = state.index_for_branch(Some(branch)).map_err(map_tx_error)?;

    let nodes = {
        let idx = idx_lock.read().unwrap_or_else(|e| e.into_inner());
        let ext = state
            .external_cache
            .read()
            .unwrap_or_else(|e| e.into_inner());
        idx.list_nodes()
            .into_iter()
            .map(|doc| {
                let neighbors = idx.neighbors_1hop(&doc.meta.id);
                hydrate_node(&doc, neighbors, &ext)
            })
            .collect::<Vec<_>>()
    };

    Ok(Json(json!({
        "branch": branch,
        "nodes": nodes,
    })))
}

pub async fn get_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    Query(params): Query<BranchQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let branch = AppState::resolve_branch(params.branch.as_deref()).map_err(map_tx_error)?;
    let idx_lock = state.index_for_branch(Some(branch)).map_err(map_tx_error)?;

    let hydrated = {
        let idx = idx_lock.read().unwrap_or_else(|e| e.into_inner());
        let doc = idx.get_node(&node_id).ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                format!("node not found: {}", node_id),
            )
        })?;
        let neighbors = idx.neighbors_1hop(&node_id);
        let ext = state
            .external_cache
            .read()
            .unwrap_or_else(|e| e.into_inner());
        hydrate_node(doc, neighbors, &ext)
    };

    Ok(Json(json!({
        "branch": branch,
        "document": hydrated.document,
        "neighbors_1hop": hydrated.neighbors_1hop,
        "external_bug": hydrated.external_bug,
        "external_change": hydrated.external_change,
    })))
}

pub async fn ui_direct_upsert_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    Json(payload): Json<UpsertNodePayload>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let (rel_path, doc) = payload.into_path_and_doc();
    if doc.meta.id != node_id {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "path node_id '{}' does not match frontmatter id '{}'",
                node_id, doc.meta.id
            ),
        ));
    }

    state
        .coord
        .direct_manual_upsert(&rel_path, &doc)
        .map_err(map_tx_error)?;
    state.reload_indexes().map_err(map_tx_error)?;
    state.emit_event(BrainEvent::NodeUpdated {
        branch: BRANCH_MANUAL.to_string(),
        node_id: node_id.clone(),
    });

    Ok(Json(json!({
        "status": "updated",
        "branch": BRANCH_MANUAL,
        "node_id": node_id,
        "path": rel_path,
    })))
}

pub async fn ui_direct_delete_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    Query(query): Query<DeleteNodeQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let rel_path = if let Some(path) = query.path {
        path
    } else {
        let manual_idx = state.manual_index.read().unwrap_or_else(|e| e.into_inner());
        manual_idx
            .get_node(&node_id)
            .map(|d| d.path.clone())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    format!("node not found on manual branch: {}", node_id),
                )
            })?
    };

    state
        .coord
        .direct_manual_delete(&rel_path)
        .map_err(map_tx_error)?;
    state.reload_indexes().map_err(map_tx_error)?;
    state.emit_event(BrainEvent::NodeUpdated {
        branch: BRANCH_MANUAL.to_string(),
        node_id: node_id.clone(),
    });

    Ok(Json(json!({
        "status": "deleted",
        "branch": BRANCH_MANUAL,
        "node_id": node_id,
        "path": rel_path,
    })))
}

pub async fn get_graph(
    State(state): State<AppState>,
    Query(params): Query<BranchQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let branch = AppState::resolve_branch(params.branch.as_deref()).map_err(map_tx_error)?;
    let idx_lock = state.index_for_branch(Some(branch)).map_err(map_tx_error)?;

    let (nodes, edges) = {
        let idx = idx_lock.read().unwrap_or_else(|e| e.into_inner());
        let g = &idx.graph_store.graph;
        let nodes_vec: Vec<serde_json::Value> = idx
            .list_nodes()
            .into_iter()
            .map(|d| {
                json!({
                    "id": d.meta.id,
                    "title": d.meta.title,
                    "node_type": d.meta.node_type,
                    "tags": d.meta.tags,
                    "parse_error": d.parse_error,
                })
            })
            .collect();

        let mut edges_vec: Vec<serde_json::Value> = Vec::new();
        for edge_idx in g.edge_indices() {
            if let (Some((src_idx, dst_idx)), Some(kind)) =
                (g.edge_endpoints(edge_idx), g.edge_weight(edge_idx))
            {
                if let (Some(src), Some(dst)) = (g.node_weight(src_idx), g.node_weight(dst_idx)) {
                    edges_vec.push(json!({
                        "source": src.meta.id,
                        "target": dst.meta.id,
                        "kind": kind,
                    }));
                }
            }
        }
        (nodes_vec, edges_vec)
    };

    Ok(Json(json!({
        "branch": branch,
        "nodes": nodes,
        "edges": edges,
    })))
}

pub async fn get_timeline(
    State(state): State<AppState>,
    Query(params): Query<BranchQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let branch = AppState::resolve_branch(params.branch.as_deref()).map_err(map_tx_error)?;
    let idx_lock = state.index_for_branch(Some(branch)).map_err(map_tx_error)?;

    let projection = {
        let idx = idx_lock.read().unwrap_or_else(|e| e.into_inner());
        let ext = state
            .external_cache
            .read()
            .unwrap_or_else(|e| e.into_inner());
        idx.build_timeline(&ext)
    };

    Ok(Json(json!({
        "branch": branch,
        "tasks": projection.tasks,
        "unlinked_bugs": projection.unlinked_bugs,
        "unlinked_changes": projection.unlinked_changes,
        "journals": projection.journals,
    })))
}
