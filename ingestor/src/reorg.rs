use anyhow::{anyhow, Context, Result};
use hex::decode;

use crate::{rpc::Rpc, store::Store};

pub async fn heal_reorg(
    start_height: i64,
    store: &Store,
    rpc: &Rpc,
    finality_window: i64,
) -> Result<()> {
    let mut h = start_height - 1;
    let mut steps = 0_i64;

    let mut db_hash_at_h = store
        .block_hash_at(h)
        .await?
        .ok_or_else(|| anyhow!("no db block at height {}", h))?;

    loop {
        if steps > finality_window {
            return Err(anyhow!(
                "reorg exceeds FINALITY_WINDOW={} ({} steps)",
                finality_window,
                steps
            ));
        }

        let live_hdr = rpc
            .get_block_header_by_height(h as u64)
            .await
            .with_context(|| format!("fetch header at height {}", h))?;
        let live_hash = decode(&live_hdr.block_header.hash)
            .with_context(|| format!("decode live hash at height {}", h))?;

        if live_hash == db_hash_at_h {
            break;
        }

        h -= 1;
        steps += 1;
        db_hash_at_h = store
            .block_hash_at(h)
            .await?
            .ok_or_else(|| anyhow!("no db block at height {}", h))?;
    }

    let fork_height = h + 1;
    tracing::warn!(
        fork_height = fork_height,
        steps_back = steps,
        "healing reorg"
    );

    let mut tx = store
        .pool()
        .begin()
        .await
        .context("begin reorg healing transaction")?;

    for height in fork_height..start_height {
        Store::requeue_mempool_from_block(&mut tx, height)
            .await
            .with_context(|| format!("requeue mempool at height {}", height))?;
    }

    sqlx::query!(
        "DELETE FROM public.chain_tips WHERE height >= $1",
        fork_height
    )
    .execute(&mut *tx)
    .await
    .with_context(|| "delete chain tips".to_string())?;

    sqlx::query!("DELETE FROM public.blocks WHERE height >= $1", fork_height)
        .execute(&mut *tx)
        .await
        .with_context(|| "delete blocks".to_string())?;

    tx.commit().await?;

    Ok(())
}
