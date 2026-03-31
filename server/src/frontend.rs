use std::path::Path;

use axum::{
    Router,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

pub fn routes(state: &AppState) -> Router<AppState> {
    let dist_dir = state.settings.config.server.webui_dist_dir.clone();
    let index_path = dist_dir.join("index.html");

    Router::new()
        .route("/", get(root_redirect))
        .route("/login", get(spa_index))
        .route("/register", get(spa_index))
        .route("/account", get(spa_index))
        .nest_service("/assets", ServeDir::new(dist_dir.join("assets")))
        .route_service("/favicon.svg", ServeFile::new(dist_dir.join("favicon.svg")))
        .fallback_service(ServeFile::new(index_path))
}

async fn root_redirect() -> Redirect {
    Redirect::temporary("/login")
}

async fn spa_index() -> Response {
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
    .into_response()
}

pub fn dist_exists(state: &AppState) -> bool {
    Path::new(&state.settings.config.server.webui_dist_dir).exists()
}
