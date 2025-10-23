use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use ingestor::{
    checkpoint::Checkpoint,
    limits,
    pipeline::{self, PipelineCfg},
    rpc::{
        BlockHeader, GetBlockCountResult, GetBlockHeaderByHeightResult, GetBlockResult,
        GetTransactionsResult, MoneroRpc,
    },
    store::Store,
    work_block, work_persist, work_sched, work_tx,
};
use sqlx::{migrate::Migrator, PgPool};
use tokio::sync::Mutex;

static MIGRATOR: Migrator = sqlx::migrate!("../db/migrations");

const BLOCK_COUNT: u64 = 6;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pipeline_persists_in_order() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping pipeline_persists_in_order: DATABASE_URL not set");
            return Ok(());
        }
    };

    let pool = match PgPool::connect(&database_url).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("skipping pipeline_persists_in_order: failed to connect: {err}");
            return Ok(());
        }
    };
    if let Err(err) = MIGRATOR.run(&pool).await {
        eprintln!("skipping pipeline_persists_in_order: migrations failed: {err}");
        return Ok(());
    }

    let mut cleanup = pool.begin().await?;
    sqlx::query("DELETE FROM public.chain_tips")
        .execute(&mut *cleanup)
        .await?;
    sqlx::query("DELETE FROM public.blocks")
        .execute(&mut *cleanup)
        .await?;
    sqlx::query("DELETE FROM public.txs")
        .execute(&mut *cleanup)
        .await?;
    sqlx::query("DELETE FROM public.ingestor_checkpoint")
        .execute(&mut *cleanup)
        .await?;
    cleanup.commit().await?;

    let store = Store::connect(&database_url)
        .await
        .context("connect store")?;
    let checkpoint = Arc::new(Checkpoint::new(store.pool().clone()));
    let rpc: Arc<dyn MoneroRpc> = Arc::new(MockRpc::new(BLOCK_COUNT));
    let limiter = Arc::new(limits::make_limiter(100, false));

    let pipeline_cfg = PipelineCfg {
        sched_buffer: 8,
        block_workers: 3,
        tx_workers: 2,
    };
    let (tx_sched, rx_sched, tx_block, rx_block, tx_tx, rx_tx) =
        pipeline::make_channels(&pipeline_cfg);

    let sched_cfg = work_sched::Config {
        checkpoint: checkpoint.clone(),
        rpc: Arc::clone(&rpc),
        limiter: limiter.clone(),
        start_height: Some(1),
        limit: Some(BLOCK_COUNT),
        finality_window: 0,
    };
    let scheduler = tokio::spawn(async move { work_sched::run(tx_sched, sched_cfg, None).await });

    let rx_sched = Arc::new(Mutex::new(rx_sched));
    let block_cfg = work_block::Config {
        rpc: Arc::clone(&rpc),
        limiter: limiter.clone(),
        store: store.clone(),
        finality_window: 0,
    };
    let mut block_handles = Vec::with_capacity(pipeline_cfg.block_workers);
    for _ in 0..pipeline_cfg.block_workers {
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
        concurrency: 3,
    };
    let mut tx_handles = Vec::with_capacity(pipeline_cfg.tx_workers);
    for _ in 0..pipeline_cfg.tx_workers {
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
        finality_window: 0,
        do_analytics: false,
    };
    let persister = tokio::spawn(async move { work_persist::run(rx_tx, persist_cfg, None).await });

    if let Err(err) = scheduler.await? {
        panic!("scheduler failed: {:?}", err);
    }

    for handle in block_handles {
        handle.await??;
    }

    for handle in tx_handles {
        handle.await??;
    }

    if let Err(err) = persister.await? {
        panic!("persister failed: {:?}", err);
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM public.blocks")
        .fetch_one(store.pool())
        .await?;
    assert_eq!(count, BLOCK_COUNT as i64);

    let checkpoint_height = checkpoint.get().await?;
    assert_eq!(checkpoint_height, BLOCK_COUNT as i64);

    Ok(())
}

struct MockRpc {
    blocks: Vec<MockBlock>,
}

