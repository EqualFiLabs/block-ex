mod config;
mod models;
mod routes;
mod state;
mod util;

use std::{iter, time::Duration};

use anyhow::{anyhow, Result};
use axum::{routing::get, Router};
use clap::Parser;
use config::Config;
use sqlx::PgPool;
use state::AppState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer, trace::TraceLayer};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
struct ProbeArgs {
    #[arg(long)]
    url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|v| v.as_str()) == Some("probe") {
        let probe_args = ProbeArgs::parse_from(
            iter::once(String::from("probe")).chain(args.iter().skip(2).cloned()),
        );
        return run_probe(&probe_args.url).await;
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,api=info".into());
    fmt().with_env_filter(filter).init();

    let cfg = Config::parse_from(args);

    let db = PgPool::connect(&cfg.database_url).await?;
    let client = redis::Client::open(cfg.redis_url.clone())?;
    let cache = redis::aio::ConnectionManager::new(client).await?;

    let state = AppState { db, cache };

    let app = Router::new()
        .route("/healthz", get(routes::healthz))
        .merge(routes::v1_router())
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(GlobalConcurrencyLimitLayer::new(1024))
        .layer(TimeoutLayer::new(Duration::from_secs(10)))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!("api listening on {}", cfg.bind);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn run_probe(url: &str) -> Result<()> {
    let uri: http::Uri = url.parse()?;
    if uri.scheme_str() != Some("http") {
        return Err(anyhow!("probe only supports http urls"));
    }
    let host = uri
        .host()
        .ok_or_else(|| anyhow!("probe url missing host"))?;
    let port = uri.port_u16().unwrap_or(80);
    let addr = format!("{host}:{port}");
    let mut stream = tokio::net::TcpStream::connect(addr).await?;
    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;

    let mut buf = Vec::with_capacity(256);
    stream.read_to_end(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf);
    let status_line = response
        .lines()
        .next()
        .ok_or_else(|| anyhow!("invalid probe response"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("invalid status line"))?
        .parse::<u16>()?;
    if (200..400).contains(&status) {
        Ok(())
    } else {
        Err(anyhow!("probe status code {status}"))
    }
}
