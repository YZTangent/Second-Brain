pub mod audit;
pub mod nodes;
pub mod search;
pub mod tx;
pub mod workers;

use crate::state::AppState;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use brain_core::tx::TxError;
use serde_json::{json, Value};

pub type ApiResponse = (StatusCode, Json<Value>);

pub fn error_response(status: StatusCode, msg: impl Into<String>) -> ApiResponse {
    (
        status,
        Json(json!({
            "error": msg.into(),
        })),
    )
}

pub fn map_tx_error(err: TxError) -> ApiResponse {
    let status = match &err {
        TxError::SessionMismatch { .. } => StatusCode::FORBIDDEN,
        TxError::NotFound(_) => StatusCode::NOT_FOUND,
        TxError::Validation(_) | TxError::InvalidVaultDirectory(_, _) => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        TxError::Git(_) | TxError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error_response(status, err.to_string())
}

pub async fn openapi_spec() -> Json<Value> {
    Json(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Second Brain v2 API (braind)",
            "version": "2.0.0",
            "description": "Decoupled REST and SSE contract for Second Brain v2"
        },
        "paths": {
            "/api/v1/tx": {
                "post": {
                    "summary": "Create session-scoped ephemeral worktree transaction off main",
                    "operationId": "beginTx"
                }
            },
            "/api/v1/tx/{tx_id}/nodes": {
                "put": {
                    "summary": "Stage node upsert in transaction worktree",
                    "operationId": "txUpsertNode"
                },
                "delete": {
                    "summary": "Stage node deletion in transaction worktree",
                    "operationId": "txDeleteNode"
                }
            },
            "/api/v1/tx/{tx_id}/commit": {
                "post": {
                    "summary": "Commit transaction worktree onto main and sync manual",
                    "operationId": "commitTx"
                }
            },
            "/api/v1/tx/{tx_id}/abort": {
                "post": {
                    "summary": "Abort and remove transaction worktree",
                    "operationId": "abortTx"
                }
            },
            "/api/v1/search": {
                "get": {
                    "summary": "Tier-1 Heuristic RRF Search (BM25 + Cosine + 1-hop Graph)",
                    "operationId": "searchTier1"
                }
            },
            "/api/v1/search/deep": {
                "get": {
                    "summary": "Tier-2 Agentic Deep Search via HeadlessHarness",
                    "operationId": "searchDeep"
                }
            },
            "/api/v1/nodes": {
                "get": {
                    "summary": "List nodes on branch with 1-hop edges and external hydration",
                    "operationId": "listNodes"
                }
            },
            "/api/v1/nodes/{id}": {
                "get": {
                    "summary": "Get single node with 1-hop edges and external hydration",
                    "operationId": "getNode"
                },
                "put": {
                    "summary": "UI-only direct node edit on manual branch",
                    "operationId": "uiDirectUpsertNode"
                },
                "delete": {
                    "summary": "UI-only direct node delete on manual branch",
                    "operationId": "uiDirectDeleteNode"
                }
            },
            "/api/v1/graph": {
                "get": {
                    "summary": "UI-only full property graph projection",
                    "operationId": "getGraph"
                }
            },
            "/api/v1/timeline": {
                "get": {
                    "summary": "Daily/weekly timeline projection with hydrated tasks, bugs, changes, and journals",
                    "operationId": "getTimeline"
                }
            },
            "/api/v1/audit": {
                "get": {
                    "summary": "UI-only commit audit history",
                    "operationId": "listAudit"
                }
            },
            "/api/v1/audit/{sha}/revert": {
                "post": {
                    "summary": "UI-only atomic git revert of a commit on main",
                    "operationId": "revertAuditCommit"
                }
            },
            "/api/v1/workers": {
                "get": {
                    "summary": "UI-only inspect supervised workers",
                    "operationId": "listWorkers"
                },
                "post": {
                    "summary": "UI-only pause, resume, or trigger a worker",
                    "operationId": "controlWorker"
                }
            },
            "/api/v1/events": {
                "get": {
                    "summary": "UI-only SSE real-time event stream",
                    "operationId": "eventsSse"
                }
            }
        }
    }))
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/tx", post(tx::begin_tx))
        .route(
            "/api/v1/tx/:tx_id/nodes",
            put(tx::tx_upsert_node).delete(tx::tx_delete_node),
        )
        .route("/api/v1/tx/:tx_id/commit", post(tx::commit_tx))
        .route("/api/v1/tx/:tx_id/abort", post(tx::abort_tx))
        .route("/api/v1/search", get(search::search_tier1))
        .route("/api/v1/search/deep", get(search::search_deep))
        .route("/api/v1/nodes", get(nodes::list_nodes))
        .route(
            "/api/v1/nodes/:id",
            get(nodes::get_node)
                .put(nodes::ui_direct_upsert_node)
                .delete(nodes::ui_direct_delete_node),
        )
        .route("/api/v1/graph", get(nodes::get_graph))
        .route("/api/v1/timeline", get(nodes::get_timeline))
        .route("/api/v1/audit", get(audit::list_audit))
        .route(
            "/api/v1/audit/:sha/revert",
            post(audit::revert_audit_commit),
        )
        .route(
            "/api/v1/workers",
            get(workers::list_workers).post(workers::control_worker),
        )
        .route("/api/v1/events", get(workers::events_sse))
        .route("/api/v1/openapi.json", get(openapi_spec))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use brain_core::config::BrainConfig;
    use brain_core::index::{ExternalBug, ExternalChange};
    use brain_core::model::{EdgeMap, NodeDocument, NodeFrontmatter, NodeType, TaskStatus};
    use brain_core::tx::WorktreeCoordinator;
    use brain_core::workers::harness::HeadlessHarness;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tower::ServiceExt;

    struct MockDeepHarness;

    #[async_trait]
    impl HeadlessHarness for MockDeepHarness {
        fn harness_id(&self) -> &str {
            "mock-deep"
        }

        async fn run_prompt(&self, prompt: &str, _timeout: Duration) -> anyhow::Result<String> {
            Ok(format!(
                "Synthesized answer referencing [[knowledge:pd-gains]] from prompt len={}",
                prompt.len()
            ))
        }
    }

    fn test_app_state() -> (TempDir, AppState) {
        let tmp = TempDir::new().unwrap();
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("config.toml");
        let cfg = BrainConfig::load(&config_path).unwrap();
        let coord = WorktreeCoordinator::new(tmp.path().join("vault")).unwrap();
        let state = AppState::with_harness(cfg, coord, Arc::new(MockDeepHarness)).unwrap();
        (tmp, state)
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_api_tx_session_enforcement_branch_selection_and_get_search() {
        let (_tmp, state) = test_app_state();
        let app = build_router(state.clone());

        // 1. Missing X-Brain-Session-Id returns 400 Bad Request
        let req_missing_session = Request::builder()
            .method("POST")
            .uri("/api/v1/tx")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req_missing_session).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 2. Begin tx with session-A succeeds
        let req_begin = Request::builder()
            .method("POST")
            .uri("/api/v1/tx")
            .header("X-Brain-Session-Id", "session-A")
            .body(Body::empty())
            .unwrap();
        let resp_begin = app.clone().oneshot(req_begin).await.unwrap();
        assert_eq!(resp_begin.status(), StatusCode::OK);
        let begin_json = body_json(resp_begin).await;
        let tx_id = begin_json["tx_id"].as_str().unwrap().to_string();

        let doc = NodeDocument {
            path: "knowledge/pd-gains.md".to_string(),
            meta: NodeFrontmatter {
                id: "knowledge:pd-gains".to_string(),
                node_type: NodeType::Knowledge,
                title: "PD Gain Scheduling".to_string(),
                status: None,
                due: None,
                bug: None,
                change: None,
                kind: None,
                tags: vec!["robotics".to_string(), "controls".to_string()],
                edges: EdgeMap::default(),
                updated_at: "2026-11-01T12:00:00Z".to_string(),
            },
            body: "# Gains\nBiquad notch filter tuning for ankle actuators.".to_string(),
            parse_error: false,
        };

        // 3. Mismatched session-B returns 403 Forbidden on upsert and commit
        let req_mismatch_upsert = Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/tx/{}/nodes", tx_id))
            .header("X-Brain-Session-Id", "session-B")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&doc).unwrap()))
            .unwrap();
        let resp_mismatch = app.clone().oneshot(req_mismatch_upsert).await.unwrap();
        assert_eq!(resp_mismatch.status(), StatusCode::FORBIDDEN);

        // 4. Unknown tx_id returns 404 Not Found
        let req_not_found = Request::builder()
            .method("PUT")
            .uri("/api/v1/tx/tx-nonexistent/nodes")
            .header("X-Brain-Session-Id", "session-A")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&doc).unwrap()))
            .unwrap();
        let resp_not_found = app.clone().oneshot(req_not_found).await.unwrap();
        assert_eq!(resp_not_found.status(), StatusCode::NOT_FOUND);

        // 5. Invalid path traversal returns 422 Unprocessable Entity
        let mut invalid_doc = doc.clone();
        invalid_doc.path = "../escape.md".to_string();
        let req_invalid = Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/tx/{}/nodes", tx_id))
            .header("X-Brain-Session-Id", "session-A")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&invalid_doc).unwrap()))
            .unwrap();
        let resp_invalid = app.clone().oneshot(req_invalid).await.unwrap();
        assert_eq!(resp_invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // 6. Owner session-A upserts and commits
        let req_upsert = Request::builder()
            .method("PUT")
            .uri(format!("/api/v1/tx/{}/nodes", tx_id))
            .header("X-Brain-Session-Id", "session-A")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&doc).unwrap()))
            .unwrap();
        let resp_upsert = app.clone().oneshot(req_upsert).await.unwrap();
        assert_eq!(resp_upsert.status(), StatusCode::OK);

        let req_commit = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tx/{}/commit", tx_id))
            .header("X-Brain-Session-Id", "session-A")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "actor": "agent:jetski",
                    "summary": "remember: pd gain scheduling"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp_commit = app.clone().oneshot(req_commit).await.unwrap();
        assert_eq!(resp_commit.status(), StatusCode::OK);
        let commit_json = body_json(resp_commit).await;
        let commit_sha = commit_json["sha"].as_str().unwrap().to_string();

        // 7. Direct UI PUT onto `manual` branch creates a node only on `manual`
        let manual_only_doc = NodeDocument {
            path: "knowledge/manual-draft.md".to_string(),
            meta: NodeFrontmatter {
                id: "knowledge:manual-draft".to_string(),
                node_type: NodeType::Knowledge,
                title: "Manual Draft Note".to_string(),
                status: None,
                due: None,
                bug: None,
                change: None,
                kind: None,
                tags: vec!["draft".to_string()],
                edges: EdgeMap::default(),
                updated_at: "2026-11-01T12:05:00Z".to_string(),
            },
            body: "Draft content only on manual branch.".to_string(),
            parse_error: false,
        };
        let req_ui_put = Request::builder()
            .method("PUT")
            .uri("/api/v1/nodes/knowledge:manual-draft")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&manual_only_doc).unwrap()))
            .unwrap();
        let resp_ui_put = app.clone().oneshot(req_ui_put).await.unwrap();
        assert_eq!(resp_ui_put.status(), StatusCode::OK);

        // Verify branch isolation via GET /api/v1/nodes (default main vs ?branch=manual)
        let resp_nodes_main = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_nodes_main.status(), StatusCode::OK);
        let nodes_main_json = body_json(resp_nodes_main).await;
        assert_eq!(nodes_main_json["branch"], "main");
        assert_eq!(nodes_main_json["nodes"].as_array().unwrap().len(), 1);

        let resp_nodes_manual = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nodes?branch=manual")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_nodes_manual.status(), StatusCode::OK);
        let nodes_manual_json = body_json(resp_nodes_manual).await;
        assert_eq!(nodes_manual_json["branch"], "manual");
        assert_eq!(nodes_manual_json["nodes"].as_array().unwrap().len(), 2);

        // 8. GET /api/v1/search and GET /api/v1/search/deep
        let resp_search = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/search?q=biquad&branch=main&limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_search.status(), StatusCode::OK);
        let search_json = body_json(resp_search).await;
        assert_eq!(
            search_json["hits"][0]["node_id"].as_str(),
            Some("knowledge:pd-gains")
        );

        let resp_deep = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/search/deep?q=biquad&branch=main")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_deep.status(), StatusCode::OK);
        let deep_json = body_json(resp_deep).await;
        assert!(deep_json["synthesis"]
            .as_str()
            .unwrap()
            .contains("[[knowledge:pd-gains]]"));
        assert_eq!(
            deep_json["cited_nodes"].as_array().unwrap()[0].as_str(),
            Some("knowledge:pd-gains")
        );

        // 9. Audit list and revert commit via POST /api/v1/audit/:sha/revert
        let resp_audit = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/audit?branch=main")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_audit.status(), StatusCode::OK);
        let audit_json = body_json(resp_audit).await;
        assert_eq!(
            audit_json["commits"][0]["sha"].as_str(),
            Some(commit_sha.as_str())
        );

        let resp_revert = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/audit/{}/revert", commit_sha))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_revert.status(), StatusCode::OK);

        // Verify pd-gains was removed from main after revert while manual-draft remains on manual
        let resp_after_revert = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nodes?branch=main")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let after_revert_json = body_json(resp_after_revert).await;
        assert_eq!(after_revert_json["nodes"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_api_timeline_graph_workers_and_openapi_contract() {
        let (_tmp, state) = test_app_state();
        {
            let mut ext = state.external_cache.write().unwrap();
            ext.bugs.insert(
                "b/123".to_string(),
                ExternalBug {
                    id: "b/123".to_string(),
                    title: "Fix controller oscillation".to_string(),
                    priority: "P0".to_string(),
                    status: "ASSIGNED".to_string(),
                },
            );
            ext.changes.insert(
                "cl/456".to_string(),
                ExternalChange {
                    id: "cl/456".to_string(),
                    title: "feat: notch filter".to_string(),
                    status: "APPROVED".to_string(),
                },
            );
        }

        let task_doc = NodeDocument {
            path: "tasks/fix-osc.md".to_string(),
            meta: NodeFrontmatter {
                id: "task:fix-osc".to_string(),
                node_type: NodeType::Task,
                title: "Fix oscillation".to_string(),
                status: Some(TaskStatus::InProgress),
                due: Some("2026-11-10".to_string()),
                bug: Some("b/123".to_string()),
                change: Some("cl/456".to_string()),
                kind: None,
                tags: vec!["controls".to_string()],
                edges: EdgeMap::default(),
                updated_at: "2026-11-01T12:00:00Z".to_string(),
            },
            body: "Task details.".to_string(),
            parse_error: false,
        };
        let tx = state.coord.begin_tx("sess-tl").unwrap();
        state
            .coord
            .stage_upsert(&tx.tx_id, "sess-tl", "tasks/fix-osc.md", &task_doc)
            .unwrap();
        state
            .coord
            .commit_tx(&tx.tx_id, "sess-tl", "agent:jetski", "remember: task")
            .unwrap();
        state.reload_indexes().unwrap();

        let app = build_router(state);

        // GET /api/v1/nodes/:id hydrates bug & change
        let resp_node = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nodes/task:fix-osc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_node.status(), StatusCode::OK);
        let node_json = body_json(resp_node).await;
        assert_eq!(node_json["external_bug"]["priority"], "P0");
        assert_eq!(node_json["external_change"]["status"], "APPROVED");

        // GET /api/v1/timeline
        let resp_tl = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/timeline")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_tl.status(), StatusCode::OK);
        let tl_json = body_json(resp_tl).await;
        assert_eq!(tl_json["tasks"][0]["id"], "task:fix-osc");
        assert_eq!(tl_json["tasks"][0]["inherited_priority"], "P0");

        // GET /api/v1/graph
        let resp_graph = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/graph")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_graph.status(), StatusCode::OK);
        let graph_json = body_json(resp_graph).await;
        assert_eq!(graph_json["nodes"].as_array().unwrap().len(), 1);

        // GET & POST /api/v1/workers
        let resp_workers = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/workers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_workers.status(), StatusCode::OK);
        let workers_json = body_json(resp_workers).await;
        assert!(workers_json["workers"].as_array().unwrap().len() >= 5);

        let resp_pause = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/workers")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "worker_id": "observer:jetski",
                            "action": "pause"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_pause.status(), StatusCode::OK);
        let pause_json = body_json(resp_pause).await;
        assert_eq!(pause_json["state"], "paused");

        // GET /api/v1/openapi.json OpenAPI 3.1 contract snapshot
        let resp_openapi = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_openapi.status(), StatusCode::OK);
        let spec = body_json(resp_openapi).await;
        assert_eq!(spec["openapi"], "3.1.0");
        for required_path in [
            "/api/v1/tx",
            "/api/v1/tx/{tx_id}/nodes",
            "/api/v1/tx/{tx_id}/commit",
            "/api/v1/tx/{tx_id}/abort",
            "/api/v1/search",
            "/api/v1/search/deep",
            "/api/v1/nodes",
            "/api/v1/nodes/{id}",
            "/api/v1/graph",
            "/api/v1/timeline",
            "/api/v1/audit",
            "/api/v1/audit/{sha}/revert",
            "/api/v1/workers",
            "/api/v1/events",
        ] {
            assert!(
                spec["paths"].get(required_path).is_some(),
                "missing OpenAPI path: {}",
                required_path
            );
        }
    }
}
