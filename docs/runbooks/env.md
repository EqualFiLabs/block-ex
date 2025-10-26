# Environment Variables

These variables configure the ingestor and API. Copy `.env.example` to `.env` and adjust as needed.

## Required

- `DATABASE_URL`  
  Example: `postgres://explorer:explorer@localhost:5432/explorer`  
  Postgres 16 connection string.

- `XMR_RPC_URL`  
  Monero JSON-RPC endpoint (stagenet/mainnet). Example: `http://monerod:38081/json_rpc`.

- `XMR_ZMQ_URL`  
  Monero ZMQ pub endpoint for mempool and block notifications. Example: `tcp://monerod:38082`.

- `FINALITY_WINDOW`  
  Number of blocks to keep as a rollback window for safe reorg handling. Default: `30`.

- `NETWORK`  
  One of: `mainnet`, `stagenet`, `devnet`. Default: `stagenet`.

## Optional

- `REDIS_URL`  
  Redis connection string for API caching. Default: `redis://redis:6379` in Docker; `redis://127.0.0.1:6379` locally.
- `BOOTSTRAP`  
  Set to `true` to enable the high-throughput mode (higher RPC rate limit, doubled concurrency, analytics deferred). Leave `false` in steady state.
- `INGEST_CONCURRENCY`  
  Number of concurrent block/tx workers. Default: `8`. Increase (e.g., `16`) when bootstrapping a fresh database.
- `RPC_RPS`  
  Maximum JSON-RPC requests per second enforced by the ingestor’s rate limiter. Default: `10`. Raise cautiously when running against local daemons you control.
- `START_HEIGHT` / `LIMIT`  
  Optional lower bound and block count for partial syncs; mostly useful for diagnostics or replaying a small window.

## Usage

```bash
# Local
cp .env.example .env
# Edit values as needed
```

In Docker Compose, these are usually provided via service `environment` blocks. See `ops/docker-compose.yml`.
