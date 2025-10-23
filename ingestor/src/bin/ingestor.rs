use std::{convert::TryFrom, env, sync::Arc};

use anyhow::{Context, Result};
use clap::Parser;
use ingestor::{
    checkpoint::Checkpoint,
    cli::Args,
    limits,
    mempool::MempoolWatcher,
    pipeline::{self, PipelineCfg},
    rpc::{MoneroRpc, Rpc},
    store::Store,
    work_block, work_persist, work_sched, work_tx,
};
use tokio::sync::Mutex;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ingestor=info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    if env::var("INGEST_CONCURRENCY").is_err() {
        if let Ok(val) = env::var("CONCURRENCY") {
            env::set_var("INGEST_CONCURRENCY", val);
        }
    }

    let args = Args::parse();

    let limiter = Arc::new(limits::make_limiter(args.rpc_rps, args.bootstrap));
    let conc = limits::eff_concurrency(args.ingest_concurrency, args.bootstrap);
    let block_workers = conc.max(1).min(4);
    let tx_workers = conc.max(1);
    let do_analytics = !args.bootstrap;

    info!("connecting to database");
    let store = Store::connect(&args.database_url)
        .await
        .context("failed to connect to postgres")?;
    let checkpoint = Arc::new(Checkpoint::new(store.pool().clone()));
    let rpc: Arc<dyn MoneroRpc> = Arc::new(Rpc::new(&args.rpc_url));

    MempoolWatcher::new(&args.zmq_url, Arc::clone(&rpc), store.clone()).spawn();

    let start_height = match args.start_height {
        Some(start) => Some(i64::try_from(start).context("start height overflow")?),
        None => None,
    };

    let pipeline_cfg = PipelineCfg {
        sched_buffer: 512,
        block_workers,
        tx_workers,
    };
    let (tx_sched, rx_sched, tx_block, rx_block, tx_tx, rx_tx) =
        pipeline::make_channels(&pipeline_cfg);

    let sched_cfg = work_sched::Config {
        checkpoint: checkpoint.clone(),
        rpc: Arc::clone(&rpc),
        limiter: limiter.clone(),
        start_height,
        limit: args.limit,
        finality_window: args.finality_window,
    };

    let scheduler = tokio::spawn(async move { work_sched::run(tx_sched, sched_cfg, None).await });

    let rx_sched = Arc::new(Mutex::new(rx_sched));
    let block_cfg = work_block::Config {
        rpc: Arc::clone(&rpc),
        limiter: limiter.clone(),
        store: store.clone(),
        finality_window: args.finality_window,
    };
    let mut block_handles = Vec::with_capacity(block_workers);
    for _ in 0..block_workers {
        let rx = rx_sched.clone();
        let tx = tx_block.clone();
        let cfg = block_cfg.clone();
        block_handles.push(tokio::spawn(async move {
            work_block::run(rx, tx, cfg, None).await
        }));
    }
    drop(tx_block);

    let rx_block = Arc::new(Mutex::new(rx_block));
    let tx_cfg = work_tx::Config {
        rpc: Arc::clone(&rpc),
        limiter: limiter.clone(),
        concurrency: conc,
    };
    let mut tx_handles = Vec::with_capacity(tx_workers);
    for _ in 0..tx_workers {
        let rx = rx_block.clone();
        let tx = tx_tx.clone();
        let cfg = tx_cfg.clone();
        tx_handles.push(tokio::spawn(async move {
            work_tx::run(rx, tx, cfg, None).await
        }));
    }
    drop(tx_tx);

    let persist_cfg = work_persist::Config {
        store: store.clone(),
        checkpoint: checkpoint.clone(),
        finality_window: args.finality_window,
        do_analytics,
    };
    let persister = tokio::spawn(async move { work_persist::run(rx_tx, persist_cfg, None).await });

    if let Err(err) = scheduler.await? {
        error!(error = ?err, "scheduler exited with error");
        return Err(err);
    }

    drain_handles(block_handles, "block").await?;
    drain_handles(tx_handles, "tx").await?;

    if let Err(err) = persister.await? {
        error!(error = ?err, "persistence exited with error");
        return Err(err);
    }

    info!("backfill complete");
    Ok(())
}

async fn drain_handles(
    handles: Vec<tokio::task::JoinHandle<Result<()>>>,
    label: &str,
) -> Result<()> {
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                error!(target = "ingestor", ?err, worker = label, "worker failed");
                return Err(err);
            }
            Err(join_err) => {
                error!(
                    target = "ingestor",
                    ?join_err,
                    worker = label,
                    "worker panicked"
                );
                return Err(join_err.into());
            }
        }
    }
    Ok(())
}
