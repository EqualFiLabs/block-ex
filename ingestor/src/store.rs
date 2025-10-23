use anyhow::Result;
use sqlx::{postgres::PgQueryResult, PgPool, Postgres, Row, Transaction};

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

impl Store {
    pub async fn connect(db_url: &str) -> Result<Self> {
        let pool = PgPool::connect(db_url).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn begin_block(&self) -> Result<Transaction<'_, Postgres>> {
        Ok(self.pool.begin().await?)
    }

    pub async fn insert_block(
        tx: &mut Transaction<'_, Postgres>,
        height: i64,
        hash: &[u8],
        prev_hash: &[u8],
        ts: i64,
        size_bytes: i32,
        major: i32,
        minor: i32,
        nonce: i64,
        tx_count: i32,
        reward_nanos: i64,
    ) -> Result<PgQueryResult> {
        sqlx::query(
            r#"
INSERT INTO public.blocks (height, hash, prev_hash, block_timestamp, size_bytes, major_version, minor_version, nonce, tx_count, reward_nanos)
VALUES ($1, $2, $3, to_timestamp($4), $5, $6, $7, $8, $9, $10)
ON CONFLICT DO NOTHING
"#,
        )
        .bind(height)
        .bind(hash)
        .bind(prev_hash)
        .bind(ts)
        .bind(size_bytes)
        .bind(major)
        .bind(minor)
        .bind(nonce)
        .bind(tx_count)
        .bind(reward_nanos)
        .execute(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn insert_tx(
        tx: &mut Transaction<'_, Postgres>,
        tx_hash: &[u8],
        block_height: Option<i64>,
        block_ts: Option<i64>,
        in_mempool: bool,
        fee_nanos: Option<i64>,
        size_bytes: i32,
        version: i32,
        unlock_time: i64,
        extra: &serde_json::Value,
        rct_type: i32,
        proof_type: Option<&str>,
        bp_plus: bool,
        num_inputs: i32,
        num_outputs: i32,
    ) -> Result<PgQueryResult> {
        sqlx::query(
            r#"
INSERT INTO public.txs
(tx_hash, block_height, block_timestamp, in_mempool, fee_nanos, size_bytes, version, unlock_time, extra, rct_type, proof_type, bp_plus, num_inputs, num_outputs)
VALUES ($1, $2, CASE WHEN $3 IS NULL THEN NULL ELSE to_timestamp($3) END, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
ON CONFLICT DO NOTHING
"#,
        )
        .bind(tx_hash)
        .bind(block_height)
        .bind(block_ts)
        .bind(in_mempool)
        .bind(fee_nanos)
        .bind(size_bytes)
        .bind(version)
        .bind(unlock_time)
        .bind(extra)
        .bind(rct_type)
        .bind(proof_type)
        .bind(bp_plus)
        .bind(num_inputs)
        .bind(num_outputs)
        .execute(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn insert_input(
        tx: &mut Transaction<'_, Postgres>,
        tx_hash: &[u8],
        idx: i32,
        key_image: &[u8],
        ring_size: i32,
        pseudo_out: Option<&[u8]>,
    ) -> Result<PgQueryResult> {
        sqlx::query(
            r#"
INSERT INTO public.tx_inputs (tx_hash, idx, key_image, ring_size, pseudo_out)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (tx_hash, idx) DO NOTHING
"#,
        )
        .bind(tx_hash)
        .bind(idx)
        .bind(key_image)
        .bind(ring_size)
        .bind(pseudo_out)
        .execute(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn insert_output(
        tx: &mut Transaction<'_, Postgres>,
        tx_hash: &[u8],
        idx_in_tx: i32,
        commitment: &[u8],
        amount: Option<i64>,
        stealth_pub: &[u8],
        global_index: Option<i64>,
    ) -> Result<PgQueryResult> {
        sqlx::query(
            r#"
INSERT INTO public.outputs (tx_hash, idx_in_tx, commitment, amount, stealth_public_key, global_index)
VALUES ($1, $2, $3, $4, $5, $6)
ON CONFLICT (tx_hash, idx_in_tx) DO NOTHING
"#,
        )
        .bind(tx_hash)
        .bind(idx_in_tx)
        .bind(commitment)
        .bind(amount)
        .bind(stealth_pub)
        .bind(global_index)
        .execute(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn record_tip(
        tx: &mut Transaction<'_, Postgres>,
        height: i64,
        hash: &[u8],
        prev_hash: &[u8],
    ) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO public.chain_tips (height, hash, prev_hash) VALUES ($1,$2,$3)
               ON CONFLICT (height) DO UPDATE SET hash = EXCLUDED.hash, prev_hash = EXCLUDED.prev_hash"#,
            height,
            hash,
            prev_hash
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn upsert_soft_facts_for_block(
        tx: &mut Transaction<'_, Postgres>,
        height: i64,
    ) -> Result<()> {
        let rec = sqlx::query!(
            r#"
WITH per_tx AS (
  SELECT
    COALESCE(fee_nanos,0) AS fee,
    NULLIF(size_bytes,0) AS size,
    num_inputs,
    (CASE WHEN size_bytes>0 THEN COALESCE(fee_nanos,0)::numeric / size_bytes::numeric ELSE NULL END) AS fee_rate
  FROM public.txs WHERE block_height = $1
),
aggs AS (
  SELECT
    SUM(fee)::bigint AS total_fee,
    AVG(NULLIF(num_inputs,0))::double precision AS avg_inputs,
    (PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY fee_rate))::double precision AS median_fee_rate
  FROM per_tx
)
SELECT
  COALESCE(total_fee,0)::bigint AS total_fee,
  COALESCE(avg_inputs,0::double precision) AS avg_inputs,
  COALESCE(median_fee_rate,0::double precision) AS median_fee_rate
FROM aggs
"#,
            height
        )
        .fetch_one(&mut **tx)
        .await?;

        let bp_total_bytes: i64 = 0;
        let clsag_count: i32 = {
            let r = sqlx::query!(
                "SELECT COALESCE(SUM(num_inputs),0)::int AS c FROM public.txs WHERE block_height=$1",
                height
            )
            .fetch_one(&mut **tx)
            .await?;
            r.c.unwrap_or(0)
        };

        sqlx::query!(
            r#"
INSERT INTO public.soft_facts
(block_height, block_timestamp, total_fee, avg_ring_size, median_fee_rate, bp_total_bytes, clsag_count)
SELECT b.height, b.block_timestamp, $2, ($3)::double precision, ($4)::double precision, $5, $6 FROM public.blocks b WHERE b.height = $1
ON CONFLICT (block_height) DO UPDATE
  SET total_fee=$2, avg_ring_size=($3)::double precision, median_fee_rate=($4)::double precision, bp_total_bytes=$5, clsag_count=$6
"#,
            height,
            rec.total_fee,
            rec.avg_inputs,
            rec.median_fee_rate,
            bp_total_bytes,
            clsag_count
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("UPDATE public.blocks SET analytics_pending = FALSE WHERE height=$1")
            .bind(height)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    pub async fn update_block_confirmations_tx(
        tx: &mut Transaction<'_, Postgres>,
        height: i64,
        confirmations: i32,
        is_final: bool,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE public.blocks SET confirmations = $2, is_final = $3 WHERE height = $1",
            height,
            confirmations,
            is_final
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn refresh_confirmations(
        &self,
        start_height: i64,
        tip_height: i64,
        finalized_height: i64,
    ) -> Result<()> {
        let start = start_height.min(tip_height).max(0);
        sqlx::query!(
            r#"
WITH params AS (
  SELECT $1::bigint AS start_h, $2::bigint AS tip_h, $3::bigint AS finalized_h
)
UPDATE public.blocks AS b
SET confirmations = GREATEST(params.tip_h - b.height + 1, 0),
    is_final = b.height <= params.finalized_h
FROM params
WHERE b.height BETWEEN params.start_h AND params.tip_h
"#,
            start,
            tip_height,
            finalized_height
        )
        .execute(self.pool())
        .await?;

        sqlx::query!(
            "UPDATE public.blocks SET is_final = true WHERE height <= $1 AND is_final = false",
            finalized_height
        )
        .execute(self.pool())
        .await?;

        Ok(())
    }

    pub async fn block_hash_at(&self, height: i64) -> Result<Option<Vec<u8>>> {
        let rec = sqlx::query!("SELECT hash FROM public.blocks WHERE height=$1", height)
            .fetch_optional(self.pool())
            .await?;
        Ok(rec.map(|r| r.hash))
    }

    pub async fn evict_mempool_on_inclusion(
        tx: &mut Transaction<'_, Postgres>,
        included_hashes_hex: &[String],
    ) -> Result<PgQueryResult> {
        for hash in included_hashes_hex {
            let _ = sqlx::query("DELETE FROM public.mempool_txs WHERE tx_hash = decode($1,'hex')")
                .bind(hash)
                .execute(&mut **tx)
                .await?;
        }

        Ok(PgQueryResult::default())
    }

    pub async fn requeue_mempool_from_block(
        tx: &mut Transaction<'_, Postgres>,
        block_height: i64,
    ) -> Result<()> {
        let rows = sqlx::query(
            "SELECT encode(tx_hash, 'hex') AS h FROM public.txs WHERE block_height = $1",
        )
        .bind(block_height)
        .fetch_all(&mut **tx)
        .await?;

        for row in rows {
            if let Ok(hash) = row.try_get::<String, _>("h") {
                sqlx::query(
                    r#"INSERT INTO public.mempool_txs (tx_hash, first_seen, last_seen)
                       VALUES (decode($1,'hex'), NOW(), NOW())
                       ON CONFLICT (tx_hash) DO UPDATE SET last_seen = NOW()"#,
                )
                .bind(&hash)
                .execute(&mut **tx)
                .await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Store;
    use anyhow::Result;
    use sqlx::{migrate::Migrator, PgPool};

    static MIGRATOR: Migrator = sqlx::migrate!("../db/migrations");

    async fn setup_pool() -> Result<Option<PgPool>> {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(url) => url,
            Err(_) => return Ok(None),
        };

        let pool = PgPool::connect(&database_url).await?;
        MIGRATOR.run(&pool).await?;
        Ok(Some(pool))
    }

    #[tokio::test]
    async fn evict_mempool_removes_included_transactions() -> Result<()> {
        let Some(pool) = setup_pool().await? else {
            eprintln!("skipping evict_mempool_removes_included_transactions: DATABASE_URL not set");
            return Ok(());
        };

        let mut tx = pool.begin().await?;
        let hash = "01".repeat(32);

        sqlx::query(
            r#"INSERT INTO public.mempool_txs (tx_hash, first_seen, last_seen)
               VALUES (decode($1,'hex'), NOW(), NOW())"#,
        )
        .bind(&hash)
        .execute(&mut *tx)
        .await?;

        Store::evict_mempool_on_inclusion(&mut tx, &[hash.clone()]).await?;

        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM public.mempool_txs WHERE tx_hash = decode($1,'hex')",
        )
        .bind(&hash)
        .fetch_one(&mut *tx)
        .await?;

        assert_eq!(remaining, 0);
        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn requeue_mempool_inserts_transactions() -> Result<()> {
        let Some(pool) = setup_pool().await? else {
            eprintln!("skipping requeue_mempool_inserts_transactions: DATABASE_URL not set");
            return Ok(());
        };

        let mut tx = pool.begin().await?;
        let hash = "02".repeat(32);
        let block_height = 42_i64;

        sqlx::query(
            r#"INSERT INTO public.txs (
                    tx_hash, block_height, block_timestamp, in_mempool, fee_nanos,
                    size_bytes, version, unlock_time, extra, rct_type, proof_type,
                    bp_plus, num_inputs, num_outputs
                ) VALUES (decode($1,'hex'), $2, NOW(), FALSE, NULL,
                          1, 2, 0, '{}'::jsonb, 0, NULL,
                          TRUE, 0, 0)"#,
        )
        .bind(&hash)
        .bind(block_height)
        .execute(&mut *tx)
        .await?;

        Store::requeue_mempool_from_block(&mut tx, block_height).await?;

        let in_mempool: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM public.mempool_txs WHERE tx_hash = decode($1,'hex')",
        )
        .bind(&hash)
        .fetch_one(&mut *tx)
        .await?;

        assert_eq!(in_mempool, 1);
        tx.rollback().await?;
        Ok(())
    }
}
