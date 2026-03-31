use std::path::PathBuf;

use axum::serve;
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};

use tweet_db_server::{app, config::Settings};

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let settings = Settings::load(cli.config.as_deref())?;

    fmt()
        .with_env_filter(
            EnvFilter::try_new(&settings.config.observability.log_filter)
                .unwrap_or_else(|_| EnvFilter::new("tweet_db_server=info")),
        )
        .init();

    let app = app::build_app(settings.clone()).await?;
    let listener = TcpListener::bind(settings.config.server.listen_addr).await?;

    tracing::info!("listening on {}", settings.config.server.listen_addr);
    serve(listener, app).await?;

    Ok(())
}
