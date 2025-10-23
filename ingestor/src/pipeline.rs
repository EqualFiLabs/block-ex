use tokio::sync::{mpsc, oneshot};

use crate::rpc::BlockHeader;

pub type Shutdown = oneshot::Receiver<()>;

pub struct SchedMsg {
    pub height: i64,
    pub tip_height: i64,
    pub finalized_height: i64,
}

pub struct BlockMsg {
    pub height: i64,
    pub hash: String,
    pub tx_hashes: Vec<String>,
    pub ts: i64,
    pub tip_height: i64,
    pub finalized_height: i64,
    pub header: BlockHeader,
    pub miner_tx_json: Option<String>,
    pub miner_tx_hash: Option<String>,
}

pub struct TxMsg {
    pub height: i64,
    pub block_hash: String,
    pub tx_jsons: Vec<String>,
    pub ts: i64,
    pub tip_height: i64,
    pub finalized_height: i64,
    pub header: BlockHeader,
    pub miner_tx_json: Option<String>,
    pub miner_tx_hash: Option<String>,
    pub ordered_tx_hashes: Vec<String>,
}

pub struct PipelineCfg {
    pub sched_buffer: usize,
    pub block_workers: usize,
    pub tx_workers: usize,
}

pub fn make_channels(
    cfg: &PipelineCfg,
) -> (
    mpsc::Sender<SchedMsg>,
    mpsc::Receiver<SchedMsg>,
    mpsc::Sender<BlockMsg>,
    mpsc::Receiver<BlockMsg>,
    mpsc::Sender<TxMsg>,
    mpsc::Receiver<TxMsg>,
) {
    let (s1, r1) = mpsc::channel(cfg.sched_buffer);
    let (s2, r2) = mpsc::channel(cfg.block_workers * 4);
    let (s3, r3) = mpsc::channel(cfg.tx_workers * 4);
    (s1, r1, s2, r2, s3, r3)
}
