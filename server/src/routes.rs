use axum::{
    Json, Router,
    extract::State,
    middleware,
    routing::{get, post},
};
use serde::Serialize;

use crate::{
    admin, auth, db, error::AppResult, internal_api, state::AppState, tweet_query, tweet_submit,
};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub fn api_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .merge(public_api_routes())
        .merge(internal_api_routes())
        .merge(internal_api::v2::routes())
        .merge(admin_api_routes())
        .merge(browser_routes())
        .merge(integration_routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::session_cookie_middleware,
        ))
}

fn public_api_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/session", get(auth::session_me))
        .route("/api/v1/tweet/submit", post(tweet_submit::submit_tweets))
        .route("/api/v1/tweet/query", post(tweet_query::query_tweets))
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
        .route("/internal/v1/admin/overview", get(admin::overview))
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
        .route(
            "/internal/v1/admin/twitter-users",
            get(admin::list_twitter_users),
        )
        .route(
            "/internal/v1/admin/twitter-users/{user_id}",
            get(admin::get_twitter_user),
        )
        .route("/internal/v1/admin/tweets", get(admin::list_tweets))
        .route(
            "/internal/v1/admin/tweets/{tweet_id}",
            get(admin::get_tweet),
        )
        .route("/internal/v1/admin/media", get(admin::list_media))
        .route("/internal/v1/admin/media/{media_id}", get(admin::get_media))
        .route(
            "/internal/v1/admin/media/{media_id}/transfer-tasks",
            post(admin::create_media_transfer_task),
        )
        .route(
            "/internal/v1/admin/storage-objects",
            get(admin::list_storage_objects),
        )
        .route(
            "/internal/v1/admin/storage-objects/{object_id}",
            get(admin::get_storage_object),
        )
        .route(
            "/internal/v1/admin/storage-objects/{object_id}/open",
            get(admin::open_storage_object),
        )
        .route(
            "/internal/v1/admin/transfers/overview",
            get(admin::transfer_overview),
        )
        .route(
            "/internal/v1/admin/transfers/tasks",
            get(admin::list_transfer_tasks),
        )
        .route(
            "/internal/v1/admin/transfers/tasks/{task_id}",
            get(admin::get_transfer_task),
        )
        .route(
            "/internal/v1/admin/transfers/tasks/{task_id}/retry",
            post(admin::retry_transfer_task),
        )
        .route(
            "/internal/v1/admin/transfers/tasks/{task_id}/cancel",
            post(admin::cancel_transfer_task),
        )
        .route(
            "/internal/v1/admin/transfers/tasks/{task_id}/release",
            post(admin::release_transfer_task),
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
            AppConfig, AppSecrets, AppSection, IngestSection, ObservabilitySection, SearchSection,
            ServerMode, ServerSection, SessionSection, Settings, SsoSection, StorageSection,
            TransferSection,
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
                    stats_sample_interval_seconds: 3_600,
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
                    task_stale_timeout_seconds: 900,
                    worker_poll_interval_seconds: 5,
                    max_attempts: 1,
                },
                search: SearchSection {
                    enabled: false,
                    index_dir: "var/search".into(),
                    worker_count: 1,
                    queue_batch_size: 200,
                    writer_memory_mb: 128,
                    commit_interval_seconds: 5,
                    stale_timeout_seconds: 300,
                    max_attempts: 8,
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
                Request::get("/internal/v2/me").body(Body::empty()).unwrap(),
                StatusCode::UNAUTHORIZED,
            ),
            (
                Request::get("/internal/v2/system/summary")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
            ),
            (
                Request::get("/internal/v2/media/1/open")
                    .body(Body::empty())
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
            ),
            (
                Request::post("/api/v1/tweet/submit")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"users":[],"tweets":[],"media":[]}"#))
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
            ),
            (
                Request::post("/api/v1/tweet/query")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"users":[],"tweets":[],"media":[]}"#))
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
            ),
            (
                Request::post("/internal/v1/tweet/submit")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"users":[],"tweets":[],"media":[]}"#))
                    .unwrap(),
                StatusCode::NOT_FOUND,
            ),
            (
                Request::post("/internal/v1/tweet/query")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"users":[],"tweets":[],"media":[]}"#))
                    .unwrap(),
                StatusCode::NOT_FOUND,
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

    #[tokio::test]
    async fn v2_flattened_limit_query_accepts_url_strings() {
        let response = test_router()
            .oneshot(
                Request::get("/internal/v2/tweets?relation=all&include=author%2Cstats&limit=30")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
