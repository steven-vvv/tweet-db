use axum::Router;
use tower_http::{
    compression::CompressionLayer, normalize_path::NormalizePathLayer, trace::TraceLayer,
};

use crate::{
    config::Settings,
    error::AppResult,
    frontend, routes, search,
    state::{AppState, build_auth_http_client, connect_db},
    string_dict::StringDictCache,
    transfer,
};

pub async fn build_app(settings: Settings) -> AppResult<Router> {
    let db = connect_db(&settings).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    let auth_http_client = build_auth_http_client()?;
    let string_dict = StringDictCache::load(&db).await?;
    let search_state = search::build_state(&settings)?;
    let state =
        AppState::new(settings, db, auth_http_client, string_dict).with_search(search_state);
    search::start_workers(state.clone())?;
    transfer::start_workers(state.clone())?;

    let app = Router::new()
        .merge(routes::api_routes(&state))
        .merge(frontend::routes(&state))
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(NormalizePathLayer::trim_trailing_slash())
        .layer(TraceLayer::new_for_http());

    Ok(app)
}
