use axum::{
    Json, Router,
    extract::State,
    middleware,
    routing::{get, post},
};
use serde::Serialize;

use crate::{admin, auth, db, error::AppResult, state::AppState};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub fn api_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .merge(public_api_routes())
        .merge(internal_api_routes())
        .merge(admin_api_routes())
        .merge(browser_routes())
        .merge(integration_routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::session_cookie_middleware,
        ))
}

fn public_api_routes() -> Router<AppState> {
    Router::new().route("/api/v1/session", get(auth::session_me))
}

fn internal_api_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/v1/session",
            get(auth::internal_session_me).delete(auth::internal_logout),
        )
        .route(
            "/internal/v1/auth/registration",
            post(auth::internal_register_complete),
        )
}

fn admin_api_routes() -> Router<AppState> {
    Router::new()
        .route("/internal/v1/admin/users", get(admin::list_users))
        .route("/internal/v1/admin/users/{user_id}", get(admin::get_user))
        .route(
            "/internal/v1/admin/users/{user_id}/disable",
            post(admin::disable_user),
        )
        .route(
            "/internal/v1/admin/users/{user_id}/enable",
            post(admin::enable_user),
        )
}

fn browser_routes() -> Router<AppState> {
    Router::new().route("/account/login", get(auth::account_login))
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

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use reqwest::Client;
    use sqlx::postgres::PgPoolOptions;
    use tower::util::ServiceExt;

    use super::*;
    use crate::{
        config::{
            AppConfig, AppSecrets, AppSection, IngestSection, ObservabilitySection, ServerMode,
            ServerSection, SessionSection, Settings, SsoSection, StorageSection, TransferSection,
        },
        state::AppState,
        string_dict::StringDictCache,
    };

    fn test_state() -> AppState {
        let settings = Settings {
            config: AppConfig {
                app: AppSection {
                    environment: "test".to_owned(),
                    base_url: "http://127.0.0.1:3001".to_owned(),
                },
                server: ServerSection {
                    mode: ServerMode::Http,
                    listen_addr: "127.0.0.1:3001".parse().unwrap(),
                    webui_dist_dir: "../../webui/dist".into(),
                    tls: None,
                },
                session: SessionSection {
                    cookie_name: "tweet_db_sid".to_owned(),
                    pending_login_cookie_name: "tweet_db_sso_state".to_owned(),
                    cookie_secure: false,
                    ttl_hours: 12,
                    absolute_ttl_hours: 720,
                    auto_renew: true,
                    pending_login_ttl_seconds: 600,
                },
                sso: SsoSection {
                    issuer: "http://127.0.0.1:3000".to_owned(),
                    client_id: "tweet-db".to_owned(),
                    login_redirect_uri: "http://127.0.0.1:3001/integrations/sso/callback"
                        .to_owned(),
                    authorization_cache_ttl_seconds: 300,
                },
                ingest: IngestSection {
                    max_items_per_batch: 5000,
                    actor_metrics_min_interval_seconds: 86_400,
                },
                storage: StorageSection {
                    provider: "s3_compatible".to_owned(),
                    endpoint: "http://127.0.0.1:9000".to_owned(),
                    region: "us-east-1".to_owned(),
                    bucket: "tweet-db".to_owned(),
                    object_key_prefix: "media".to_owned(),
                    path_style: true,
                },
                transfer: TransferSection {
                    enabled: false,
                    worker_count: 1,
                    chunk_size_mb: 1,
                    download_parallelism: 1,
                    upload_parallelism: 1,
                    max_in_flight_parts: 1,
                    connect_timeout_seconds: 5,
                    read_timeout_seconds: 30,
                    attempt_timeout_seconds: 300,
                    worker_poll_interval_seconds: 5,
                    max_attempts: 1,
                },
                observability: ObservabilitySection {
                    log_filter: "info".to_owned(),
                },
            },
            secrets: AppSecrets {
                database_url: "postgres://postgres:postgres@127.0.0.1/tweet_db".to_owned(),
                app_token: "test-token".to_owned(),
                session_hmac_key: [7; 32],
                storage_access_key: None,
                storage_secret_key: None,
            },
        };

        AppState::new(
            settings,
            PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@127.0.0.1/tweet_db")
                .unwrap(),
            Client::new(),
            StringDictCache::default(),
        )
    }

    fn test_router() -> Router {
        let state = test_state();
        api_routes(&state).with_state(state)
    }

    #[tokio::test]
    async fn legacy_public_auth_routes_are_not_exposed() {
        let requests = [
            (
                Request::post("/api/v1/auth/login-url")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::NOT_FOUND,
            ),
            (
                Request::post("/api/v1/auth/registration")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::NOT_FOUND,
            ),
            (
                Request::delete("/api/v1/session")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::METHOD_NOT_ALLOWED,
            ),
            (
                Request::post("/api/v1/ingest/submissions")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::NOT_FOUND,
            ),
            (
                Request::post("/api/v1/posts/status/query")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::NOT_FOUND,
            ),
            (
                Request::get("/internal/v1/admin/users")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
            ),
            (
                Request::get("/internal/v1/admin/posts")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::NOT_FOUND,
            ),
        ];

        for (request, expected_status) in requests {
            let response = test_router().oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected_status);
        }
    }
}
