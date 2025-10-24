use std::collections::BTreeMap;

use axum::{
    extract::{Path, Query, State},
    response::Response,
    routing::get,
    Router,
};
use serde::Deserialize;

use crate::util::json_ok;
use crate::{models, state::AppState};

pub async fn healthz() -> Response {
    json_ok(serde_json::json!({"status": "ok"}))
}

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/block/:id", get(get_block))
        .route("/api/v1/blocks", get(list_blocks))
        .route("/api/v1/tx/:hash", get(get_tx))
        .route("/api/v1/tx/:hash/rings", get(get_tx_rings))
        .route("/api/v1/mempool", get(get_mempool))
        .route("/api/v1/key_image/:hex", get(get_key_image))
        .route("/api/v1/search", get(search))
        .route("/api-docs", get(openapi_docs))
}

pub async fn openapi_docs() -> Response {
    let body = include_str!("../openapi.yaml");
    json_ok(serde_yaml::from_str::<serde_json::Value>(body).unwrap())
}

#[derive(Deserialize)]
pub struct Page {
    pub start: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn list_blocks(State(st): State<AppState>, Query(p): Query<Page>) -> Response {
    let limit = p.limit.unwrap_or(20).clamp(1, 200);

    let start_height = match p.start {
        Some(s) if s >= 0 => s,
        Some(_) => return crate::util::json_ok(Vec::<models::BlockView>::new()),
        None => match sqlx::query_scalar!("SELECT MAX(height) FROM public.blocks")
            .fetch_one(&st.db)
            .await
        {
            Ok(Some(h)) => h,
            Ok(None) => return crate::util::json_ok(Vec::<models::BlockView>::new()),
            Err(e) => return crate::util::json_err(500, &format!("db error: {e}")),
        },
    };

    let cache_key = format!("blocks:{start_height}:{limit}");
    if let Some(resp) = crate::util::cached_response(&st.cache, &cache_key).await {
        return resp;
    }

    let rows = sqlx::query_as!(
        models::BlockView,
        r#"
SELECT height, encode(hash,'hex') AS hash, extract(epoch from block_timestamp)::bigint AS ts,
       size_bytes, major_version, minor_version, tx_count, reward_nanos
FROM public.blocks
WHERE height <= $1
ORDER BY height DESC
LIMIT $2
"#,
        start_height,
        limit
    )
    .fetch_all(&st.db)
    .await;

    match rows {
        Ok(v) => crate::util::cached_json(&st.cache, &cache_key, &v, 3).await,
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_block(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    let cache_key = format!("block:{id}");
    if let Some(resp) = crate::util::cached_response(&st.cache, &cache_key).await {
        return resp;
    }

    let is_hex = id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit());
    let row = if is_hex {
        sqlx::query_as!(
            models::BlockView,
            r#"
SELECT height, encode(hash,'hex') AS hash, extract(epoch from block_timestamp)::bigint AS ts,
       size_bytes, major_version, minor_version, tx_count, reward_nanos
FROM public.blocks WHERE hash = decode($1,'hex')
"#,
            id
        )
        .fetch_optional(&st.db)
        .await
    } else {
        let h: i64 = id.parse().unwrap_or(-1);
        sqlx::query_as!(
            models::BlockView,
            r#"
SELECT height, encode(hash,'hex') AS hash, extract(epoch from block_timestamp)::bigint AS ts,
       size_bytes, major_version, minor_version, tx_count, reward_nanos
FROM public.blocks WHERE height = $1
"#,
            h
        )
        .fetch_optional(&st.db)
        .await
    };

    match row {
        Ok(Some(v)) => crate::util::cached_json(&st.cache, &cache_key, &v, 30).await,
        Ok(None) => crate::util::json_err(404, "not found"),
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_tx(State(st): State<AppState>, Path(hash): Path<String>) -> Response {
    if !crate::util::is_hex_64(&hash) {
        return crate::util::json_err(400, "invalid hash");
    }
    let cache_key = format!("tx:{hash}");
    if let Some(resp) = crate::util::cached_response(&st.cache, &cache_key).await {
        return resp;
    }

    let row = sqlx::query_as!(
        models::TxView,
        r#"
SELECT
  encode(tx_hash,'hex') AS hash,
  block_height,
  extract(epoch from block_timestamp)::bigint AS ts,
  in_mempool,
  fee_nanos,
  size_bytes,
  version,
  unlock_time,
  extra::text AS extra_json,
  rct_type,
  proof_type,
  bp_plus,
  num_inputs,
  num_outputs
FROM public.txs WHERE tx_hash = decode($1,'hex')
"#,
        hash.as_str()
    )
    .fetch_optional(&st.db)
    .await;

    let tx = match row {
        Ok(Some(v)) => v,
        Ok(None) => return crate::util::json_err(404, "not found"),
        Err(e) => return crate::util::json_err(500, &format!("db error: {e}")),
    };

    let inputs = match sqlx::query_as!(
        models::InputView,
        r#"
SELECT idx,
       encode(key_image,'hex') AS "key_image!",
       ring_size,
       encode(pseudo_out,'hex') AS pseudo_out
FROM public.tx_inputs
WHERE tx_hash = decode($1,'hex')
ORDER BY idx ASC
"#,
        hash.as_str()
    )
    .fetch_all(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => return crate::util::json_err(500, &format!("db error: {e}")),
    };

    let outputs = match sqlx::query_as!(
        models::OutputView,
        r#"
SELECT idx_in_tx,
       global_index,
       amount,
       encode(commitment,'hex') AS "commitment!",
       encode(stealth_public_key,'hex') AS "stealth_public_key!",
       encode(spent_by_key_image,'hex') AS spent_by_key_image,
       encode(spent_in_tx,'hex') AS spent_in_tx
FROM public.outputs
WHERE tx_hash = decode($1,'hex')
ORDER BY idx_in_tx ASC
"#,
        hash.as_str()
    )
    .fetch_all(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => return crate::util::json_err(500, &format!("db error: {e}")),
    };

    let body = models::TxDetailView {
        tx,
        inputs,
        outputs,
    };

    crate::util::cached_json(&st.cache, &cache_key, &body, 60).await
}

pub async fn get_mempool(State(st): State<AppState>) -> Response {
    let cache_key = "mempool:latest";
    if let Some(resp) = crate::util::cached_response(&st.cache, cache_key).await {
        return resp;
    }

    let rows = sqlx::query_as!(
        models::MempoolView,
        r#"
SELECT encode(tx_hash,'hex') AS hash,
       extract(epoch from first_seen)::bigint AS first_seen,
       extract(epoch from last_seen)::bigint AS last_seen,
       fee_rate, relayed_by
FROM public.mempool_txs
ORDER BY last_seen DESC
LIMIT 1000
"#
    )
    .fetch_all(&st.db)
    .await;

    match rows {
        Ok(v) => crate::util::cached_json(&st.cache, cache_key, &v, 2).await,
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_tx_rings(State(st): State<AppState>, Path(hash): Path<String>) -> Response {
    if !crate::util::is_hex_64(&hash) {
        return crate::util::json_err(400, "invalid hash");
    }
    let cache_key = format!("rings:{hash}");
    if let Some(resp) = crate::util::cached_response(&st.cache, &cache_key).await {
        return resp;
    }

    let rows = sqlx::query_as!(
        models::RingView,
        r#"
SELECT
  encode(r.tx_hash,'hex') AS tx_hash,
  r.input_idx,
  r.ring_index,
  o.global_index
FROM public.rings r
LEFT JOIN public.outputs o ON o.output_id = r.referenced_output_id
WHERE r.tx_hash = decode($1,'hex')
ORDER BY r.input_idx ASC, r.ring_index ASC
"#,
        hash.as_str()
    )
    .fetch_all(&st.db)
    .await;

    let rows = match rows {
        Ok(v) => v,
        Err(e) => return crate::util::json_err(500, &format!("db error: {e}")),
    };

    let mut grouped: BTreeMap<i32, Vec<models::RingMemberView>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry(row.input_idx)
            .or_default()
            .push(models::RingMemberView {
                ring_index: row.ring_index,
                global_index: row.global_index,
            });
    }

    let rings: Vec<models::RingSetView> = grouped
        .into_iter()
        .map(|(input_idx, mut members)| {
            members.sort_by_key(|m| m.ring_index);
            models::RingSetView { input_idx, members }
        })
        .collect();

    crate::util::cached_json(&st.cache, &cache_key, &rings, 60).await
}

pub async fn get_key_image(State(st): State<AppState>, Path(hex): Path<String>) -> Response {
    if !crate::util::is_hex_64(&hex) {
        return crate::util::json_err(400, "invalid key image");
    }
    let cache_key = format!("ki:{hex}");
    if let Some(resp) = crate::util::cached_response(&st.cache, &cache_key).await {
        return resp;
    }

    let row = sqlx::query_as!(
        models::KeyImageView,
        r#"
SELECT
  encode(ti.key_image,'hex') AS key_image,
  encode(t.tx_hash,'hex') AS spending_tx,
  t.block_height
FROM public.tx_inputs ti
JOIN public.txs t ON t.tx_hash = ti.tx_hash
WHERE ti.key_image = decode($1,'hex')
ORDER BY t.block_height DESC NULLS LAST
LIMIT 1
"#,
        hex.as_str()
    )
    .fetch_optional(&st.db)
    .await;

    match row {
        Ok(Some(v)) => crate::util::cached_json(&st.cache, &cache_key, &v, 120).await,
        Ok(None) => crate::util::json_err(404, "not found"),
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

#[derive(Deserialize)]
pub struct Q {
    pub q: String,
}

pub async fn search(State(st): State<AppState>, Query(Q { q }): Query<Q>) -> Response {
    let s = q.trim();
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        if sqlx::query_scalar!(
            "SELECT 1 FROM public.txs WHERE tx_hash = decode($1,'hex') LIMIT 1",
            s
        )
        .fetch_optional(&st.db)
        .await
        .ok()
        .flatten()
        .is_some()
        {
            return crate::util::json_ok(models::SearchResult {
                kind: "tx".to_owned(),
                value: serde_json::Value::String(s.to_owned()),
            });
        }
        if sqlx::query_scalar!(
            "SELECT 1 FROM public.blocks WHERE hash = decode($1,'hex') LIMIT 1",
            s
        )
        .fetch_optional(&st.db)
        .await
        .ok()
        .flatten()
        .is_some()
        {
            return crate::util::json_ok(models::SearchResult {
                kind: "block".to_owned(),
                value: serde_json::Value::String(s.to_owned()),
            });
        }
        if sqlx::query_scalar!(
            "SELECT 1 FROM public.tx_inputs WHERE key_image = decode($1,'hex') LIMIT 1",
            s
        )
        .fetch_optional(&st.db)
        .await
        .ok()
        .flatten()
        .is_some()
        {
            return crate::util::json_ok(models::SearchResult {
                kind: "key_image".to_owned(),
                value: serde_json::Value::String(s.to_owned()),
            });
        }
    }
    if let Ok(h) = s.parse::<i64>() {
        if sqlx::query_scalar!("SELECT 1 FROM public.blocks WHERE height=$1 LIMIT 1", h)
            .fetch_optional(&st.db)
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return crate::util::json_ok(models::SearchResult {
                kind: "height".to_owned(),
                value: serde_json::json!(h),
            });
        }
        if sqlx::query_scalar!(
            "SELECT 1 FROM public.outputs WHERE global_index=$1 LIMIT 1",
            h
        )
        .fetch_optional(&st.db)
        .await
        .ok()
        .flatten()
        .is_some()
        {
            return crate::util::json_ok(models::SearchResult {
                kind: "global_index".to_owned(),
                value: serde_json::json!(h),
            });
        }
    }
    crate::util::json_err(404, "no match")
}
