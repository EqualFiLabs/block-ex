use std::{collections::HashSet, convert::TryFrom, env, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use futures::{stream, StreamExt, TryStreamExt};
use governor::DefaultDirectRateLimiter;
use hex::FromHex;
use ingestor::{
    checkpoint::Checkpoint,
    cli::Args,
    codec::{analyze_tx, parse_tx_json},
    limits,
    mempool::MempoolWatcher,
    reorg::heal_reorg,
    rpc::{BlockHeader, Rpc},
    store::Store,
};
use serde_json::Value;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ingestor=info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    if env::var("INGEST_CONCURRENCY").is_err() {
        if let Ok(val) = env::var("CONCURRENCY") {
            env::set_var("INGEST_CONCURRENCY", val);
        }
    }

    let args = Args::parse();

    let limiter = Arc::new(limits::make_limiter(args.rpc_rps, args.bootstrap));
    let conc = limits::eff_concurrency(args.ingest_concurrency, args.bootstrap);

    info!("connecting to database");
    let store = Store::connect(&args.database_url)
        .await
        .context("failed to connect to postgres")?;
    let checkpoint = Checkpoint::new(store.pool().clone());
    let rpc = Rpc::new(&args.rpc_url);

    MempoolWatcher::new(&args.zmq_url, rpc.clone(), store.clone()).spawn();

    let mut next_height = if let Some(start) = args.start_height {
        i64::try_from(start).context("start height overflow")?
    } else {
        checkpoint.get().await.context("read checkpoint")? + 1
    };
    if next_height < 0 {
        next_height = 0;
    }

    let mut processed_blocks = 0u64;

    loop {
        if let Some(limit) = args.limit {
            if processed_blocks >= limit {
                info!(processed = processed_blocks, "block limit reached");
                break;
            }
        }

        let height_u64 = u64::try_from(next_height).context("height became negative")?;
        let tip_height = loop {
            let tip = fetch_chain_tip(&rpc, &limiter).await?;
            if height_u64 <= tip {
                break tip;
            }
            debug!(height = height_u64, tip, "waiting for new blocks");
            sleep(Duration::from_secs(2)).await;
        };
        let tip_height_i64 = i64::try_from(tip_height).context("tip height overflow")?;
        let finalized_height_u64 = tip_height.saturating_sub(args.finality_window);
        let finalized_height_i64 =
            i64::try_from(finalized_height_u64).context("finalized height overflow")?;

        info!(height = height_u64, "processing block");
        let header = fetch_block_header(&rpc, &limiter, height_u64).await?;

        let prev_hex = header.prev_hash.clone();
        let prev_bytes = <[u8; 32]>::from_hex(&prev_hex).unwrap_or([0u8; 32]);
        if let Some(expected_prev) = store
            .block_hash_at((header.height as i64) - 1)
            .await
            .context("fetch previous hash")?
        {
            if expected_prev.as_slice() != &prev_bytes {
                warn!(
                    height = header.height,
                    "REORG DETECTED at height {}: header.prev != stored hash(h-1)", header.height
                );
                let finality_window = i64::try_from(args.finality_window).unwrap_or(i64::MAX);
                heal_reorg(header.height as i64, &store, &rpc, finality_window).await?;
                continue;
            }
        }

        let (block_json, miner_tx_hash) = fetch_block_json(&rpc, &limiter, &header).await?;
        let block_value: Value = serde_json::from_str(&block_json).context("parse block json")?;

        let tx_hashes = extract_tx_hashes(&block_value);
        let tx_json_pairs = fetch_transactions(&rpc, &limiter, &tx_hashes, conc).await?;

        let mut prepared_txs = Vec::with_capacity(tx_json_pairs.len() + 1);

        if let Some(miner_tx_value) = block_value.get("miner_tx") {
            let miner_tx_json =
                serde_json::to_string(miner_tx_value).context("serialize miner tx")?;
            let miner_hash = miner_tx_hash
                .as_deref()
                .or_else(|| block_value.get("miner_tx_hash").and_then(Value::as_str))
                .context("missing miner_tx_hash in block json")?;
            prepared_txs.push(prepare_tx(&miner_tx_json, Some(miner_hash))?);
        } else {
            warn!(height = height_u64, "miner_tx missing from block json");
        }

        for (hash, blob) in tx_json_pairs {
            prepared_txs.push(prepare_tx(&blob, Some(&hash))?);
        }

        persist_block(
            &store,
            &checkpoint,
            &header,
            &prepared_txs,
            tip_height_i64,
            finalized_height_i64,
            args.finality_window,
        )
        .await?;

        processed_blocks += 1;
        next_height += 1;

        if processed_blocks % 100 == 0 {
            info!(processed = processed_blocks, "backfill progress");
        }
    }

    info!(processed = processed_blocks, "backfill complete");
    Ok(())
}

