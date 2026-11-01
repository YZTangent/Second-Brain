use super::{error_response, map_tx_error, ApiResponse};
use crate::state::{AppState, BrainEvent};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use brain_core::model::NodeDocument;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const HEADER_SESSION_ID: &str = "x-brain-session-id";
const DEFAULT_COMMIT_ACTOR: &str = "agent:mcp";
const MAX_SESSION_ID_LEN: usize = 128;
const MAX_SUMMARY_LEN: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UpsertNodePayload {
    Document(Box<NodeDocument>),
    Markdown { path: String, markdown: String },
}

impl UpsertNodePayload {
    pub fn into_path_and_doc(self) -> (String, NodeDocument) {
        match self {
            Self::Document(doc) => (doc.path.clone(), *doc),
            Self::Markdown { path, markdown } => {
                let doc = NodeDocument::parse(&path, &markdown);
                (path, doc)
            }
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeleteNodeQuery {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteNodeBody {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitTxRequest {
    #[serde(default = "default_commit_actor")]
    pub actor: String,
    pub summary: String,
}

fn default_commit_actor() -> String {
    DEFAULT_COMMIT_ACTOR.to_string()
}

fn extract_session_id(headers: &HeaderMap) -> Result<String, ApiResponse> {
    let raw = headers
        .get(HEADER_SESSION_ID)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    if raw.is_empty() || raw.len() > MAX_SESSION_ID_LEN || raw.chars().any(|c| c.is_control()) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "missing or invalid X-Brain-Session-Id header",
        ));
    }
    Ok(raw.to_string())
}

pub async fn begin_tx(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let session_id = extract_session_id(&headers)?;
    let handle = state.coord.begin_tx(&session_id).map_err(map_tx_error)?;
    Ok(Json(json!({
        "tx_id": handle.tx_id,
        "owner_session_id": handle.owner_session_id,
        "worktree_path": handle.worktree_path,
    })))
}

pub async fn tx_upsert_node(
    State(state): State<AppState>,
    Path(tx_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpsertNodePayload>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let session_id = extract_session_id(&headers)?;
    let (rel_path, doc) = payload.into_path_and_doc();
    state
        .coord
        .stage_upsert(&tx_id, &session_id, &rel_path, &doc)
        .map_err(map_tx_error)?;
    Ok(Json(json!({
        "status": "staged",
        "tx_id": tx_id,
        "path": rel_path,
        "node_id": doc.meta.id,
    })))
}

pub async fn tx_delete_node(
    State(state): State<AppState>,
    Path(tx_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<DeleteNodeQuery>,
    body: Option<Json<DeleteNodeBody>>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let session_id = extract_session_id(&headers)?;
    let rel_path = query
        .path
        .or_else(|| body.map(|b| b.0.path))
        .ok_or_else(|| {
            error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "missing node path for delete",
            )
        })?;
    state
        .coord
        .stage_delete(&tx_id, &session_id, &rel_path)
        .map_err(map_tx_error)?;
    Ok(Json(json!({
        "status": "deleted",
        "tx_id": tx_id,
        "path": rel_path,
    })))
}

pub async fn commit_tx(
    State(state): State<AppState>,
    Path(tx_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<CommitTxRequest>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let session_id = extract_session_id(&headers)?;
    let summary = req.summary.trim();
    let actor = req.actor.trim();
    if summary.is_empty()
        || summary.len() > MAX_SUMMARY_LEN
        || summary.chars().any(|c| c.is_control())
        || actor.is_empty()
        || actor.chars().any(|c| c.is_control())
    {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid commit summary or actor",
        ));
    }

    let sha = state
        .coord
        .commit_tx(&tx_id, &session_id, actor, summary)
        .map_err(map_tx_error)?;
    state.reload_indexes().map_err(map_tx_error)?;
    state.emit_event(BrainEvent::TxCommitted {
        tx_id: tx_id.clone(),
        sha: sha.clone(),
        summary: summary.to_string(),
    });
    Ok(Json(json!({
        "status": "committed",
        "tx_id": tx_id,
        "sha": sha,
    })))
}

pub async fn abort_tx(
    State(state): State<AppState>,
    Path(tx_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let session_id = extract_session_id(&headers)?;
    state
        .coord
        .abort_tx(&tx_id, &session_id)
        .map_err(map_tx_error)?;
    Ok(Json(json!({
        "status": "aborted",
        "tx_id": tx_id,
    })))
}
