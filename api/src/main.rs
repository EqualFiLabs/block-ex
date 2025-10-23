use std::{net::SocketAddr, process, str::FromStr, time::Duration};

use axum::{routing::get, Router};
use clap::{Parser, Subcommand};
use reqwest::Client;
use tokio::{net::TcpListener, signal};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the HTTP API server
    Serve {
        /// Address to bind the HTTP server to
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
    },
    /// Probe an HTTP endpoint and exit with success if it is healthy
    Probe {
        /// URL to probe
        #[arg(long, default_value = "http://localhost:8081/healthz")]
        url: String,
        /// Request timeout in seconds
        #[arg(long, default_value_t = 3)]
        timeout_secs: u64,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        error!("{err}");
        eprintln!("{err}");
        process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|err| format!("failed to install tracing subscriber: {err}"))?;

    let cli = Cli::parse();
    match cli.command {
        Command::Serve { bind } => serve(bind).await,
        Command::Probe { url, timeout_secs } => probe(url, timeout_secs).await,
    }
}

async fn serve(bind: String) -> Result<(), String> {
    let addr = SocketAddr::from_str(bind.as_str())
        .map_err(|err| format!("invalid bind address '{bind}': {err}"))?;

    let app = Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz));

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|err| format!("failed to bind listener on {addr}: {err}"))?;

    info!("api listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| format!("server error: {err}"))
}

async fn probe(url: String, timeout_secs: u64) -> Result<(), String> {
    let timeout = Duration::from_secs(timeout_secs);
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|err| format!("failed to build http client: {err}"))?;

    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|err| format!("failed to query {url}: {err}"))?;

    if response.status().is_success() {
        info!("probe succeeded for {url} ({})", response.status());
        Ok(())
    } else {
        Err(format!(
            "probe failed for {url}: received status {}",
            response.status()
        ))
    }
}

async fn root() -> &'static str {
    "bex explorer api"
}

async fn healthz() -> &'static str {
    "ok"
}

async fn shutdown_signal() {
    match signal::ctrl_c().await {
        Ok(()) => info!("shutdown signal received"),
        Err(err) => error!("failed to install ctrl-c handler: {err}"),
    }
}
