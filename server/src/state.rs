use std::sync::Arc;

use reqwest::Client;
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::{
    config::Settings,
    error::{AppError, AppResult},
    search::SearchState,
    string_dict::StringDictCache,
};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub db: PgPool,
    pub auth_http_client: Client,
    pub string_dict: StringDictCache,
    pub search: Option<SearchState>,
}

impl AppState {
    pub fn new(
        settings: Settings,
        db: PgPool,
        auth_http_client: Client,
        string_dict: StringDictCache,
    ) -> Self {
        Self {
            settings: Arc::new(settings),
            db,
            auth_http_client,
            string_dict,
            search: None,
        }
    }

    pub fn with_search(mut self, search: Option<SearchState>) -> Self {
        self.search = search;
        self
    }
}

pub async fn connect_db(settings: &Settings) -> AppResult<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(&settings.secrets.database_url)
        .await
        .map_err(Into::into)
}

pub fn build_auth_http_client() -> AppResult<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| AppError::config(format!("failed to build http client: {error}")))
}
