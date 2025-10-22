use anyhow::Result;
use sqlx::PgPool;

pub struct Checkpoint {
    pool: PgPool,
}

impl Checkpoint {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self) -> Result<i64> {
        let rec = sqlx::query!("SELECT last_height FROM ingestor_checkpoint WHERE id=1")
            .fetch_one(&self.pool)
            .await?;
        Ok(rec.last_height.unwrap_or(0))
    }

    pub async fn set(&self, h: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE ingestor_checkpoint SET last_height=$1, updated_at=NOW() WHERE id=1",
            h
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
