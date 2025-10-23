use std::{str, thread, time::Duration};

use anyhow::{Context, Result};
use tokio::runtime::Handle;
use tracing::{debug, error, info, warn};

use crate::{rpc::Rpc, store::Store};

const RAW_TX: &str = "raw_tx";
const RAW_BLOCK: &str = "raw_block";
const RECEIVE_TIMEOUT_MS: i32 = 5_000;

pub struct MempoolWatcher {
    zmq_addr: String,
    rpc: Rpc,
    store: Store,
}

impl MempoolWatcher {
    pub fn new<S: Into<String>>(zmq_addr: S, rpc: Rpc, store: Store) -> Self {
        Self {
            zmq_addr: zmq_addr.into(),
            rpc,
            store,
        }
    }

    pub fn spawn(self) {
        let handle = Handle::current();
        thread::Builder::new()
            .name("mempool-zmq".into())
            .spawn(move || {
                if let Err(err) = self.run(handle) {
                    error!(error = ?err, "mempool watcher exited");
                }
            })
            .expect("spawn mempool watcher");
    }

    fn run(self, handle: Handle) -> Result<()> {
        let ctx = zmq::Context::new();
        let sub = ctx.socket(zmq::SUB).context("create ZMQ SUB socket")?;
        sub.set_rcvtimeo(RECEIVE_TIMEOUT_MS)?;
        sub.connect(&self.zmq_addr)
            .with_context(|| format!("connect zmq {}", self.zmq_addr))?;
        sub.set_subscribe(RAW_TX.as_bytes())?;
        sub.set_subscribe(RAW_BLOCK.as_bytes())?;

        info!(addr = %self.zmq_addr, "subscribed to mempool topics");

        if let Err(err) = handle.block_on(self.refresh_from_pool()) {
            warn!(error = ?err, "initial mempool refresh failed");
        }

        loop {
            match sub.recv_multipart(0) {
                Ok(frames) => {
                    let topic = frames
                        .get(0)
                        .and_then(|frame| str::from_utf8(frame).ok())
                        .unwrap_or("");

                    if matches!(topic, RAW_TX | RAW_BLOCK) {
                        debug!(%topic, "refreshing mempool");
                        if let Err(err) = handle.block_on(self.refresh_from_pool()) {
                            warn!(topic = %topic, error = ?err, "mempool refresh failed");
                        }
                    } else {
                        debug!(%topic, "ignored zmq topic");
                    }
                }
                Err(err) => {
                    if err == zmq::Error::EAGAIN {
                        if let Err(err) = handle.block_on(self.refresh_from_pool()) {
                            debug!(error = ?err, "periodic mempool refresh failed");
                        }
                        continue;
                    }

                    warn!(error = ?err, "zmq receive error");
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }

    async fn refresh_from_pool(&self) -> Result<()> {
        let hashes = self
            .rpc
            .get_transaction_pool_hashes()
            .await
            .context("get_transaction_pool_hashes")?;

        if hashes.is_empty() {
            return Ok(());
        }

        let mut tx = self.store.pool().begin().await?;
        for hash in hashes {
            sqlx::query(
                r#"
INSERT INTO public.mempool_txs (tx_hash)
VALUES (decode($1, 'hex'))
ON CONFLICT (tx_hash) DO UPDATE SET last_seen = NOW()
"#,
            )
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        Ok(())
    }
}
