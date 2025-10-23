use std::{convert::TryFrom, fmt, sync::Arc};

use anyhow::{anyhow, Context, Result};
use governor::DefaultDirectRateLimiter;
use hex::FromHex;
use tokio::sync::{mpsc, Mutex};
use tracing::warn;

use crate::{
    pipeline::{BlockMsg, SchedMsg, Shutdown},
    reorg::heal_reorg,
    rpc::{BlockHeader, MoneroRpc},
    store::Store,
};

#[derive(Clone)]
pub struct Config {
    pub rpc: Arc<dyn MoneroRpc>,
    pub limiter: Arc<DefaultDirectRateLimiter>,
    pub store: Store,
    pub finality_window: u64,
}

pub async fn run(
    rx: Arc<Mutex<mpsc::Receiver<SchedMsg>>>,
    tx: mpsc::Sender<BlockMsg>,
    cfg: Config,
    _shutdown: Option<Shutdown>,
) -> Result<()> {
    loop {
        let job = {
            let mut guard = rx.lock().await;
            guard.recv().await
        };
        let Some(job) = job else {
            break;
        };

        let current = job;
        let block = loop {
            match process_height(&cfg, &current).await {
                Ok(block) => break block,
                Err(err) => {
                    if err.downcast_ref::<ReorgDetected>().is_some() {
                        continue;
                    }
                    return Err(err);
                }
            }
        };

        if tx.send(block).await.is_err() {
            break;
        }
    }

    Ok(())
}

#[derive(Debug)]
struct ReorgDetected;

impl fmt::Display for ReorgDetected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "reorg detected")
    }
}

impl std::error::Error for ReorgDetected {}

async fn process_height(cfg: &Config, msg: &SchedMsg) -> Result<BlockMsg> {
    let height_u64 = u64::try_from(msg.height).context("height became negative")?;
    let header = fetch_block_header(cfg.rpc.as_ref(), &cfg.limiter, height_u64).await?;

    let prev_hex = header.prev_hash.clone();
    let prev_bytes = <[u8; 32]>::from_hex(&prev_hex).unwrap_or([0u8; 32]);
    if let Some(expected_prev) = cfg
        .store
        .block_hash_at((header.height as i64) - 1)
        .await
        .context("fetch previous hash")?
    {
        if expected_prev.as_slice() != &prev_bytes {
            warn!(
                height = header.height,
                "REORG DETECTED at height {}: header.prev != stored hash(h-1)", header.height
            );
            let finality_window = i64::try_from(cfg.finality_window).unwrap_or(i64::MAX);
            heal_reorg(
                header.height as i64,
                &cfg.store,
                cfg.rpc.as_ref(),
                finality_window,
            )
            .await?;
            return Err(ReorgDetected.into());
        }
    }

    let (block_json, miner_tx_hash) =
        fetch_block_json(cfg.rpc.as_ref(), &cfg.limiter, &header).await?;
    let block_value: serde_json::Value =
        serde_json::from_str(&block_json).context("parse block json")?;

    let miner_tx_json = block_value
        .get("miner_tx")
        .cloned()
        .map(|v| serde_json::to_string(&v))
        .transpose()
        .context("serialize miner tx")?;

    let mut tx_hashes = extract_tx_hashes(&block_value);
    tx_hashes.retain(|h| !h.is_empty());

    let ts = i64::try_from(header.timestamp).context("timestamp overflow")?;

    Ok(BlockMsg {
        height: msg.height,
        hash: header.hash.clone(),
        tx_hashes,
        ts,
        tip_height: msg.tip_height,
        finalized_height: msg.finalized_height,
        header,
        miner_tx_json,
        miner_tx_hash,
    })
}

async fn fetch_block_header(
    rpc: &dyn MoneroRpc,
    limiter: &Arc<DefaultDirectRateLimiter>,
    height: u64,
) -> Result<BlockHeader> {
    limiter.until_ready().await;
    let res = rpc
        .get_block_header_by_height(height)
        .await
        .context("fetch header")?;
    Ok(res.block_header)
}

async fn fetch_block_json(
    rpc: &dyn MoneroRpc,
    limiter: &Arc<DefaultDirectRateLimiter>,
    header: &BlockHeader,
) -> Result<(String, Option<String>)> {
    limiter.until_ready().await;
    let blk = rpc
        .get_block(&header.hash, false)
        .await
        .with_context(|| format!("fetch block {}", header.hash))?;
    let miner_tx_hash = blk.miner_tx_hash.clone();
    let json = blk
        .json
        .ok_or_else(|| anyhow!("block json missing for height {}", header.height))?;
    Ok((json, miner_tx_hash))
}

fn extract_tx_hashes(block: &serde_json::Value) -> Vec<String> {
    block
        .get("tx_hashes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}
