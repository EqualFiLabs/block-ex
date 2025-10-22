use ingestor::codec::{analyze_tx, parse_tx_json};
use ingestor::rpc::Rpc;
use std::{env, fs, path::PathBuf};

#[tokio::test]
async fn parse_three_blocks_txs_against_golden() {
    let refresh = env::var("GOLDEN_REFRESH")
        .ok()
        .filter(|v| v == "1")
        .is_some();
    let url = env::var("XMR_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:38081/json_rpc".into());
    let start = env::var("XMR_GOLDEN_START")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000u64);
    let mut all_json = vec![];

    if refresh {
        let rpc = Rpc::new(url);
        for h in start..start + 3 {
            let hdr = rpc.get_block_header_by_height(h).await.expect("header");
            let blk = rpc
                .get_block(&hdr.block_header.hash, false)
                .await
                .expect("block");
            if let Some(json_str) = blk.json {
                let v: serde_json::Value =
                    serde_json::from_str(&json_str).expect("block json decode");
                let arr = v
                    .get("tx_hashes")
                    .and_then(|x| x.as_array())
                    .cloned()
                    .unwrap_or_default();
                let hashes: Vec<String> = arr
                    .iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect();
                if !hashes.is_empty() {
                    let res = rpc
                        .get_transactions(&hashes)
                        .await
                        .expect("get_transactions");
                    for j in res.txs_as_json {
                        all_json.push(j);
                    }
                }
            }
        }
        let out = serde_json::to_string_pretty(&all_json).expect("fixture encode");
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests/fixtures/txs_three_blocks.json");
        fs::create_dir_all(p.parent().unwrap()).expect("mkdir fixtures");
        fs::write(p, out).expect("write fixture");
    }

    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/txs_three_blocks.json");
    let data = fs::read_to_string(p).expect("fixtures missing, run with GOLDEN_REFRESH=1 once");
    let vec: Vec<String> = serde_json::from_str(&data).expect("fixture parse");

    for s in vec {
        let tx = parse_tx_json(&s).expect("tx decode");
        let a = analyze_tx(&tx).expect("tx analyze");
        assert_eq!(a.ring_sizes.len(), a.num_inputs);
        assert!(a.num_outputs > 0);
        assert!(a.bp_plus);
    }
}
