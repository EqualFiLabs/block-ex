#[tokio::test]
async fn dto_serializes() {
    let b = api::models::BlockView {
        height: 1,
        hash: Some("ab".repeat(32)),
        ts: Some(0),
        size_bytes: 1234,
        major_version: 14,
        minor_version: 14,
        tx_count: 1,
        reward_nanos: 0,
    };

    let j = serde_json::to_string(&b).unwrap();
    assert!(j.contains("\"height\":1"));

    let t = api::models::TxView {
        hash: Some("cd".repeat(32)),
        block_height: Some(1),
        ts: Some(0),
        in_mempool: false,
        fee_nanos: Some(123),
        size_bytes: 2000,
        version: 2,
        unlock_time: 0,
        extra_json: Some("{\"extra\":\"00\"}".into()),
        rct_type: 6,
        proof_type: Some("CLSAG".into()),
        bp_plus: true,
        num_inputs: 2,
        num_outputs: 2,
    };

    let _ = serde_json::to_string(&t).unwrap();
}
