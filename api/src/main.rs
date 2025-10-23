mod config;
mod models;
mod routes;
mod state;
mod util;

use std::time::Duration;

use anyhow::Result;
use axum::{routing::get, Router};
use clap::Parser;
use config::Config;
use sqlx::PgPool;
use state::AppState;
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer, trace::TraceLayer};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,api=info".into());
    fmt().with_env_filter(filter).init();

    let cfg = Config::parse();

    let db = PgPool::connect(&cfg.database_url).await?;
    let client = redis::Client::open(cfg.redis_url.clone())?;
    let cache = redis::aio::ConnectionManager::new(client).await?;

    let state = AppState { db, cache };

    let app = Router::new()
        .route("/healthz", get(routes::healthz))
        .merge(routes::v1_router())
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(10)))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!("api listening on {}", cfg.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
