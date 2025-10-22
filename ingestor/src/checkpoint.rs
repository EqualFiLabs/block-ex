use anyhow::Result;
use sqlx::{PgPool, Row};

pub struct Checkpoint {
    pool: PgPool,
}

impl Checkpoint {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self) -> Result<i64> {
        let rec = sqlx::query("SELECT last_height FROM ingestor_checkpoint WHERE id=$1")
            .bind(1i32)
            .fetch_optional(&self.pool)
            .await?;

        let height = rec
            .map(|row| row.try_get::<i64, _>("last_height"))
            .transpose()?
            .unwrap_or(0);

        Ok(height)
    }

    pub async fn set(&self, h: i64) -> Result<()> {
        sqlx::query(
            r#"
INSERT INTO ingestor_checkpoint (id, last_height, updated_at)
VALUES (1, $1, NOW())
ON CONFLICT (id)
DO UPDATE SET last_height = EXCLUDED.last_height,
              updated_at = NOW()
"#,
        )
        .bind(h)
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
        assert_eq!(checkpoint.get().await?, 0);

        checkpoint.set(42).await?;
        assert_eq!(checkpoint.get().await?, 42);

        checkpoint.set(1337).await?;
        assert_eq!(checkpoint.get().await?, 1337);

        Ok(())
    }
}