async fn fetch_chain_tip(rpc: &Rpc, limiter: &Arc<DefaultDirectRateLimiter>) -> Result<u64> {
    limiter.until_ready().await;
    let res = rpc.get_block_count().await.context("get_block_count rpc")?;
    let highest = res.count.saturating_sub(1);
    Ok(highest)
}

async fn fetch_block_header(
    rpc: &Rpc,
    limiter: &Arc<DefaultDirectRateLimiter>,
    height: u64,
) -> Result<BlockHeader> {
    limiter.until_ready().await;
    let res = rpc
        .get_block_header_by_height(height)
        .await
        .context("fetch header")?;
    Ok(res.block_header)
}

async fn fetch_block_json(
    rpc: &Rpc,
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

async fn fetch_transactions(
    rpc: &Rpc,
    limiter: &Arc<DefaultDirectRateLimiter>,
    hashes: &[String],
    concurrency: usize,
) -> Result<Vec<(String, String)>> {
    if hashes.is_empty() {
        return Ok(Vec::new());
    }

    let chunked = hashes
        .chunks(100)
        .map(|chunk| chunk.to_vec())
        .collect::<Vec<_>>();

    let rpc_clone = rpc.clone();
    let limiter_clone = limiter.clone();
    let stream = stream::iter(chunked.into_iter().map(move |chunk| {
        let rpc = rpc_clone.clone();
        let limiter = limiter_clone.clone();
        async move {
            limiter.until_ready().await;
            let res = rpc
                .get_transactions(&chunk)
                .await
                .with_context(|| "fetch transactions batch")?;
            if !res.missed_tx.is_empty() {
                warn!(missed = res.missed_tx.len(), "daemon missed transactions");
            }

            let missed: HashSet<String> = res.missed_tx.into_iter().collect();
            let mut json_iter = res.txs_as_json.into_iter();
            let mut paired = Vec::with_capacity(chunk.len().saturating_sub(missed.len()));

            for hash in chunk.into_iter() {
                if missed.contains(&hash) {
                    continue;
                }
                let json = json_iter
                    .next()
                    .ok_or_else(|| anyhow!("daemon returned fewer txs than expected"))?;
                paired.push((hash, json));
            }

            if let Some(extra) = json_iter.next() {
                warn!(
                    extra_len = extra.len(),
                    "daemon returned extra transaction payload"
                );
            }

            Ok::<Vec<(String, String)>, anyhow::Error>(paired)
        }
    }));

    let limit = concurrency.max(1);
    stream
        .buffer_unordered(limit)
        .try_fold(Vec::new(), |mut acc, batch| async move {
            acc.extend(batch);
            Ok(acc)
        })
        .await
}

fn extract_tx_hashes(block: &Value) -> Vec<String> {
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

fn prepare_tx(json_str: &str, fallback_hash: Option<&str>) -> Result<PreparedTx> {
    let tx_json = parse_tx_json(json_str).context("parse tx json")?;
    let analysis = analyze_tx(&tx_json).context("analyze tx")?;
    let value: Value = serde_json::from_str(json_str).context("tx json to value")?;

    let hash_str = value
        .get("tx_hash")
        .or_else(|| value.get("hash"))
        .and_then(|v| v.as_str())
        .or(fallback_hash)
        .context("transaction hash missing")?;
    let hash = hex::decode(hash_str).context("decode tx hash")?;
    let hash_hex = hash_str.to_string();

    let size = value_u64(&value, &["size", "blob_size", "weight"])
        .unwrap_or_else(|| json_str.len() as u64);
    let fee = parse_fee(&value);
    let rct_type = value
        .get("rct_signatures")
        .and_then(|rs| rs.get("type"))
        .and_then(Value::as_i64)
        .unwrap_or_default();

    let version = i32::try_from(tx_json.version).context("tx version overflow")?;
    let unlock_time = i64::try_from(tx_json.unlock_time).context("unlock time overflow")?;
    let size_bytes = i32::try_from(size).unwrap_or(i32::MAX);
    let num_inputs = i32::try_from(analysis.num_inputs).context("inputs overflow")?;
    let num_outputs = i32::try_from(analysis.num_outputs).context("outputs overflow")?;
    let rct_type_i32 = i32::try_from(rct_type).unwrap_or_default();

    let proof_type = if analysis.bp_plus {
        Some("CLSAG".to_string())
    } else {
        None
    };

    let extra = serde_json::json!({ "extra": tx_json.extra });

    Ok(PreparedTx {
        hash,
        hash_hex,
        fee,
        size_bytes,
        version,
        unlock_time,
        extra,
        rct_type: rct_type_i32,
        proof_type,
        bp_plus: analysis.bp_plus,
        num_inputs,
        num_outputs,
    })
}

fn value_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(v) = value.get(key) {
            if let Some(num) = v.as_u64() {
                return Some(num);
            }
            if let Some(s) = v.as_str() {
                if let Ok(parsed) = s.parse::<u64>() {
                    return Some(parsed);
                }
            }
        }
    }
    None
}

