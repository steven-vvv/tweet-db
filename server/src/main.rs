use std::{convert::Infallible, path::PathBuf, sync::Arc};

use axum::{Router, serve};
use clap::Parser;
use hyper::{Request, body::Incoming, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
};
use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tracing_subscriber::{EnvFilter, fmt};

use tweet_db_server::{
    app,
    config::{ServerMode, ServerTlsSection, Settings},
    error::{AppError, AppResult},
};

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
    match settings.config.server.mode {
        ServerMode::Http => serve_http(app, settings.config.server.listen_addr).await?,
        ServerMode::Https => {
            serve_https(
                app,
                settings.config.server.listen_addr,
                settings.config.server.require_tls()?,
            )
            .await?;
        }
    }

    Ok(())
}

async fn serve_http(app: Router, listen_addr: std::net::SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    tracing::info!(mode = ServerMode::Http.as_str(), address = %listen_addr, "listening");
    serve(listener, app).await
}

async fn serve_https(
    app: Router,
    listen_addr: std::net::SocketAddr,
    tls: &ServerTlsSection,
) -> AppResult<()> {
    let tls_acceptor = TlsAcceptor::from(Arc::new(load_rustls_server_config(tls)?));
    let listener = TcpListener::bind(listen_addr).await?;

    tracing::info!(
        mode = ServerMode::Https.as_str(),
        address = %listen_addr,
        certificate_chain = %tls.certificate_chain_path.display(),
        private_key = %tls.private_key_path.display(),
        "listening"
    );

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let tls_acceptor = tls_acceptor.clone();
        let app = app.clone();

        tokio::spawn(async move {
            if let Err(error) = serve_https_connection(app, tls_acceptor, stream).await {
                tracing::warn!(remote_addr = %remote_addr, error = %error, "failed to serve https connection");
            }
        });
    }
}

async fn serve_https_connection(
    app: Router,
    tls_acceptor: TlsAcceptor,
    stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tls_stream = tls_acceptor.accept(stream).await?;
    let io = TokioIo::new(tls_stream);
    let service = service_fn(move |request: Request<Incoming>| {
        let mut app = app.clone();
        async move {
            let response = app
                .call(request)
                .await
                .unwrap_or_else(|err: Infallible| match err {});
            Ok::<_, Infallible>(response)
        }
    });

    let mut builder = Builder::new(TokioExecutor::new());
    builder.http2().enable_connect_protocol();
    builder.serve_connection_with_upgrades(io, service).await?;

    Ok(())
}

fn load_rustls_server_config(tls: &ServerTlsSection) -> AppResult<ServerConfig> {
    let certificates = load_certificate_chain(&tls.certificate_chain_path)?;
    let private_key = load_private_key(&tls.private_key_path)?;

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|error| AppError::config(format!("invalid TLS certificate or key: {error}")))?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(config)
}

fn load_certificate_chain(path: &std::path::Path) -> AppResult<Vec<CertificateDer<'static>>> {
    let certificates = CertificateDer::pem_file_iter(path)
        .map_err(|error| {
            AppError::config(format!(
                "failed to read TLS certificate chain {}: {error}",
                path.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::config(format!(
                "failed to parse TLS certificate chain {}: {error}",
                path.display()
            ))
        })?;

    if certificates.is_empty() {
        return Err(AppError::config(format!(
            "TLS certificate chain file is empty: {}",
            path.display()
        )));
    }

    Ok(certificates)
}

fn load_private_key(path: &std::path::Path) -> AppResult<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path).map_err(|error| {
        AppError::config(format!(
            "failed to read TLS private key {}: {error}",
            path.display()
        ))
    })
}
