# ADR 0001: Live Block Ingestion with Finality Tracking

## Status
Accepted

## Context

The ingestor previously throttled block imports until a block was considered
"final" by subtracting `FINALITY_WINDOW` from the daemon tip. In shallow test
networks this meant the explorer always appeared several blocks behind the
latest chain height. Additionally, soft-fact updates and UI consumers had no
persisted notion of block confirmations, and the JSON-RPC call we used for
mempool priming (`get_transaction_pool_hashes`) is not available on the stagenet
node – only the legacy REST endpoint exists.

## Decision

- Ingest every block as soon as it is announced. Track confirmations and
  finality in the database instead of pausing the pipeline.
- Extend the checkpoint to persist both the highest ingested height and the
  highest finalized height so restart logic can resume correctly.
- Store per-block confirmation counts and an `is_final` flag on
  `public.blocks`, refreshing a sliding window after each block commit.
- Use the REST `/get_transaction_pool_hashes` endpoint for mempool seeding.
- Document the approach so future changes understand the separation between
  "ingested" and "final" states.

## Details

### Database

- Added `confirmations INTEGER NOT NULL DEFAULT 0` and `is_final BOOLEAN NOT
  NULL DEFAULT false` columns to `public.blocks` via migrations 0006/0007.
- Added `finalized_height BIGINT NOT NULL DEFAULT 0` to `ingestor_checkpoint`.
- `Store` provides helpers to update confirmations inside the block
  transaction and to refresh a trailing confirmation window after commits.

### Runtime loop

- The main loop no longer subtracts the finality window from the daemon tip.
  Instead it fetches the live chain tip, waits only when the next height
  exceeds the tip, and otherwise processes the block immediately.
- Post-ingest we compute confirmations using the current tip, set `is_final`
  when the block height is ≤ `finalized_height`, and refresh a sliding window
  (finality window + 16) to keep recent confirmation counts accurate.
- The checkpoint now stores `(ingested_height, finalized_height)` meaning the
  ingestor can restart without losing whether older blocks were finalized.

### Mempool

- `Rpc::get_transaction_pool_hashes` now issues a GET to
  `<base>/get_transaction_pool_hashes` and the watcher reuses it so startup no
  longer logs spurious “method not found”.

## Consequences

- UI can surface new blocks instantly with confirmation counts updating as we
  learn about new tips. A block becomes "final" once its confirmations exceed
  `FINALITY_WINDOW` (exposed via `is_final`).
- Finality window still bounds reorg healing (`heal_reorg`) but no longer
  throttles ingestion throughput.
- Because confirmation counts depend on the latest tip, very old rows may have
  stale `confirmations` values. The sliding window keeps recent data accurate
  while avoiding large table updates. Consumers needing exact counts for deep
  history can recompute from the chain tip on demand.
- Introducing new columns requires the migrations to run before starting the
  updated ingestor.
