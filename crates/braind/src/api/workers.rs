use super::{error_response, map_tx_error, ApiResponse};
use crate::state::{AppState, BrainEvent};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::Json;
use brain_core::workers::synthesizer::run_daily_checkpoint_and_synthesize;
use brain_core::workers::WorkerRunState;
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

const WORKER_SYNTHESIZER: &str = "synthesizer";
const SYNTHESIZER_TRIGGER_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerAction {
    Pause,
    Resume,
    Trigger,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerControlRequest {
    pub worker_id: String,
    pub action: WorkerAction,
}

pub async fn list_workers(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "workers": state.supervisor.list_workers(),
    }))
}

pub async fn control_worker(
    State(state): State<AppState>,
    Json(req): Json<WorkerControlRequest>,
) -> Result<Json<serde_json::Value>, ApiResponse> {
    if state.supervisor.get_worker(&req.worker_id).is_none() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            format!("unknown worker: {}", req.worker_id),
        ));
    }

    match req.action {
        WorkerAction::Pause => {
            state
                .supervisor
                .set_worker_state(&req.worker_id, WorkerRunState::Paused);
            state
                .supervisor
                .append_log(&req.worker_id, "paused via API");
            state.emit_event(BrainEvent::WorkerStateChanged {
                worker_id: req.worker_id.clone(),
                state: WorkerRunState::Paused,
            });
        }
        WorkerAction::Resume => {
            state
                .supervisor
                .set_worker_state(&req.worker_id, WorkerRunState::Running);
            state
                .supervisor
                .append_log(&req.worker_id, "resumed via API");
            state.emit_event(BrainEvent::WorkerStateChanged {
                worker_id: req.worker_id.clone(),
                state: WorkerRunState::Running,
            });
        }
        WorkerAction::Trigger => {
            state
                .supervisor
                .set_worker_state(&req.worker_id, WorkerRunState::Running);
            state
                .supervisor
                .append_log(&req.worker_id, "triggered via API");
            if req.worker_id == WORKER_SYNTHESIZER {
                match run_daily_checkpoint_and_synthesize(
                    &state.coord,
                    state.harness.as_ref(),
                    Duration::from_secs(SYNTHESIZER_TRIGGER_TIMEOUT_SECS),
                )
                .await
                {
                    Ok(_) => {
                        state.reload_indexes().map_err(map_tx_error)?;
                        state
                            .supervisor
                            .set_worker_state(&req.worker_id, WorkerRunState::Idle);
                        state.emit_event(BrainEvent::WorkerStateChanged {
                            worker_id: req.worker_id.clone(),
                            state: WorkerRunState::Idle,
                        });
                    }
                    Err(err) => {
                        let err_state = WorkerRunState::Error(err.to_string());
                        state
                            .supervisor
                            .set_worker_state(&req.worker_id, err_state.clone());
                        state.emit_event(BrainEvent::WorkerStateChanged {
                            worker_id: req.worker_id.clone(),
                            state: err_state,
                        });
                    }
                }
            } else {
                state.emit_event(BrainEvent::WorkerStateChanged {
                    worker_id: req.worker_id.clone(),
                    state: WorkerRunState::Running,
                });
            }
        }
    }

    let info = state
        .supervisor
        .get_worker(&req.worker_id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "worker disappeared"))?;
    Ok(Json(json!(info)))
}

pub async fn events_sse(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.events_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| {
        res.ok().and_then(|ev| {
            serde_json::to_string(&ev)
                .ok()
                .map(|payload| Ok(Event::default().data(payload)))
        })
    });
    Sse::new(stream)
}
