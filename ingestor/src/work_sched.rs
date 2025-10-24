use std::{
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use governor::DefaultDirectRateLimiter;
use tokio::{sync::mpsc, time::sleep};
use tracing::{debug, info};

use crate::{
    checkpoint::Checkpoint,
    pipeline::{SchedMsg, Shutdown},
    rpc::{Capabilities, MoneroRpc},
};

pub struct Config {
    pub checkpoint: Arc<Checkpoint>,
    pub rpc: Arc<dyn MoneroRpc>,
    pub limiter: Arc<DefaultDirectRateLimiter>,
    pub start_height: Option<i64>,
    pub limit: Option<u64>,
    pub finality_window: u64,
    pub caps: Capabilities,
    pub header_batch: u64,
}

pub async fn run(
    tx: mpsc::Sender<SchedMsg>,
    cfg: Config,
    _shutdown: Option<Shutdown>,
) -> Result<()> {
    if cfg.caps.headers_range {
        info!(
            batch = cfg.header_batch,
            "scheduler using bulk header queues"
        );
    } else {
        info!("scheduler using single header queues");
    }

    let mut processed_blocks = 0u64;

    let mut next_height = if let Some(start) = cfg.start_height {
        start
    } else {
        cfg.checkpoint.get().await.context("read checkpoint")? + 1
    };
    if next_height < 0 {
        next_height = 0;
    }

    loop {
        if let Some(limit) = cfg.limit {
            if processed_blocks >= limit {
                info!(processed = processed_blocks, "block limit reached");
                break;
            }
        }

        let height_u64 = u64::try_from(next_height).context("height became negative")?;
        let (tip_height_u64, finalized_height_i64) = loop {
            let tip_height_u64 = fetch_chain_tip(cfg.rpc.as_ref(), &cfg.limiter).await?;
            if height_u64 <= tip_height_u64 {
                let finalized_height_u64 = tip_height_u64.saturating_sub(cfg.finality_window);
                let finalized_height_i64 =
                    i64::try_from(finalized_height_u64).context("finalized height overflow")?;
                break (tip_height_u64, finalized_height_i64);
            }
            debug!(
                height = height_u64,
                tip = tip_height_u64,
                "waiting for new blocks"
            );
            sleep(Duration::from_secs(2)).await;
        };

        let tip_height_i64 = i64::try_from(tip_height_u64).context("tip height overflow")?;

        info!(height = height_u64, tip = tip_height_u64, "queueing block");
        if tx
            .send(SchedMsg {
                height: next_height,
                tip_height: tip_height_i64,
                finalized_height: finalized_height_i64,
                started: Instant::now(),
            })
            .await
            .is_err()
        {
            break;
        }

        crate::pipeline::record_queue_depth_sender("sched", &tx);

        processed_blocks += 1;
        next_height += 1;

        if processed_blocks % 100 == 0 {
            info!(processed = processed_blocks, "scheduler progress");
        }
    }

    info!(processed = processed_blocks, "scheduler complete");
    Ok(())
}

async fn fetch_chain_tip(
    rpc: &dyn MoneroRpc,
    limiter: &Arc<DefaultDirectRateLimiter>,
) -> Result<u64> {
    limiter.until_ready().await;
    let res = rpc.get_block_count().await.context("get_block_count rpc")?;
    let highest = res.count.saturating_sub(1);
    Ok(highest)
}
