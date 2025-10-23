#![cfg(feature = "integration")]

use ingestor::rpc::{GetBlockHeaderByHeightResult, Rpc};
use serde_json::Value;
use std::env;

#[tokio::test]
async fn can_fetch_header_and_block_and_txs() {
    let url = env::var("XMR_RPC_URL").expect("XMR_RPC_URL must be set for integration tests");
    let rpc = Rpc::new(url);

    let height: u64 = env::var("XMR_TEST_HEIGHT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    let hdr: GetBlockHeaderByHeightResult = rpc
        .get_block_header_by_height(height)
        .await
        .expect("header fetch");
    assert_eq!(hdr.block_header.height, height);
    let hash = hdr.block_header.hash.clone();

    let blk = rpc.get_block(&hash, false).await.expect("block fetch");
    assert_eq!(blk.block_header.hash, hash);

    if let Some(json_str) = blk.json {
        let v: Value = serde_json::from_str(&json_str).expect("block json parse");
        let mut txs = vec![];
        if let Some(arr) = v.get("tx_hashes").and_then(|x| x.as_array()) {
            for t in arr {
                if let Some(s) = t.as_str() {
                    txs.push(s.to_string());
                }
            }
        }
        if !txs.is_empty() {
            let res = rpc.get_transactions(&txs).await.expect("get txs");
            assert_eq!(res.status, "OK");
            assert_eq!(res.txs_as_json.len(), txs.len());
        }
    }
}
