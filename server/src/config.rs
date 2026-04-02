use std::{
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Settings {
    pub config: AppConfig,
    pub secrets: AppSecrets,
}

impl Settings {
    pub fn load(path: Option<&Path>) -> AppResult<Self> {
        let _ = dotenvy::dotenv_override();
        let config = AppConfig::load(path)?;
        let secrets = AppSecrets::load()?;
        Ok(Self { config, secrets })
    }
}

#[derive(Debug, Clone)]
pub struct AppSecrets {
    pub database_url: String,
    pub app_token: String,
    pub session_hmac_key: [u8; 32],
    pub storage_access_key: Option<String>,
    pub storage_secret_key: Option<String>,
}

impl AppSecrets {
    fn load() -> AppResult<Self> {
        let database_url =
            env::var("DATABASE_URL").map_err(|_| AppError::config("DATABASE_URL is required"))?;
        let app_token =
            env::var("APP_TOKEN").map_err(|_| AppError::config("APP_TOKEN is required"))?;
        let session_hmac_key = decode_key(
            "SESSION_HMAC_KEY",
            &env::var("SESSION_HMAC_KEY")
                .map_err(|_| AppError::config("SESSION_HMAC_KEY is required"))?,
        )?;
        let storage_access_key = env::var("STORAGE_ACCESS_KEY").ok();
        let storage_secret_key = env::var("STORAGE_SECRET_KEY").ok();

        Ok(Self {
            database_url,
            app_token,
            session_hmac_key,
            storage_access_key,
            storage_secret_key,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app: AppSection,
    pub server: ServerSection,
    pub session: SessionSection,
    pub sso: SsoSection,
    pub ingest: IngestSection,
    pub storage: StorageSection,
    pub transfer: TransferSection,
    pub observability: ObservabilitySection,
}

impl AppConfig {
    fn load(path: Option<&Path>) -> AppResult<Self> {
        let path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("config/default.toml"));
        let raw = fs::read_to_string(&path)?;
        let parsed: RawConfig = toml::from_str(&raw)
            .map_err(|error| AppError::config(format!("invalid config file: {error}")))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));

        Ok(Self {
            app: parsed.app,
            server: ServerSection {
                listen_addr: parsed.server.listen_addr.parse().map_err(|error| {
                    AppError::config(format!("invalid server.listen_addr: {error}"))
                })?,
                webui_dist_dir: resolve_path(base, &parsed.server.webui_dist_dir),
            },
            session: parsed.session,
            sso: parsed.sso,
            ingest: parsed.ingest,
            storage: parsed.storage,
            transfer: parsed.transfer,
            observability: parsed.observability,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppSection {
    pub environment: String,
    pub base_url: String,
}

#[derive(Debug, Clone)]
pub struct ServerSection {
    pub listen_addr: SocketAddr,
    pub webui_dist_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionSection {
    pub cookie_name: String,
    pub pending_login_cookie_name: String,
    pub cookie_secure: bool,
    pub ttl_hours: i64,
    pub absolute_ttl_hours: i64,
    pub auto_renew: bool,
    pub pending_login_ttl_seconds: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SsoSection {
    pub issuer: String,
    pub client_id: String,
    pub login_redirect_uri: String,
    pub authorization_cache_ttl_seconds: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestSection {
    pub max_items_per_batch: usize,
    pub actor_metrics_min_interval_seconds: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageSection {
    pub provider: String,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub object_key_prefix: String,
    pub path_style: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransferSection {
    pub enabled: bool,
    pub worker_count: usize,
    pub chunk_size_mb: usize,
    pub download_parallelism: usize,
    pub upload_parallelism: usize,
    pub max_in_flight_parts: usize,
    pub connect_timeout_seconds: u64,
    pub read_timeout_seconds: u64,
    pub attempt_timeout_seconds: u64,
    pub worker_poll_interval_seconds: u64,
    pub max_attempts: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilitySection {
    pub log_filter: String,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    app: AppSection,
    server: RawServerSection,
    session: SessionSection,
    sso: SsoSection,
    ingest: IngestSection,
    storage: StorageSection,
    transfer: TransferSection,
    observability: ObservabilitySection,
}

#[derive(Debug, Deserialize)]
struct RawServerSection {
    listen_addr: String,
    webui_dist_dir: String,
}

fn decode_key(name: &str, value: &str) -> AppResult<[u8; 32]> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|error| AppError::config(format!("invalid {name}: {error}")))?;
    decoded
        .try_into()
        .map_err(|_| AppError::config(format!("{name} must decode to exactly 32 bytes")))
}

fn resolve_path(base: &Path, relative: &str) -> PathBuf {
    let path = Path::new(relative);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}
