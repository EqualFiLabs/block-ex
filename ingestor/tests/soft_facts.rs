#[tokio::test]
async fn soft_facts_exist_for_recent_block() {
    let db = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::PgPool::connect(&db).await.unwrap();
    // Find the latest block you ingested
    let rec = sqlx::query!("SELECT max(height) AS h FROM public.blocks")
        .fetch_one(&pool)
        .await
        .unwrap();
    let h = rec.h.expect("no blocks");
    let sf = sqlx::query!(
        "SELECT total_fee, clsag_count FROM public.soft_facts WHERE block_height=$1",
        h
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(sf.clsag_count.unwrap_or(0) >= 0);
    assert!(sf.total_fee.unwrap_or(0) >= 0);
}
