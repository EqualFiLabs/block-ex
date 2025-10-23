use axum::{
    extract::{Path, Query, State},
    routing::get,
    Response, Router,
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
    let start = p.start.unwrap_or_else(|| 0);
    let limit = p.limit.unwrap_or(20).clamp(1, 200);

    let rows = if start == 0 {
        sqlx::query_as!(
            models::BlockView,
            r#"
SELECT height, encode(hash,'hex') AS hash, extract(epoch from block_timestamp)::bigint AS ts,
       size_bytes, major_version, minor_version, tx_count, reward_nanos
FROM public.blocks
ORDER BY height DESC
LIMIT $1
"#,
            limit
        )
        .fetch_all(&st.db)
        .await
    } else {
        sqlx::query_as!(
            models::BlockView,
            r#"
SELECT height, encode(hash,'hex') AS hash, extract(epoch from block_timestamp)::bigint AS ts,
       size_bytes, major_version, minor_version, tx_count, reward_nanos
FROM public.blocks
WHERE height >= $1
ORDER BY height ASC
LIMIT $2
"#,
            start,
            limit
        )
        .fetch_all(&st.db)
        .await
    };

    match rows {
        Ok(v) => {
            crate::util::cached_json(&st.cache, &format!("blocks:{start}:{limit}"), &v, 3).await
        }
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_block(State(st): State<AppState>, Path(id): Path<String>) -> Response {
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
        Ok(Some(v)) => crate::util::cached_json(&st.cache, &format!("block:{id}"), &v, 30).await,
        Ok(None) => crate::util::json_err(404, "not found"),
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_tx(State(st): State<AppState>, Path(hash): Path<String>) -> Response {
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
        hash
    )
    .fetch_optional(&st.db)
    .await;

    match row {
        Ok(Some(v)) => crate::util::cached_json(&st.cache, &format!("tx:{hash}"), &v, 60).await,
        Ok(None) => crate::util::json_err(404, "not found"),
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_mempool(State(st): State<AppState>) -> Response {
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
        Ok(v) => crate::util::cached_json(&st.cache, "mempool:latest", &v, 2).await,
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_tx_rings(State(st): State<AppState>, Path(hash): Path<String>) -> Response {
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
        hash
    )
    .fetch_all(&st.db)
    .await;

    match rows {
        Ok(v) => crate::util::cached_json(&st.cache, &format!("rings:{hash}"), &v, 60).await,
        Err(e) => crate::util::json_err(500, &format!("db error: {e}")),
    }
}

pub async fn get_key_image(State(st): State<AppState>, Path(hex): Path<String>) -> Response {
    let row = sqlx::query_as!(
        models::KeyImageView,
        r#"
SELECT
  encode(key_image,'hex') AS key_image,
  encode(tx_hash,'hex') AS spending_tx,
  block_height
FROM public.tx_inputs
JOIN public.txs ON txs.tx_hash = tx_inputs.tx_hash
WHERE key_image = decode($1,'hex')
ORDER BY block_height DESC NULLS LAST
LIMIT 1
"#,
        hex
    )
    .fetch_optional(&st.db)
    .await;

    match row {
        Ok(Some(v)) => crate::util::cached_json(&st.cache, &format!("ki:{hex}"), &v, 120).await,
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
            return crate::util::json_ok(serde_json::json!({"kind": "tx", "value": s}));
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
            return crate::util::json_ok(serde_json::json!({"kind": "block", "value": s}));
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
            return crate::util::json_ok(serde_json::json!({"kind": "key_image", "value": s}));
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
            return crate::util::json_ok(serde_json::json!({"kind": "height", "value": h}));
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
            return crate::util::json_ok(serde_json::json!({"kind": "global_index", "value": h}));
        }
    }
    crate::util::json_err(404, "no match")
}
