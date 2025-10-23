#[tokio::test]
async fn blocks_marked_pending_in_bootstrap() {
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap()).await.unwrap();
    // Insert a block row manually to mimic commit:
    sqlx::query!("INSERT INTO public.blocks (height, hash, prev_hash, block_timestamp, size_bytes, major_version, minor_version, nonce, tx_count, reward_nanos, analytics_pending)
                  VALUES ($1, decode($2,'hex'), decode($3,'hex'), NOW(), 100,14,14,0,0,0, TRUE)
                  ON CONFLICT DO NOTHING", 424242i64, "aa".repeat(32), "bb".repeat(32)).execute(&pool).await.unwrap();
    let rec = sqlx::query!("SELECT analytics_pending FROM public.blocks WHERE height=$1", 424242i64).fetch_one(&pool).await.unwrap();
    assert!(rec.analytics_pending);
}
