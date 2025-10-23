use anyhow::Result;
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct Checkpoint {
    pool: PgPool,
}

#[derive(Debug, Clone, Copy)]
pub struct CheckpointState {
    pub ingested_height: i64,
    pub finalized_height: i64,
}

impl Checkpoint {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_state(&self) -> Result<CheckpointState> {
        let rec = sqlx::query(
            "SELECT last_height, finalized_height FROM ingestor_checkpoint WHERE id=$1",
        )
        .bind(1i32)
        .fetch_optional(&self.pool)
        .await?;

        let ingested_height = rec
            .as_ref()
            .map(|row| row.try_get::<i64, _>("last_height"))
            .transpose()?
            .unwrap_or(0);

        let finalized_height = rec
            .map(|row| row.try_get::<i64, _>("finalized_height"))
            .transpose()?
            .unwrap_or(0);

        Ok(CheckpointState {
            ingested_height,
            finalized_height,
        })
    }

    pub async fn get(&self) -> Result<i64> {
        Ok(self.get_state().await?.ingested_height)
    }

    pub async fn set(&self, ingested_height: i64, finalized_height: i64) -> Result<()> {
        sqlx::query(
            r#"
INSERT INTO ingestor_checkpoint (id, last_height, updated_at)
VALUES (1, $1, NOW())
ON CONFLICT (id)
DO UPDATE SET last_height = EXCLUDED.last_height,
              updated_at = NOW()
"#,
        )
        .bind(ingested_height)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
UPDATE ingestor_checkpoint
SET finalized_height = $1,
    updated_at = NOW()
WHERE id = 1
"#,
        )
        .bind(finalized_height)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use sqlx::{postgres::PgPoolOptions, Executor, PgPool};

    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../db/migrations");

    async fn setup_pool() -> Result<Option<PgPool>> {
        let url =
            match std::env::var("TEST_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL")) {
                Ok(url) => url,
                Err(_) => return Ok(None),
            };

        let pool = match PgPoolOptions::new().max_connections(1).connect(&url).await {
            Ok(pool) => pool,
            Err(err) => {
                eprintln!("skipping checkpoint test: failed to connect to {url}: {err}");
                return Ok(None);
            }
        };

        if let Err(err) = MIGRATOR.run(&pool).await {
            eprintln!("skipping checkpoint test: failed to run migrations: {err}");
            return Ok(None);
        }

        Ok(Some(pool))
    }

    #[tokio::test]
    async fn checkpoint_roundtrip() -> Result<()> {
        let Some(pool) = setup_pool().await? else {
            eprintln!("checkpoint_roundtrip skipped (set TEST_DATABASE_URL to run)");
            return Ok(());
        };

        if let Err(err) = pool.execute("DELETE FROM ingestor_checkpoint").await {
            eprintln!("skipping checkpoint test: cleanup failed: {err}");
            return Ok(());
        }

        let checkpoint = Checkpoint::new(pool.clone());
        let initial = checkpoint.get_state().await?;
        assert_eq!(initial.ingested_height, 0);
        assert_eq!(initial.finalized_height, 0);

        checkpoint.set(42, 21).await?;
        let mid = checkpoint.get_state().await?;
        assert_eq!(mid.ingested_height, 42);
        assert_eq!(mid.finalized_height, 21);

        checkpoint.set(1337, 1300).await?;
        let end = checkpoint.get_state().await?;
        assert_eq!(end.ingested_height, 1337);
        assert_eq!(end.finalized_height, 1300);

        Ok(())
    }
}
