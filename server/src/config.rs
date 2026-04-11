use std::{
    env, fs,
    net::SocketAddr,
    path::{Component, Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use url::Url;

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

fn discover_default_config_path() -> PathBuf {
    default_config_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("config/default.toml"))
}

fn default_config_candidates() -> Vec<PathBuf> {
    if running_from_server_dir() {
        vec![
            PathBuf::from("config.toml"),
            PathBuf::from("config/default.toml"),
            PathBuf::from("server/config.toml"),
            PathBuf::from("server/config/default.toml"),
        ]
    } else {
        vec![
            PathBuf::from("server/config.toml"),
            PathBuf::from("config.toml"),
            PathBuf::from("server/config/default.toml"),
            PathBuf::from("config/default.toml"),
        ]
    }
}

fn running_from_server_dir() -> bool {
    Path::new("Cargo.toml").is_file() && Path::new("src/main.rs").is_file()
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
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    normalize_path(&resolved)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

fn validate_url_scheme(name: &str, value: &str, expected_scheme: &str) -> AppResult<()> {
    let parsed =
        Url::parse(value).map_err(|error| AppError::config(format!("invalid {name}: {error}")))?;
    if parsed.scheme() != expected_scheme {
        return Err(AppError::config(format!(
            "{name} must use the {expected_scheme} scheme when server.mode = \"https\""
        )));
    }
    Ok(())
}

fn ensure_file_declared(name: &str, path: &Path) -> AppResult<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        AppError::config(format!(
            "{name} could not be accessed at {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::config(format!(
            "{name} must reference a file: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("tweet-db-config-test-{}", Uuid::now_v7()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write(&self, relative: &str, contents: &str) -> PathBuf {
            let path = self.path.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, contents).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn defaults_server_mode_to_http_for_existing_configs() {
        let temp_dir = TempDir::new();
        let config_path = temp_dir.write(
            "config.toml",
            &test_config(
                r#"
listen_addr = "127.0.0.1:3001"
webui_dist_dir = "../../webui/dist"
"#,
                "http://127.0.0.1:3001",
                false,
                "http://127.0.0.1:3001/integrations/sso/callback",
            ),
        );

        let config = AppConfig::load(Some(&config_path)).unwrap();

        assert_eq!(config.server.mode, ServerMode::Http);
        assert!(config.server.tls.is_none());
    }

    #[test]
    fn resolves_https_certificate_paths_relative_to_config_file() {
        let temp_dir = TempDir::new();
        let config_path = temp_dir.write(
            "config/config.toml",
            &test_config(
                r#"
mode = "https"
listen_addr = "127.0.0.1:3443"
webui_dist_dir = "../../webui/dist"

[server.tls]
certificate_chain_path = "../tls/server.pem"
private_key_path = "../tls/server.key"
"#,
                "https://127.0.0.1:3443",
                true,
                "https://127.0.0.1:3443/integrations/sso/callback",
            ),
        );
        temp_dir.write("tls/server.pem", "placeholder");
        temp_dir.write("tls/server.key", "placeholder");

        let config = AppConfig::load(Some(&config_path)).unwrap();
        let tls = config.server.require_tls().unwrap();

        assert_eq!(
            tls.certificate_chain_path,
            temp_dir.path.join("tls/server.pem")
        );
        assert_eq!(tls.private_key_path, temp_dir.path.join("tls/server.key"));
    }

    #[test]
    fn rejects_https_mode_without_secure_cookie() {
        let temp_dir = TempDir::new();
        let config_path = temp_dir.write(
            "config.toml",
            &test_config(
                r#"
mode = "https"
listen_addr = "127.0.0.1:3443"
webui_dist_dir = "../../webui/dist"

[server.tls]
certificate_chain_path = "server.pem"
private_key_path = "server.key"
"#,
                "https://127.0.0.1:3443",
                false,
                "https://127.0.0.1:3443/integrations/sso/callback",
            ),
        );
        temp_dir.write("server.pem", "placeholder");
        temp_dir.write("server.key", "placeholder");

        let error = AppConfig::load(Some(&config_path)).unwrap_err();

        assert!(
            matches!(error, AppError::Config(message) if message.contains("session.cookie_secure"))
        );
    }

    #[test]
    fn rejects_https_mode_with_non_https_base_url() {
        let temp_dir = TempDir::new();
        let config_path = temp_dir.write(
            "config.toml",
            &test_config(
                r#"
mode = "https"
listen_addr = "127.0.0.1:3443"
webui_dist_dir = "../../webui/dist"

[server.tls]
certificate_chain_path = "server.pem"
private_key_path = "server.key"
"#,
                "http://127.0.0.1:3443",
                true,
                "https://127.0.0.1:3443/integrations/sso/callback",
            ),
        );
        temp_dir.write("server.pem", "placeholder");
        temp_dir.write("server.key", "placeholder");

        let error = AppConfig::load(Some(&config_path)).unwrap_err();

        assert!(matches!(error, AppError::Config(message) if message.contains("app.base_url")));
    }

    #[test]
    fn rejects_https_mode_without_tls_files() {
        let temp_dir = TempDir::new();
        let config_path = temp_dir.write(
            "config.toml",
            &test_config(
                r#"
mode = "https"
listen_addr = "127.0.0.1:3443"
webui_dist_dir = "../../webui/dist"

[server.tls]
certificate_chain_path = "missing.pem"
private_key_path = "missing.key"
"#,
                "https://127.0.0.1:3443",
                true,
                "https://127.0.0.1:3443/integrations/sso/callback",
            ),
        );

        let error = AppConfig::load(Some(&config_path)).unwrap_err();

        assert!(
            matches!(error, AppError::Config(message) if message.contains("server.tls.certificate_chain_path"))
        );
    }

    #[test]
    fn rejects_nonpositive_stats_sample_interval() {
        let temp_dir = TempDir::new();
        let config = test_config(
            r#"
listen_addr = "127.0.0.1:3001"
webui_dist_dir = "../../webui/dist"
"#,
            "http://127.0.0.1:3001",
            false,
            "http://127.0.0.1:3001/integrations/sso/callback",
        )
        .replace(
            "stats_sample_interval_seconds = 3600",
            "stats_sample_interval_seconds = 0",
        );
        let config_path = temp_dir.write("config.toml", &config);

        let error = AppConfig::load(Some(&config_path)).unwrap_err();

        assert!(
            matches!(error, AppError::Config(message) if message.contains("ingest.stats_sample_interval_seconds"))
        );
    }

    fn test_config(
        server_body: &str,
        app_base_url: &str,
        cookie_secure: bool,
        login_redirect_uri: &str,
    ) -> String {
        format!(
            r#"[app]
environment = "test"
base_url = "{app_base_url}"

[server]
{server_body}

[session]
cookie_name = "tweet_db_sid"
pending_login_cookie_name = "tweet_db_sso_state"
cookie_secure = {cookie_secure}
ttl_hours = 12
absolute_ttl_hours = 720
auto_renew = true
pending_login_ttl_seconds = 600

[sso]
issuer = "http://127.0.0.1:3000"
client_id = "tweet-db"
login_redirect_uri = "{login_redirect_uri}"
authorization_cache_ttl_seconds = 300

[ingest]
max_items_per_batch = 5000
actor_metrics_min_interval_seconds = 86400
stats_sample_interval_seconds = 3600

[storage]
provider = "s3_compatible"
endpoint = "http://127.0.0.1:9000"
region = "us-east-1"
bucket = "tweet-db"
object_key_prefix = "media"
path_style = true

[transfer]
enabled = false
worker_count = 1
chunk_size_mb = 1
download_parallelism = 1
upload_parallelism = 1
max_in_flight_parts = 1
connect_timeout_seconds = 5
read_timeout_seconds = 30
attempt_timeout_seconds = 300
worker_poll_interval_seconds = 5
max_attempts = 1

[observability]
log_filter = "info"
"#
        )
    }
}
