use super::{map_tx_error, ApiResponse};
use crate::state::{AppState, BrainEvent};
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

const DEFAULT_AUDIT_LIMIT: usize = 50;
const MAX_AUDIT_LIMIT: usize = 200;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuditQuery {
    pub branch: Option<String>,
    pub limit: Option<usize>,
}

pub async fn list_audit(
    State(state): State<AppState>,
    Query(params): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let branch = AppState::resolve_branch(params.branch.as_deref()).map_err(map_tx_error)?;
    let limit = params
        .limit
        .unwrap_or(DEFAULT_AUDIT_LIMIT)
        .clamp(1, MAX_AUDIT_LIMIT);
    let commits = state
        .coord
        .list_audit_commits(branch, limit)
        .map_err(map_tx_error)?;

    Ok(Json(json!({
        "branch": branch,
        "commits": commits,
    })))
}

pub async fn revert_audit_commit(
    State(state): State<AppState>,
    Path(sha): Path<String>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let revert_sha = state.coord.revert_commit(&sha).map_err(map_tx_error)?;
    state.reload_indexes().map_err(map_tx_error)?;
    state.emit_event(BrainEvent::TxCommitted {
        tx_id: format!("revert:{}", sha),
        sha: revert_sha.clone(),
        summary: format!("revert: {}", sha),
    });

    Ok(Json(json!({
        "status": "reverted",
        "reverted_sha": sha,
        "revert_commit_sha": revert_sha,
    })))
}
