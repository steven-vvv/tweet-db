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
    pub search: SearchSection,
    pub observability: ObservabilitySection,
}

impl AppConfig {
    fn load(path: Option<&Path>) -> AppResult<Self> {
        let path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(discover_default_config_path);
        let raw = fs::read_to_string(&path)?;
        let parsed: RawConfig = toml::from_str(&raw)
            .map_err(|error| AppError::config(format!("invalid config file: {error}")))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));

        let config = Self {
            app: parsed.app,
            server: ServerSection {
                mode: parsed.server.mode,
                listen_addr: parsed.server.listen_addr.parse().map_err(|error| {
                    AppError::config(format!("invalid server.listen_addr: {error}"))
                })?,
                webui_dist_dir: resolve_path(base, &parsed.server.webui_dist_dir),
                tls: parsed.server.tls.map(|tls| ServerTlsSection {
                    certificate_chain_path: resolve_path(base, &tls.certificate_chain_path),
                    private_key_path: resolve_path(base, &tls.private_key_path),
                }),
            },
            session: parsed.session,
            sso: parsed.sso,
            ingest: parsed.ingest,
            storage: parsed.storage,
            transfer: parsed.transfer,
            search: SearchSection {
                enabled: parsed.search.enabled,
                index_dir: resolve_path(base, &parsed.search.index_dir),
                worker_count: parsed.search.worker_count,
                queue_batch_size: parsed.search.queue_batch_size,
                writer_memory_mb: parsed.search.writer_memory_mb,
                commit_interval_seconds: parsed.search.commit_interval_seconds,
                stale_timeout_seconds: parsed.search.stale_timeout_seconds,
                max_attempts: parsed.search.max_attempts,
            },
            observability: parsed.observability,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> AppResult<()> {
        if self.ingest.max_items_per_batch == 0 {
            return Err(AppError::config(
                "ingest.max_items_per_batch must be greater than 0",
            ));
        }

        if self.ingest.stats_sample_interval_seconds <= 0 {
            return Err(AppError::config(
                "ingest.stats_sample_interval_seconds must be greater than 0",
            ));
        }

        if self.transfer.enabled {
            if self.transfer.chunk_size_mb < 5 {
                return Err(AppError::config(
                    "transfer.chunk_size_mb must be at least 5 when transfer.enabled = true",
                ));
            }

            if self.transfer.download_parallelism == 0 {
                return Err(AppError::config(
                    "transfer.download_parallelism must be greater than 0 when transfer.enabled = true",
                ));
            }

            if self.transfer.upload_parallelism == 0 {
                return Err(AppError::config(
                    "transfer.upload_parallelism must be greater than 0 when transfer.enabled = true",
                ));
            }

            if self.transfer.max_in_flight_parts == 0 {
                return Err(AppError::config(
                    "transfer.max_in_flight_parts must be greater than 0 when transfer.enabled = true",
                ));
            }

            if self.transfer.worker_poll_interval_seconds == 0 {
                return Err(AppError::config(
                    "transfer.worker_poll_interval_seconds must be greater than 0 when transfer.enabled = true",
                ));
            }

            if self.transfer.task_stale_timeout_seconds == 0 {
                return Err(AppError::config(
                    "transfer.task_stale_timeout_seconds must be greater than 0 when transfer.enabled = true",
                ));
            }

            if self.transfer.max_attempts <= 0 {
                return Err(AppError::config(
                    "transfer.max_attempts must be greater than 0 when transfer.enabled = true",
                ));
            }
        }

        if self.search.enabled {
            if self.search.worker_count == 0 {
                return Err(AppError::config(
                    "search.worker_count must be greater than 0 when search.enabled = true",
                ));
            }

            if self.search.queue_batch_size == 0 {
                return Err(AppError::config(
                    "search.queue_batch_size must be greater than 0 when search.enabled = true",
                ));
            }

            if self.search.writer_memory_mb == 0 {
                return Err(AppError::config(
                    "search.writer_memory_mb must be greater than 0 when search.enabled = true",
                ));
            }

            if self.search.commit_interval_seconds == 0 {
                return Err(AppError::config(
                    "search.commit_interval_seconds must be greater than 0 when search.enabled = true",
                ));
            }

            if self.search.stale_timeout_seconds == 0 {
                return Err(AppError::config(
                    "search.stale_timeout_seconds must be greater than 0 when search.enabled = true",
                ));
            }

            if self.search.max_attempts <= 0 {
                return Err(AppError::config(
                    "search.max_attempts must be greater than 0 when search.enabled = true",
                ));
            }
        }

        if self.server.mode != ServerMode::Https {
            return Ok(());
        }

        let tls = self.server.require_tls()?;
        validate_url_scheme("app.base_url", &self.app.base_url, "https")?;
        validate_url_scheme(
            "sso.login_redirect_uri",
            &self.sso.login_redirect_uri,
            "https",
        )?;

        if !self.session.cookie_secure {
            return Err(AppError::config(
                "session.cookie_secure must be true when server.mode = \"https\"",
            ));
        }

        ensure_file_declared(
            "server.tls.certificate_chain_path",
            &tls.certificate_chain_path,
        )?;
        ensure_file_declared("server.tls.private_key_path", &tls.private_key_path)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppSection {
    pub environment: String,
    pub base_url: String,
}

#[derive(Debug, Clone)]
pub struct ServerSection {
    pub mode: ServerMode,
    pub listen_addr: SocketAddr,
    pub webui_dist_dir: PathBuf,
    pub tls: Option<ServerTlsSection>,
}

impl ServerSection {
    pub fn require_tls(&self) -> AppResult<&ServerTlsSection> {
        self.tls.as_ref().ok_or_else(|| {
            AppError::config("server.tls must be configured when server.mode = \"https\"")
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServerMode {
    #[default]
    Http,
    Https,
}

impl ServerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerTlsSection {
    pub certificate_chain_path: PathBuf,
    pub private_key_path: PathBuf,
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
    pub stats_sample_interval_seconds: i64,
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
    #[serde(default = "default_task_stale_timeout_seconds")]
    pub task_stale_timeout_seconds: u64,
    pub worker_poll_interval_seconds: u64,
    pub max_attempts: i32,
}

fn default_task_stale_timeout_seconds() -> u64 {
    900
}

#[derive(Debug, Clone)]
pub struct SearchSection {
    pub enabled: bool,
    pub index_dir: PathBuf,
    pub worker_count: usize,
    pub queue_batch_size: usize,
    pub writer_memory_mb: usize,
    pub commit_interval_seconds: u64,
    pub stale_timeout_seconds: u64,
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
    #[serde(default)]
    search: RawSearchSection,
    observability: ObservabilitySection,
}

#[derive(Debug, Deserialize)]
struct RawServerSection {
    #[serde(default)]
    mode: ServerMode,
    listen_addr: String,
    webui_dist_dir: String,
    tls: Option<RawServerTlsSection>,
}

#[derive(Debug, Deserialize)]
struct RawServerTlsSection {
    certificate_chain_path: String,
    private_key_path: String,
}

#[derive(Debug, Deserialize)]
struct RawSearchSection {
    #[serde(default = "default_search_enabled")]
    enabled: bool,
    #[serde(default = "default_search_index_dir")]
    index_dir: String,
    #[serde(default = "default_search_worker_count")]
    worker_count: usize,
    #[serde(default = "default_search_queue_batch_size")]
    queue_batch_size: usize,
    #[serde(default = "default_search_writer_memory_mb")]
    writer_memory_mb: usize,
    #[serde(default = "default_search_commit_interval_seconds")]
    commit_interval_seconds: u64,
    #[serde(default = "default_search_stale_timeout_seconds")]
    stale_timeout_seconds: u64,
    #[serde(default = "default_search_max_attempts")]
    max_attempts: i32,
}

impl Default for RawSearchSection {
    fn default() -> Self {
        Self {
            enabled: default_search_enabled(),
            index_dir: default_search_index_dir(),
            worker_count: default_search_worker_count(),
            queue_batch_size: default_search_queue_batch_size(),
            writer_memory_mb: default_search_writer_memory_mb(),
            commit_interval_seconds: default_search_commit_interval_seconds(),
            stale_timeout_seconds: default_search_stale_timeout_seconds(),
            max_attempts: default_search_max_attempts(),
        }
    }
}

fn default_search_enabled() -> bool {
    true
}

fn default_search_index_dir() -> String {
    "var/search".to_owned()
}

fn default_search_worker_count() -> usize {
    1
}

fn default_search_queue_batch_size() -> usize {
    200
}

fn default_search_writer_memory_mb() -> usize {
    128
}

fn default_search_commit_interval_seconds() -> u64 {
    5
}

fn default_search_stale_timeout_seconds() -> u64 {
    300
}

fn default_search_max_attempts() -> i32 {
    8
}

fn decode_key(name: &str, value: &str) -> AppResult<[u8; 32]> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|error| AppError::config(format!("invalid {name}: {error}")))?;
    decoded
        .try_into()
        .map_err(|_| AppError::config(format!("{name} must decode to exactly 32 bytes")))
}

mod paths;
mod validation;

use self::{paths::*, validation::*};

#[cfg(test)]
mod tests;
