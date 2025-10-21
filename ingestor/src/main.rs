use std::{process, time::Duration};

use clap::{Parser, Subcommand};
use tokio::{select, signal, time::sleep};
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
    /// Process new blockchain data
    Run,
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
        Command::Run => ingest_loop().await,
    }
}

async fn ingest_loop() -> Result<(), String> {
    info!("ingestor service started");

    loop {
        select! {
            result = signal::ctrl_c() => {
                match result {
                    Ok(()) => {
                        info!("shutdown signal received");
                        break;
                    }
                    Err(err) => return Err(format!("failed to listen for shutdown signal: {err}")),
                }
            }
            _ = sleep(Duration::from_secs(30)) => {
                info!("heartbeat: waiting for blocks");
            }
        }
    }

    info!("ingestor exiting gracefully");
    Ok(())
}
