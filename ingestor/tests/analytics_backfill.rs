#[tokio::test]
async fn analytics_backfill_processes_pending_blocks() {
    use ingestor::analytics;

    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping processes_pending_blocks: DATABASE_URL not set");
        return;
    };

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();

    // Clear any previously pending analytics work to keep this test deterministic.
    let _ = analytics::backfill(&pool, 1000).await.unwrap();

    let pending_height = 990_000i64;
    let missing_height = 990_001i64;

    for height in [pending_height, missing_height] {
        sqlx::query!(
            "DELETE FROM public.soft_facts WHERE block_height = $1",
            height
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!("DELETE FROM public.txs WHERE block_height = $1", height)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("DELETE FROM public.blocks WHERE height = $1", height)
            .execute(&pool)
            .await
            .unwrap();
    }

    sqlx::query!(
        "INSERT INTO public.blocks (height, hash, prev_hash, block_timestamp, size_bytes, major_version, minor_version, nonce, tx_count, reward_nanos, analytics_pending)
         VALUES ($1, decode($2,'hex'), decode($3,'hex'), NOW(), 100,14,14,0,0,0, TRUE)",
        pending_height,
        "aa".repeat(32),
        "bb".repeat(32)
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO public.blocks (height, hash, prev_hash, block_timestamp, size_bytes, major_version, minor_version, nonce, tx_count, reward_nanos, analytics_pending)
         VALUES ($1, decode($2,'hex'), decode($3,'hex'), NOW(), 100,14,14,0,0,0, FALSE)",
        missing_height,
        "cc".repeat(32),
        "dd".repeat(32)
    )
    .execute(&pool)
    .await
    .unwrap();

    let processed = analytics::backfill(&pool, 10).await.unwrap();
    assert_eq!(processed, 2);

    for height in [pending_height, missing_height] {
        let block = sqlx::query!(
            "SELECT analytics_pending FROM public.blocks WHERE height = $1",
            height
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!block.analytics_pending);

        let sf = sqlx::query!(
            "SELECT block_height FROM public.soft_facts WHERE block_height = $1",
            height
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sf.block_height, height);
    }
}
