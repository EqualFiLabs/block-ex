use anyhow::Result;
use sqlx::Row;

pub async fn backfill(db: &sqlx::PgPool, batch: i64) -> Result<i64> {
    let mut done = 0i64;
    loop {
        let heights = sqlx::query(
            "SELECT height FROM public.blocks b
             LEFT JOIN public.soft_facts s ON s.block_height=b.height
             WHERE b.analytics_pending = TRUE OR s.block_height IS NULL
             ORDER BY b.height ASC LIMIT $1",
        )
        .bind(batch)
        .fetch_all(db)
        .await?;
        if heights.is_empty() {
            break;
        }
        let mut tx = db.begin().await?;
        for h in heights {
            let h: i64 = h.get("height");
            super::store::Store::upsert_soft_facts_for_block(&mut tx, h).await?;
            sqlx::query("UPDATE public.blocks SET analytics_pending=FALSE WHERE height=$1")
                .bind(h)
                .execute(&mut *tx)
                .await?;
            done += 1;
        }
        tx.commit().await?;
    }
    Ok(done)
}
