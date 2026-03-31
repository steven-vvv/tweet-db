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
        .route("/api/auth/login-url", post(auth::login_url))
        .route("/auth/sso/callback", get(auth::sso_callback))
        .route("/api/session/me", get(auth::session_me))
        .route("/api/auth/register/complete", post(auth::register_complete))
        .route("/api/session/logout", post(auth::logout))
        .route("/api/ingest/submissions", post(content::ingest_submission))
        .route("/api/posts/status/query", post(content::query_post_status))
        .route(
            "/auth/sso/webhooks/revocations",
            post(auth::revocation_webhook),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::session_cookie_middleware,
        ))
}

pub async fn healthz(State(state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    db::healthcheck(&state.db).await?;
    Ok(Json(HealthResponse { status: "ok" }))
}
