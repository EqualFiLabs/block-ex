use std::{collections::VecDeque, convert::TryFrom, fmt, sync::Arc};

use anyhow::{anyhow, Context, Result};
use governor::DefaultDirectRateLimiter;
use hex::FromHex;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};

use crate::{
    pipeline::{BlockMsg, SchedMsg, Shutdown},
    reorg::heal_reorg,
    rpc::{BlockHeader, Capabilities, MoneroRpc},
    store::Store,
};

#[derive(Clone)]
pub struct Config {
    pub rpc: Arc<dyn MoneroRpc>,
    pub limiter: Arc<DefaultDirectRateLimiter>,
    pub store: Store,
    pub finality_window: u64,
    pub caps: Capabilities,
    pub header_batch: u64,
}

pub async fn run(
    rx: Arc<Mutex<mpsc::Receiver<SchedMsg>>>,
    tx: mpsc::Sender<BlockMsg>,
    cfg: Config,
    _shutdown: Option<Shutdown>,
) -> Result<()> {
    let mut headers = HeaderFetcher::new(
        Arc::clone(&cfg.rpc),
        Arc::clone(&cfg.limiter),
        cfg.caps,
        cfg.header_batch,
    );

    if headers.using_bulk() {
        info!(batch = headers.batch_size(), "using bulk header fetch");
    } else {
        info!("using single header fetch");
    }

    loop {
        let job = {
            let mut guard = rx.lock().await;
            let job = guard.recv().await;
            crate::pipeline::record_queue_depth_receiver("sched", &*guard);
            job
        };
        let Some(job) = job else {
            break;
        };

        let current = job;
        let block = loop {
            match process_height(&cfg, &mut headers, &current).await {
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

        crate::pipeline::record_queue_depth_sender("block", &tx);
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

async fn process_height(
    cfg: &Config,
    headers: &mut HeaderFetcher,
    msg: &SchedMsg,
) -> Result<BlockMsg> {
    let height_u64 = u64::try_from(msg.height).context("height became negative")?;
    let header = headers.fetch(height_u64).await?;

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
        started: msg.started,
    })
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

struct HeaderFetcher {
    rpc: Arc<dyn MoneroRpc>,
    limiter: Arc<DefaultDirectRateLimiter>,
    buffered: VecDeque<BlockHeader>,
    use_range: bool,
    batch_size: u64,
}

impl HeaderFetcher {
    fn new(
        rpc: Arc<dyn MoneroRpc>,
        limiter: Arc<DefaultDirectRateLimiter>,
        caps: Capabilities,
        batch_size: u64,
    ) -> Self {
        Self {
            rpc,
            limiter,
            buffered: VecDeque::new(),
            use_range: caps.headers_range,
            batch_size: batch_size.max(1),
        }
    }

    fn using_bulk(&self) -> bool {
        self.use_range
    }

    fn batch_size(&self) -> u64 {
        self.batch_size
    }

    async fn fetch(&mut self, height: u64) -> Result<BlockHeader> {
        if self.use_range {
            if let Some(header) = self.take_buffered(height) {
                return Ok(header);
            }

            match self.fill_batch(height).await {
                Ok(_) => {
                    if let Some(header) = self.take_buffered(height) {
                        return Ok(header);
                    }
                    warn!(
                        height,
                        "bulk header fetch missing requested height, falling back"
                    );
                }
                Err(err) => {
                    warn!(
                        error = ?err,
                        start_height = height,
                        "bulk header fetch failed, falling back"
                    );
                }
            }

            self.use_range = false;
            self.buffered.clear();
        }

        self.fetch_single(height).await
    }

    async fn fill_batch(&mut self, start: u64) -> Result<()> {
        let end = start.saturating_add(self.batch_size.saturating_sub(1));
        self.limiter.until_ready().await;
        let headers = self
            .rpc
            .get_block_headers_range(start, end)
            .await
            .context("fetch header range")?;
        self.buffered = headers.into();
        Ok(())
    }

    async fn fetch_single(&self, height: u64) -> Result<BlockHeader> {
        self.limiter.until_ready().await;
        let res = self
            .rpc
            .get_block_header_by_height(height)
            .await
            .context("fetch header")?;
        Ok(res.block_header)
    }

    fn take_buffered(&mut self, height: u64) -> Option<BlockHeader> {
        while let Some(front) = self.buffered.front() {
            if front.height < height {
                self.buffered.pop_front();
            } else {
                break;
            }
        }

        if self
            .buffered
            .front()
            .map(|hdr| hdr.height == height)
            .unwrap_or(false)
        {
            return self.buffered.pop_front();
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits;
    use axum::{extract::State, response::Json, routing::post, Router};
    use serde::Deserialize;
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::task::JoinHandle;

    #[derive(Clone)]
    struct ServerState {
        range_calls: Arc<AtomicUsize>,
        single_calls: Arc<AtomicUsize>,
        fail_range: bool,
    }

    #[derive(Deserialize)]
    struct RpcRequest {
        id: Option<u64>,
        method: String,
        params: Value,
    }

    async fn spawn_server(fail_range: bool) -> (String, Arc<ServerState>, JoinHandle<()>) {
        let state = Arc::new(ServerState {
            range_calls: Arc::new(AtomicUsize::new(0)),
            single_calls: Arc::new(AtomicUsize::new(0)),
            fail_range,
        });

        let app_state = state.clone();
        let app = Router::new()
            .route(
                "/json_rpc",
                post(|State(state): State<Arc<ServerState>>, Json(req): Json<RpcRequest>| async move {
                    let id = req.id.unwrap_or(0);
                    let response = match req.method.as_str() {
                        "get_block_headers_range" => {
                            state.range_calls.fetch_add(1, Ordering::SeqCst);
                            if state.fail_range {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {"code": -1, "message": "range disabled"},
                                })
                            } else {
                                let start = req
                                    .params
                                    .get("start_height")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0);
                                let end = req
                                    .params
                                    .get("end_height")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(start);
                                let headers: Vec<Value> = (start..=end)
                                    .map(|h| header_json(h))
                                    .collect();
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {"status": "OK", "headers": headers},
                                })
                            }
                        }
                        "get_block_header_by_height" => {
                            state.single_calls.fetch_add(1, Ordering::SeqCst);
                            let height = req
                                .params
                                .get("height")
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {"status": "OK", "block_header": header_json(height)},
                            })
                        }
                        _ => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {"code": -32601, "message": "unknown method"},
                        }),
                    };

                    Json(response)
                }),
            )
            .with_state(app_state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("listener addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{}", addr), state, handle)
    }

    fn header_json(height: u64) -> Value {
        json!({
            "hash": format!("{:064x}", height + 1),
            "height": height,
            "timestamp": height * 60,
            "prev_hash": format!("{:064x}", height.saturating_sub(1)),
            "major_version": 1,
            "minor_version": 1,
            "nonce": 0,
            "reward": 0,
            "block_size": 1,
        })
    }

    #[tokio::test]
    async fn header_fetcher_uses_range_when_available() {
        let (base, state, handle) = spawn_server(false).await;
        let rpc: Arc<dyn MoneroRpc> = Arc::new(crate::rpc::Rpc::new(format!("{}/json_rpc", base)));
        let limiter = Arc::new(limits::make_limiter(100, false));
        let mut fetcher = HeaderFetcher::new(
            rpc,
            limiter,
            Capabilities {
                headers_range: true,
                blocks_by_height_bin: false,
            },
            3,
        );

        let h0 = fetcher.fetch(0).await.expect("fetch height 0");
        assert_eq!(h0.height, 0);
        let h1 = fetcher.fetch(1).await.expect("fetch height 1");
        assert_eq!(h1.height, 1);

        assert_eq!(state.range_calls.load(Ordering::SeqCst), 1);
        assert_eq!(state.single_calls.load(Ordering::SeqCst), 0);

        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn header_fetcher_falls_back_when_range_fails() {
        let (base, state, handle) = spawn_server(true).await;
        let rpc: Arc<dyn MoneroRpc> = Arc::new(crate::rpc::Rpc::new(format!("{}/json_rpc", base)));
        let limiter = Arc::new(limits::make_limiter(100, false));
        let mut fetcher = HeaderFetcher::new(
            rpc,
            limiter,
            Capabilities {
                headers_range: true,
                blocks_by_height_bin: false,
            },
            3,
        );

        let h0 = fetcher.fetch(0).await.expect("fetch height 0");
        assert_eq!(h0.height, 0);
        let h1 = fetcher.fetch(1).await.expect("fetch height 1");
        assert_eq!(h1.height, 1);

        assert!(state.range_calls.load(Ordering::SeqCst) >= 1);
        assert_eq!(state.single_calls.load(Ordering::SeqCst), 2);

        handle.abort();
        let _ = handle.await;
    }
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
