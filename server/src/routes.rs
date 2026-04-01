use axum::{
    Json, Router,
    extract::State,
    middleware,
    routing::{get, post},
};
use serde::Serialize;

use crate::{auth, content, db, error::AppResult, state::AppState};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub fn api_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .merge(public_api_routes())
        .merge(internal_api_routes())
        .merge(integration_routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::session_cookie_middleware,
        ))
}

fn public_api_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login-url", post(auth::login_url))
        .route(
            "/api/v1/session",
            get(auth::session_me).delete(auth::logout),
        )
        .route("/api/v1/auth/registration", post(auth::register_complete))
        .route(
            "/api/v1/ingest/submissions",
            post(content::ingest_submission),
        )
        .route(
            "/api/v1/posts/status/query",
            post(content::query_post_status),
        )
}

fn internal_api_routes() -> Router<AppState> {
    Router::new().route("/internal/v1/session", get(auth::internal_session_me))
}

fn integration_routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/sso/callback", get(auth::sso_callback))
        .route(
            "/integrations/sso/webhooks/revocations",
            post(auth::revocation_webhook),
        )
}

pub async fn healthz(State(state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    db::healthcheck(&state.db).await?;
    Ok(Json(HealthResponse { status: "ok" }))
}
