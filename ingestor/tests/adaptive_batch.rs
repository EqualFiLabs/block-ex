use std::{collections::HashMap, num::NonZeroU32, sync::Mutex};

use anyhow::Result;
use governor::{Quota, RateLimiter};
use ingestor::fetch::fetch_txs_adaptive;
use ingestor::rpc::{
    BlockHeader, Capabilities, GetBlockCountResult, GetBlockHeaderByHeightResult, GetBlockResult,
    GetTransactionsResult, MoneroRpc,
};
use serde_json::json;

struct AdaptiveMockRpc {
    txs: HashMap<String, String>,
    calls: Mutex<Vec<usize>>,
}

impl AdaptiveMockRpc {
    fn new(hashes: &[String]) -> Self {
        let txs = hashes
            .iter()
            .map(|hash| {
                (
                    hash.clone(),
                    json!({
                        "hash": hash,
                        "payload": format!("data-{hash}"),
                    })
                    .to_string(),
                )
            })
            .collect();
        Self {
            txs,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<usize> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait::async_trait]
impl MoneroRpc for AdaptiveMockRpc {
    async fn get_block_header_by_height(
        &self,
        _height: u64,
    ) -> Result<GetBlockHeaderByHeightResult> {
        unimplemented!()
    }

    async fn get_block(&self, _hash: &str, _fill_pow: bool) -> Result<GetBlockResult> {
        unimplemented!()
    }

    async fn get_transactions(&self, txs_hashes: &[String]) -> Result<GetTransactionsResult> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(txs_hashes.len());
        if txs_hashes.len() > 100 {
            return Ok(GetTransactionsResult {
                txs_as_json: Vec::new(),
                missed_tx: txs_hashes.to_vec(),
                status: "OK".to_string(),
            });
        }

        let jsons = txs_hashes
            .iter()
            .map(|hash| self.txs.get(hash).unwrap().clone())
            .collect();
        Ok(GetTransactionsResult {
            txs_as_json: jsons,
            missed_tx: Vec::new(),
            status: "OK".to_string(),
        })
    }

    async fn get_block_count(&self) -> Result<GetBlockCountResult> {
        unimplemented!()
    }

    async fn get_transaction_pool_hashes(&self) -> Result<Vec<String>> {
        unimplemented!()
    }

    async fn get_block_headers_range(&self, _start: u64, _end: u64) -> Result<Vec<BlockHeader>> {
        unimplemented!()
    }

    async fn probe_caps(&self) -> Capabilities {
        Capabilities::default()
    }
}

#[tokio::test]
async fn adaptive_batch_retries_until_success() {
    let hashes: Vec<String> = (0..250).map(|i| format!("hash-{i}")).collect();
    let rpc = AdaptiveMockRpc::new(&hashes);
    let limiter = RateLimiter::direct(Quota::per_second(
        NonZeroU32::new(1_000).expect("quota denominator must be non-zero"),
    ));

    let txs = fetch_txs_adaptive(&rpc, &hashes, 300, &limiter)
        .await
        .expect("adaptive fetch succeeds");

    assert_eq!(txs.len(), hashes.len());
    for (json, hash) in txs.iter().zip(hashes.iter()) {
        assert!(json.contains(hash), "transaction json should include hash");
    }

    let calls = rpc.calls();
    assert!(calls.iter().any(|&len| len > 100));
    assert!(calls.iter().any(|&len| len <= 100));
    assert!(calls.last().copied().unwrap_or_default() <= 100);
}
