use crate::rpc::MoneroRpc;

pub async fn fetch_txs_adaptive(
    rpc: &(impl MoneroRpc + ?Sized),
    hashes: &[String],
    start_chunk: usize,
    limiter: &governor::DefaultDirectRateLimiter,
) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::with_capacity(hashes.len());
    let mut i = 0;
    let mut chunk = start_chunk.max(10);
    while i < hashes.len() {
        limiter.until_ready().await;
        let end = (i + chunk).min(hashes.len());
        let res = rpc.get_transactions(&hashes[i..end]).await?;
        if !res.missed_tx.is_empty() {
            chunk = (chunk / 2).max(10);
            continue;
        }
        out.extend(res.txs_as_json);
        i = end;
        if chunk < 300 {
            chunk += 10;
        }
    }
    Ok(out)
}
