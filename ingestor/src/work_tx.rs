use std::{collections::HashSet, sync::Arc};

use anyhow::{anyhow, Context, Result};
use futures::{stream, StreamExt, TryStreamExt};
use governor::DefaultDirectRateLimiter;
use tokio::sync::{mpsc, Mutex};
use tracing::warn;

use crate::{
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

    let chunked = hashes
        .chunks(100)
        .map(|chunk| chunk.to_vec())
        .collect::<Vec<_>>();

    let rpc_clone = Arc::clone(rpc);
    let limiter_clone = limiter.clone();
    let stream = stream::iter(chunked.into_iter().map(move |chunk| {
        let rpc = Arc::clone(&rpc_clone);
        let limiter = limiter_clone.clone();
        async move {
            limiter.until_ready().await;
            let res = rpc
                .get_transactions(&chunk)
                .await
                .with_context(|| "fetch transactions batch")?;
            if !res.missed_tx.is_empty() {
                warn!(missed = res.missed_tx.len(), "daemon missed transactions");
            }

            let missed: HashSet<String> = res.missed_tx.into_iter().collect();
            let mut json_iter = res.txs_as_json.into_iter();
            let mut paired = Vec::with_capacity(chunk.len().saturating_sub(missed.len()));

            for hash in chunk.into_iter() {
                if missed.contains(&hash) {
                    continue;
                }
                let json = json_iter
                    .next()
                    .ok_or_else(|| anyhow!("daemon returned fewer txs than expected"))?;
                paired.push((hash, json));
            }

            if let Some(extra) = json_iter.next() {
                warn!(
                    extra_len = extra.len(),
                    "daemon returned extra transaction payload",
                );
            }

            Ok::<Vec<(String, String)>, anyhow::Error>(paired)
        }
    }));

    let limit = concurrency.max(1);
    stream
        .buffer_unordered(limit)
        .try_fold(Vec::new(), |mut acc, batch| async move {
            acc.extend(batch);
            Ok(acc)
        })
        .await
}
