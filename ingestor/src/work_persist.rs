use std::{convert::TryFrom, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    checkpoint::Checkpoint,
    codec::{analyze_tx, parse_tx_json},
    pipeline::{Shutdown, TxMsg},
    store::Store,
};

pub struct Config {
    pub store: Store,
    pub checkpoint: Arc<Checkpoint>,
    pub finality_window: u64,
    pub do_analytics: bool,
}

pub async fn run(
    mut rx: mpsc::Receiver<TxMsg>,
    cfg: Config,
    _shutdown: Option<Shutdown>,
) -> Result<()> {
    let mut processed = 0u64;
    while let Some(msg) = rx.recv().await {
        let prepared = prepare_block(&msg, cfg.do_analytics)?;
        persist_block(&cfg, &msg, &prepared).await?;
        processed += 1;
        if processed % 100 == 0 {
            info!(processed, "persistence progress");
        }
    }
    info!(processed, "persistence complete");
    Ok(())
}

fn prepare_block(msg: &TxMsg, do_analytics: bool) -> Result<Vec<PreparedTx>> {
    let mut prepared = Vec::with_capacity(msg.tx_jsons.len() + 1);

    if let Some(json) = &msg.miner_tx_json {
        if let Some(fallback_hash) = msg.miner_tx_hash.as_deref() {
            prepared.push(prepare_tx(json, Some(fallback_hash), do_analytics)?);
        } else {
            warn!(height = msg.height, "miner_tx hash missing for block");
        }
    } else {
        warn!(height = msg.height, "miner_tx missing from block json");
    }

    if msg.ordered_tx_hashes.len() != msg.tx_jsons.len() {
        warn!(
            height = msg.height,
            hashes = msg.ordered_tx_hashes.len(),
            jsons = msg.tx_jsons.len(),
            "tx hash / json count mismatch"
        );
    }

    for (hash, json) in msg.ordered_tx_hashes.iter().zip(msg.tx_jsons.iter()) {
        prepared.push(prepare_tx(json, Some(hash), do_analytics)?);
    }

    Ok(prepared)
}

async fn persist_block(cfg: &Config, msg: &TxMsg, txs: &[PreparedTx]) -> Result<()> {
    let mut db_tx = cfg
        .store
        .begin_block()
        .await
        .context("open sql transaction")?;
    let mut mark_analytics_pending = false;

    let hash_bytes = hex::decode(&msg.header.hash).context("decode block hash")?;
    let prev_hash_bytes = hex::decode(&msg.header.prev_hash).context("decode prev hash")?;
    let ts = i64::try_from(msg.header.timestamp).context("timestamp overflow")?;
    let size_bytes = i32::try_from(msg.header.size).unwrap_or(i32::MAX);
    let major = i32::try_from(msg.header.major_version).context("major version overflow")?;
    let minor = i32::try_from(msg.header.minor_version).context("minor version overflow")?;
    let nonce = i64::try_from(msg.header.nonce).context("nonce overflow")?;
    let reward = i64::try_from(msg.header.reward).context("reward overflow")?;

    let block_height = i64::try_from(msg.header.height).context("height overflow")?;

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

    Store::record_tip(&mut db_tx, block_height, &hash_bytes, &prev_hash_bytes)
        .await
        .context("record chain tip")?;

    if cfg.do_analytics {
        Store::upsert_soft_facts_for_block(&mut db_tx, block_height)
            .await
            .context("upsert soft facts")?;
    } else {
        mark_analytics_pending = true;
    }

    let confirmations = msg
        .tip_height
        .saturating_sub(block_height)
        .saturating_add(1);
    let confirmations_i32 = i32::try_from(confirmations).unwrap_or(i32::MAX);
    let is_final = block_height <= msg.finalized_height;
    Store::update_block_confirmations_tx(&mut db_tx, block_height, confirmations_i32, is_final)
        .await
        .context("update block confirmations")?;

    db_tx.commit().await.context("commit block")?;

    if mark_analytics_pending {
        sqlx::query("UPDATE public.blocks SET analytics_pending = TRUE WHERE height=$1")
            .bind(block_height)
            .execute(cfg.store.pool())
            .await
            .ok();
    }

    cfg.checkpoint
        .set(block_height, msg.finalized_height)
        .await
        .context("update checkpoint")?;

    let window_extra = 16i64;
    let finality_i64 = i64::try_from(cfg.finality_window).unwrap_or(i64::MAX / 2);
    let span = finality_i64.max(1) + window_extra;
    let start_height = (msg.tip_height - span).max(0);
    cfg.store
        .refresh_confirmations(start_height, msg.tip_height, msg.finalized_height)
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

fn prepare_tx(
    json_str: &str,
    fallback_hash: Option<&str>,
    do_analytics: bool,
) -> Result<PreparedTx> {
    let tx_json = parse_tx_json(json_str).context("parse tx json")?;
    let value: serde_json::Value = serde_json::from_str(json_str).context("tx json to value")?;

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
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default();

    let version = i32::try_from(tx_json.version).context("tx version overflow")?;
    let unlock_time = i64::try_from(tx_json.unlock_time).context("unlock time overflow")?;
    let size_bytes = i32::try_from(size).unwrap_or(i32::MAX);
    let (num_inputs_usize, num_outputs_usize, bp_plus, proof_type) = if do_analytics {
        let analysis = analyze_tx(&tx_json).context("analyze tx")?;
        let proof_type = if analysis.bp_plus {
            Some("CLSAG".to_string())
        } else {
            None
        };
        (
            analysis.num_inputs,
            analysis.num_outputs,
            analysis.bp_plus,
            proof_type,
        )
    } else {
        let num_inputs = tx_json.vin.len();
        let num_outputs = tx_json.vout.len();
        let has_bp_plus = tx_json
            .rctsig_prunable
            .get("bp_plus")
            .or_else(|| tx_json.rctsig_prunable.get("bp"))
            .map(|v| !matches!(v, serde_json::Value::Null))
            .unwrap_or(false);
        let proof_type = if has_bp_plus {
            Some("CLSAG".to_string())
        } else {
            None
        };
        (num_inputs, num_outputs, has_bp_plus, proof_type)
    };
    let num_inputs = i32::try_from(num_inputs_usize).context("inputs overflow")?;
    let num_outputs = i32::try_from(num_outputs_usize).context("outputs overflow")?;
    let rct_type_i32 = i32::try_from(rct_type).unwrap_or_default();

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
        bp_plus,
        num_inputs,
        num_outputs,
    })
}

fn value_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(v) = value.get(*key) {
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

fn parse_fee(value: &serde_json::Value) -> Option<i64> {
    value
        .get("rct_signatures")
        .and_then(|rs| rs.get("txnFee"))
        .and_then(|fee| match fee {
            serde_json::Value::Number(n) => n.as_u64(),
            serde_json::Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        })
        .and_then(|fee| i64::try_from(fee).ok())
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

        let prepared =
            prepare_tx(json, Some(&fallback), true).expect("prepare tx with fallback hash");

        assert_eq!(prepared.hash_hex, fallback);
        assert_eq!(prepared.hash, hex::decode(&fallback).expect("hex decode"));
    }
}