fn parse_fee(value: &Value) -> Option<i64> {
    value
        .get("rct_signatures")
        .and_then(|rs| rs.get("txnFee"))
        .and_then(|fee| match fee {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        })
        .and_then(|fee| i64::try_from(fee).ok())
}

async fn persist_block(
    store: &Store,
    checkpoint: &Checkpoint,
    header: &BlockHeader,
    txs: &[PreparedTx],
    tip_height: i64,
    finalized_height: i64,
    finality_window: u64,
) -> Result<()> {
    let mut db_tx = store.begin_block().await.context("open sql transaction")?;

    let hash_bytes = hex::decode(&header.hash).context("decode block hash")?;
    let prev_hash_bytes = hex::decode(&header.prev_hash).context("decode prev hash")?;
    let ts = i64::try_from(header.timestamp).context("timestamp overflow")?;
    let size_bytes = i32::try_from(header.size).unwrap_or(i32::MAX);
    let major = i32::try_from(header.major_version).context("major version overflow")?;
    let minor = i32::try_from(header.minor_version).context("minor version overflow")?;
    let nonce = i64::try_from(header.nonce).context("nonce overflow")?;
    let reward = i64::try_from(header.reward).context("reward overflow")?;

    let block_height = i64::try_from(header.height).context("height overflow")?;

    Store::insert_block(
        &mut db_tx,
        block_height,
        &hash_bytes,
        &prev_hash_bytes,
        ts,
        size_bytes,
        major,
        minor,
        nonce,
        i32::try_from(txs.len()).unwrap_or(i32::MAX),
        reward,
    )
    .await
    .context("insert block")?;

    for tx in txs {
        Store::insert_tx(
            &mut db_tx,
            &tx.hash,
            Some(block_height),
            Some(ts),
            false,
            tx.fee,
            tx.size_bytes,
            tx.version,
            tx.unlock_time,
            &tx.extra,
            tx.rct_type,
            tx.proof_type.as_deref(),
            tx.bp_plus,
            tx.num_inputs,
            tx.num_outputs,
        )
        .await
        .context("insert tx")?;
    }

    let included_hex: Vec<String> = txs.iter().map(|tx| tx.hash_hex.clone()).collect();
    Store::evict_mempool_on_inclusion(&mut db_tx, &included_hex)
        .await
        .context("evict mempool on inclusion")?;

    Store::record_tip(
        &mut db_tx,
        i64::try_from(header.height).context("height overflow")?,
        &hash_bytes,
        &prev_hash_bytes,
    )
    .await
    .context("record chain tip")?;

    Store::upsert_soft_facts_for_block(&mut db_tx, block_height)
        .await
        .context("upsert soft facts")?;

    let confirmations = tip_height.saturating_sub(block_height).saturating_add(1);
    let confirmations_i32 = i32::try_from(confirmations).unwrap_or(i32::MAX);
    let is_final = block_height <= finalized_height;
    Store::update_block_confirmations_tx(&mut db_tx, block_height, confirmations_i32, is_final)
        .await
        .context("update block confirmations")?;

    db_tx.commit().await.context("commit block")?;

    checkpoint
        .set(block_height, finalized_height)
        .await
        .context("update checkpoint")?;

    let window_extra = 16i64;
    let finality_i64 = i64::try_from(finality_window).unwrap_or(i64::MAX / 2);
    let span = finality_i64.max(1) + window_extra;
    let start_height = (tip_height - span).max(0);
    store
        .refresh_confirmations(start_height, tip_height, finalized_height)
        .await
        .context("refresh confirmation window")?;

    Ok(())
}

struct PreparedTx {
    hash: Vec<u8>,
    hash_hex: String,
    fee: Option<i64>,
    size_bytes: i32,
    version: i32,
    unlock_time: i64,
    extra: serde_json::Value,
    rct_type: i32,
    proof_type: Option<String>,
    bp_plus: bool,
    num_inputs: i32,
    num_outputs: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_tx_falls_back_to_supplied_hash() {
        let json = r#"{
            "version": 1,
            "unlock_time": 0,
            "vin": [],
            "vout": [],
            "extra": []
        }"#;
        let fallback =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();

        let prepared = prepare_tx(json, Some(&fallback)).expect("prepare tx with fallback hash");

        assert_eq!(prepared.hash_hex, fallback);
        assert_eq!(prepared.hash, hex::decode(&fallback).expect("hex decode"));
    }
}
