use std::sync::Arc;

use reqwest::Client;
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::{
    config::Settings,
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub db: PgPool,
    pub http_client: Client,
}

impl AppState {
    pub fn new(settings: Settings, db: PgPool, http_client: Client) -> Self {
        Self {
            settings: Arc::new(settings),
            db,
            http_client,
        }
    }
}

pub async fn connect_db(settings: &Settings) -> AppResult<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(&settings.secrets.database_url)
        .await
        .map_err(Into::into)
}

pub fn build_http_client() -> AppResult<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| AppError::config(format!("failed to build http client: {error}")))
}
