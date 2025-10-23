use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use governor::DefaultDirectRateLimiter;
use tokio::sync::{mpsc, Mutex};

use crate::{
    fetch::fetch_txs_adaptive,
    pipeline::{BlockMsg, Shutdown, TxMsg},
    rpc::MoneroRpc,
};

#[derive(Clone)]
pub struct Config {
    pub rpc: Arc<dyn MoneroRpc>,
    pub limiter: Arc<DefaultDirectRateLimiter>,
    pub concurrency: usize,
}

pub async fn run(
    rx: Arc<Mutex<mpsc::Receiver<BlockMsg>>>,
    tx: mpsc::Sender<TxMsg>,
    cfg: Config,
    _shutdown: Option<Shutdown>,
) -> Result<()> {
    loop {
        let block_job = {
            let mut guard = rx.lock().await;
            guard.recv().await
        };
        let Some(block_job) = block_job else {
            break;
        };

        let pairs = fetch_transactions(
            &cfg.rpc,
            &cfg.limiter,
            &block_job.tx_hashes,
            cfg.concurrency,
        )
        .await?;

        let ordered_hashes: Vec<String> = pairs.iter().map(|(hash, _)| hash.clone()).collect();
        let tx_jsons: Vec<String> = pairs.into_iter().map(|(_, json)| json).collect();

        let msg = TxMsg {
            height: block_job.height,
            block_hash: block_job.hash,
            tx_jsons,
            ts: block_job.ts,
            tip_height: block_job.tip_height,
            finalized_height: block_job.finalized_height,
            header: block_job.header,
            miner_tx_json: block_job.miner_tx_json,
            miner_tx_hash: block_job.miner_tx_hash,
            ordered_tx_hashes: ordered_hashes,
        };

        if tx.send(msg).await.is_err() {
            break;
        }
    }

    Ok(())
}

async fn fetch_transactions(
    rpc: &Arc<dyn MoneroRpc>,
    limiter: &Arc<DefaultDirectRateLimiter>,
    hashes: &[String],
    concurrency: usize,
) -> Result<Vec<(String, String)>> {
    if hashes.is_empty() {
        return Ok(Vec::new());
    }

    let start_chunk = (concurrency.max(1) * 50).clamp(10, 300);
    let tx_jsons = fetch_txs_adaptive(rpc.as_ref(), hashes, start_chunk, limiter.as_ref())
        .await
        .with_context(|| "fetch transactions adaptive")?;

    if tx_jsons.len() != hashes.len() {
        return Err(anyhow!(
            "daemon returned {} txs for {} hashes",
            tx_jsons.len(),
            hashes.len()
        ));
    }

    Ok(hashes.iter().cloned().zip(tx_jsons.into_iter()).collect())
}
