# Ingestor Flags

- `--ingest-concurrency` / `INGEST_CONCURRENCY` (default: 8)  \
  Parallelism for transaction fetch & processing.

- `--rpc-requests-per-second` / `RPC_RPS` (default: 10)  \
  Global RPC rate limit. In `--bootstrap` this is multiplied by 2.5.

- `--bootstrap` / `BOOTSTRAP=true|false` (default: false)  \
  Enables fastest initial sync: raises RPS & concurrency and disables analytics (soft_facts) for now.
