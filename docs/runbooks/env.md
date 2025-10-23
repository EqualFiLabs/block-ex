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

## Usage

```bash
# Local
cp .env.example .env
# Edit values as needed
```

In Docker Compose, these are usually provided via service `environment` blocks. See `ops/docker-compose.yml`.
