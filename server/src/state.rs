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
    pub auth_http_client: Client,
    pub transfer_http_client: Client,
}

impl AppState {
    pub fn new(
        settings: Settings,
        db: PgPool,
        auth_http_client: Client,
        transfer_http_client: Client,
    ) -> Self {
        Self {
            settings: Arc::new(settings),
            db,
            auth_http_client,
            transfer_http_client,
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

pub fn build_auth_http_client() -> AppResult<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| AppError::config(format!("failed to build http client: {error}")))
}

pub fn build_transfer_http_client(settings: &Settings) -> AppResult<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(
            settings.config.transfer.connect_timeout_seconds,
        ))
        .read_timeout(std::time::Duration::from_secs(
            settings.config.transfer.read_timeout_seconds,
        ))
        .build()
        .map_err(|error| AppError::config(format!("failed to build transfer http client: {error}")))
}
