use anyhow::{Context, Result};
use httpmock::{prelude::*, Mock};
use ingestor::{reorg::heal_reorg, rpc::Rpc, store::Store};
use sqlx::{migrate::Migrator, PgPool};
use serde_json::json;

static MIGRATOR: Migrator = sqlx::migrate!("../db/migrations");

#[tokio::test]
async fn heals_three_block_reorg_db_only() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping heals_three_block_reorg_db_only: DATABASE_URL not set");
            return Ok(());
        }
    };

    let pool = PgPool::connect(&database_url)
        .await
        .context("connect to database")?;
    MIGRATOR.run(&pool).await.context("run migrations")?;

    let mut cleanup = pool.begin().await?;
    sqlx::query!("DELETE FROM public.chain_tips WHERE height >= $1", 100_i64)
        .execute(&mut *cleanup)
        .await?;
    sqlx::query!("DELETE FROM public.blocks WHERE height >= $1", 100_i64)
        .execute(&mut *cleanup)
        .await?;
    sqlx::query!("DELETE FROM public.mempool_txs")
        .execute(&mut *cleanup)
        .await?;
    cleanup.commit().await?;

    let store = Store::connect(&database_url)
        .await
        .context("connect store")?;

    let mut seed = store.pool().begin().await?;
    let blocks = vec![
        (100_i64, "aa".repeat(32), "00".repeat(32)),
        (101_i64, "ab".repeat(32), "aa".repeat(32)),
        (102_i64, "ac".repeat(32), "ab".repeat(32)),
        (103_i64, "ad".repeat(32), "ac".repeat(32)),
    ];
    for (height, hash_hex, prev_hex) in &blocks {
        let hash = hex::decode(hash_hex).context("decode block hash")?;
        let prev = hex::decode(prev_hex).context("decode prev hash")?;
        sqlx::query(
            "INSERT INTO public.blocks (height, hash, prev_hash, block_timestamp, size_bytes, major_version, minor_version, nonce, tx_count, reward_nanos)
             VALUES ($1,$2,$3,NOW(),1000,14,14,0,0,0)",
        )
        .bind(height)
        .bind(&hash)
        .bind(&prev)
        .execute(&mut *seed)
        .await?;
        sqlx::query(
            "INSERT INTO public.chain_tips (height, hash, prev_hash)
             VALUES ($1,$2,$3)
             ON CONFLICT (height) DO UPDATE SET hash = EXCLUDED.hash, prev_hash = EXCLUDED.prev_hash",
        )
        .bind(height)
        .bind(&hash)
        .bind(&prev)
        .execute(&mut *seed)
        .await?;
    }

    let tx_hash_hex = "de".repeat(32);
    let tx_hash = hex::decode(&tx_hash_hex).context("decode tx hash")?;
    sqlx::query(
        "INSERT INTO public.txs (
             tx_hash, block_height, block_timestamp, in_mempool, fee_nanos,
             size_bytes, version, unlock_time, extra, rct_type, proof_type,
             bp_plus, num_inputs, num_outputs)
         VALUES ($1,$2,NOW(),FALSE,NULL,1,2,0,'{}'::jsonb,0,NULL,TRUE,0,0)",
    )
    .bind(&tx_hash)
    .bind(102_i64)
    .execute(&mut *seed)
    .await?;

    seed.commit().await?;

    let server = MockServer::start();

    fn mock_header<'a>(
        server: &'a MockServer,
        height: u64,
        hash: &str,
        prev_hash: &str,
    ) -> Mock<'a> {
        server.mock(|when, then| {
            when.method(POST).path("/").json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "get_block_header_by_height",
                "params": {"height": height},
            }));
            then.status(200).json_body(json!({
                "result": {
                    "block_header": {
                        "hash": hash,
                        "height": height,
                        "timestamp": 1,
                        "prev_hash": prev_hash,
                        "major_version": 14,
                        "minor_version": 14,
                        "nonce": 0,
                        "reward": 0,
                        "size": 1,
                    },
                    "status": "OK"
                }
            }));
        })
    }

    let _mock_102 = mock_header(&server, 102, &"ee".repeat(32), &"ed".repeat(32));
    let _mock_101 = mock_header(&server, 101, &"ef".repeat(32), &"ee".repeat(32));
    let _mock_100 = mock_header(&server, 100, &"aa".repeat(32), &"00".repeat(32));

    let rpc = Rpc::new(server.url("/"));
    heal_reorg(103, &store, &rpc, 10).await?;

    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM public.blocks WHERE height >= $1")
            .bind(101_i64)
            .fetch_one(store.pool())
            .await?;
    assert_eq!(remaining, 0);

    let tip_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM public.chain_tips WHERE height >= $1")
            .bind(101_i64)
            .fetch_one(store.pool())
            .await?;
    assert_eq!(tip_rows, 0);

    let mempool_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM public.mempool_txs WHERE tx_hash = $1")
            .bind(&tx_hash)
            .fetch_one(store.pool())
            .await?;
    assert_eq!(mempool_count, 1);

    let block_100: Option<i64> =
        sqlx::query_scalar("SELECT height FROM public.blocks WHERE height = $1")
            .bind(100_i64)
            .fetch_optional(store.pool())
            .await?;
    assert_eq!(block_100, Some(100));

    Ok(())
}
