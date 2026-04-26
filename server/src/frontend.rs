use std::path::Path;

use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

pub fn routes(state: &AppState) -> Router<AppState> {
    let dist_dir = state.settings.config.server.webui_dist_dir.clone();

    Router::new()
        .route("/", get(root_redirect))
        .route("/login", get(account_redirect))
        .route("/register", get(account_redirect))
        .route("/account", get(spa_index))
        .route("/account/{*path}", get(spa_index))
        .route("/admin", get(spa_index))
        .route("/admin/{*path}", get(spa_index))
        .nest_service("/assets", ServeDir::new(dist_dir.join("assets")))
        .route_service("/favicon.svg", ServeFile::new(dist_dir.join("favicon.svg")))
}

async fn root_redirect() -> Redirect {
    Redirect::temporary("/account")
}

async fn account_redirect() -> Redirect {
    Redirect::temporary("/account")
}

async fn spa_index(State(state): State<AppState>) -> Response {
    if dist_exists(&state) {
        let index_path = state
            .settings
            .config
            .server
            .webui_dist_dir
            .join("index.html");
        match tokio::fs::read_to_string(index_path).await {
            Ok(contents) => Html(contents).into_response(),
            Err(_) => placeholder_index().into_response(),
        }
    } else {
        placeholder_index().into_response()
    }
}

fn placeholder_index() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>tweet-db</title>
  </head>
  <body>
    <div id="app"></div>
    <p>Web UI is not built yet. Run the Vite build in webui/ to serve the SPA.</p>
  </body>
</html>"#,
    )
}

pub fn dist_exists(state: &AppState) -> bool {
    Path::new(&state.settings.config.server.webui_dist_dir).exists()
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
        routes(&state).with_state(state)
    }

    #[tokio::test]
    async fn legacy_entrypoints_redirect_to_account() {
        for path in ["/", "/login", "/register"] {
            let response = test_router()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
            assert_eq!(response.headers().get("location").unwrap(), "/account");
        }
    }

    #[tokio::test]
    async fn admin_entrypoint_returns_placeholder_when_dist_is_missing() {
        let response = test_router()
            .oneshot(Request::get("/admin/users").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
