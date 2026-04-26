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

#[test]
fn rejects_too_small_transfer_chunk_when_transfer_is_enabled() {
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
    .replace("enabled = false", "enabled = true");
    let config_path = temp_dir.write("config.toml", &config);

    let error = AppConfig::load(Some(&config_path)).unwrap_err();

    assert!(
        matches!(error, AppError::Config(message) if message.contains("transfer.chunk_size_mb"))
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
task_stale_timeout_seconds = 900
worker_poll_interval_seconds = 5
max_attempts = 1

[observability]
log_filter = "info"
"#
    )
}
