use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::{error::AppResult, state::AppState};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub fn api_routes() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz))
}

pub async fn healthz(State(state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await?;
    Ok(Json(HealthResponse { status: "ok" }))
}
