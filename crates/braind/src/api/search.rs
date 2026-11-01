use super::{error_response, map_tx_error, ApiResponse};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use brain_core::model::extract_wikilinks;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::time::Duration;

const DEFAULT_SEARCH_LIMIT: usize = 10;
const MAX_SEARCH_LIMIT: usize = 100;
const DEFAULT_DEEP_SEED_LIMIT: usize = 5;
const MAX_DEEP_SEED_LIMIT: usize = 20;
const DEEP_SEARCH_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub branch: Option<String>,
    pub limit: Option<usize>,
    pub types: Option<String>,
    pub tags: Option<String>,
}

fn parse_csv_set(raw: Option<&str>) -> HashSet<String> {
    raw.unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub async fn search_tier1(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let branch = AppState::resolve_branch(params.branch.as_deref()).map_err(map_tx_error)?;
    let idx_lock = state.index_for_branch(Some(branch)).map_err(map_tx_error)?;
    let query = params.q.unwrap_or_default();
    let limit = params
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);

    let type_filter = parse_csv_set(params.types.as_deref());
    let tag_filter = parse_csv_set(params.tags.as_deref());
    let has_filters = !type_filter.is_empty() || !tag_filter.is_empty();
    let fetch_limit = if has_filters { MAX_SEARCH_LIMIT } else { limit };

    let hits = {
        let idx = idx_lock.read().unwrap_or_else(|e| e.into_inner());
        let raw_hits = idx.search_rrf(&query, fetch_limit);
        if !has_filters {
            raw_hits
        } else {
            raw_hits
                .into_iter()
                .filter(|hit| {
                    let Some(doc) = idx.get_node(&hit.node_id) else {
                        return false;
                    };
                    if !type_filter.is_empty() {
                        let type_name = serde_json::to_value(doc.meta.node_type)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_lowercase))
                            .unwrap_or_default();
                        if !type_filter.contains(&type_name) {
                            return false;
                        }
                    }
                    if !tag_filter.is_empty() {
                        let doc_tags: HashSet<String> =
                            doc.meta.tags.iter().map(|t| t.to_lowercase()).collect();
                        if !tag_filter.iter().any(|t| doc_tags.contains(t)) {
                            return false;
                        }
                    }
                    true
                })
                .take(limit)
                .collect()
        }
    };

    Ok(Json(json!({
        "branch": branch,
        "query": query,
        "hits": hits,
    })))
}

pub async fn search_deep(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    let branch = AppState::resolve_branch(params.branch.as_deref()).map_err(map_tx_error)?;
    let query = params.q.unwrap_or_default();
    if query.trim().is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "query parameter 'q' is required for deep search",
        ));
    }

    let seed_limit = params
        .limit
        .unwrap_or(DEFAULT_DEEP_SEED_LIMIT)
        .clamp(1, MAX_DEEP_SEED_LIMIT);
    let idx_lock = state.index_for_branch(Some(branch)).map_err(map_tx_error)?;

    let (seed_hits, prompt) = {
        let idx = idx_lock.read().unwrap_or_else(|e| e.into_inner());
        let hits = idx.search_rrf(&query, seed_limit);
        let mut prompt_buf = format!(
            "Synthesize an answer for query: {}\nCite relevant Second Brain nodes using [[node_id]].\n\nSeed Context:\n",
            query.trim()
        );
        for hit in &hits {
            if let Some(doc) = idx.get_node(&hit.node_id) {
                let neighbors = idx.neighbors_1hop(&hit.node_id);
                prompt_buf.push_str(&format!(
                    "--- Node [[{}]] ({}) neighbors={:?} ---\n{}\n",
                    doc.meta.id, doc.meta.title, neighbors, doc.body
                ));
            }
        }
        (hits, prompt_buf)
    };

    let synthesis = state
        .harness
        .run_prompt(&prompt, Duration::from_secs(DEEP_SEARCH_TIMEOUT_SECS))
        .await
        .map_err(|err| {
            error_response(
                StatusCode::BAD_GATEWAY,
                format!("deep search harness error: {}", err),
            )
        })?;
    let cited_nodes = extract_wikilinks(&synthesis);

    Ok(Json(json!({
        "branch": branch,
        "query": query,
        "harness": state.harness.harness_id(),
        "synthesis": synthesis,
        "seed_hits": seed_hits,
        "cited_nodes": cited_nodes,
    })))
}