impl MockRpc {
    fn new(count: u64) -> Self {
        let mut blocks = Vec::with_capacity(count as usize);
        for height in 1..=count {
            let hash = format!("{:064x}", height);
            let prev_hash = format!("{:064x}", height.saturating_sub(1));
            let tx_hashes = vec![
                format!("{:064x}", height * 10 + 1),
                format!("{:064x}", height * 10 + 2),
            ];
            let block_json = serde_json::json!({
                "miner_tx": {
                    "version": 1,
                    "extra": "",
                    "vin": [],
                    "vout": [],
                    "rct_signatures": {},
                    "rctsig_prunable": {},
                    "unlock_time": 0,
                },
                "tx_hashes": tx_hashes,
            })
            .to_string();
            let tx_jsons = tx_hashes
                .iter()
                .map(|hash| {
                    serde_json::json!({
                        "tx_hash": hash,
                        "version": 1,
                        "vin": [],
                        "vout": [],
                        "extra": "",
                        "rct_signatures": {},
                        "rctsig_prunable": {},
                        "unlock_time": 0,
                    })
                    .to_string()
                })
                .collect();

            blocks.push(MockBlock {
                header: BlockHeader {
                    hash: hash.clone(),
                    height,
                    timestamp: height * 100,
                    prev_hash,
                    major_version: 1,
                    minor_version: 1,
                    nonce: 0,
                    reward: 0,
                    size: 1,
                },
                block_json,
                miner_tx_hash: Some(format!("{:064x}", height * 1000)),
                tx_hashes,
                tx_jsons,
            });
        }
        Self { blocks }
    }

    fn jitter(height: u64, salt: u64) -> Duration {
        let millis = ((height * 37 + salt * 17) % 11) + 1;
        Duration::from_millis(millis)
    }

    async fn random_delay(height: u64, salt: u64) {
        tokio::time::sleep(Self::jitter(height, salt)).await;
    }
}

struct MockBlock {
    header: BlockHeader,
    block_json: String,
    miner_tx_hash: Option<String>,
    tx_hashes: Vec<String>,
    tx_jsons: Vec<String>,
}

#[async_trait::async_trait]
impl MoneroRpc for MockRpc {
    async fn get_block_header_by_height(
        &self,
        height: u64,
    ) -> Result<GetBlockHeaderByHeightResult> {
        Self::random_delay(height, 1).await;
        let block = self
            .blocks
            .iter()
            .find(|b| b.header.height == height)
            .context("missing block header")?;
        Ok(GetBlockHeaderByHeightResult {
            block_header: block.header.clone(),
            status: "OK".to_string(),
        })
    }

    async fn get_block(&self, hash: &str, _fill_pow: bool) -> Result<GetBlockResult> {
        let block = self
            .blocks
            .iter()
            .find(|b| b.header.hash == hash)
            .context("missing block")?;
        Self::random_delay(block.header.height, 2).await;
        Ok(GetBlockResult {
            block_header: block.header.clone(),
            json: Some(block.block_json.clone()),
            blob: None,
            miner_tx_hash: block.miner_tx_hash.clone(),
            status: "OK".to_string(),
        })
    }

    async fn get_transactions(&self, txs_hashes: &[String]) -> Result<GetTransactionsResult> {
        let mut jsons = Vec::with_capacity(txs_hashes.len());
        for hash in txs_hashes.iter() {
            let tx = self
                .blocks
                .iter()
                .flat_map(|b| b.tx_hashes.iter().zip(b.tx_jsons.iter()))
                .find(|(h, _)| h.as_str() == hash.as_str())
                .context("missing tx json")?;
            jsons.push(tx.1.clone());
        }
        let height = txs_hashes
            .first()
            .and_then(|hash| self.blocks.iter().find(|b| b.tx_hashes.contains(hash)))
            .map(|b| b.header.height)
            .unwrap_or_default();
        Self::random_delay(height, 3).await;
        Ok(GetTransactionsResult {
            txs_as_json: jsons,
            missed_tx: Vec::new(),
            status: "OK".to_string(),
        })
    }

    async fn get_block_count(&self) -> Result<GetBlockCountResult> {
        Ok(GetBlockCountResult {
            count: BLOCK_COUNT + 1,
            status: "OK".to_string(),
        })
    }

    async fn get_transaction_pool_hashes(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}
