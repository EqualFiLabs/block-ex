# ADR 0002: High-Throughput Ingestion Strategy

## Status
Proposed

## Context

The current ingestor processes roughly one block per second on the local
stagenet. That throughput is acceptable for live tailing, but it would require
many weeks to hydrate a fresh Postgres instance against the full Monero mainnet
(>3.5M blocks with dense transaction activity). The existing loop performs work
serially per block, enforces a hard-coded rate limit of 10 JSON-RPC requests per
second, and computes analytical “soft facts” for every block inline. Those
choices keep the daemon safe but prevent practical bootstrap times.

We want an approach that reaches production-scale history quickly while keeping
the live, confirmation-aware design introduced in ADR-0001. The improvements
should compose cleanly and still allow a conservative, daemon-friendly mode
once the database is caught up.

## Decision

Adopt a multi-pronged strategy to accelerate ingestion:

1. **Configurable rate limiting and concurrency.** Expose the RPC request quota
   and ingestion concurrency as configuration (CLI flags + env vars). Introduce
   a “bootstrap” mode that raises limits or temporarily disables throttling,
   while retaining a safer default for steady-state.
2. **Pipelined ingestion workers.** Restructure the ingestor loop into stages
   (header/block fetch, transaction fetch, decode/analysis, persistence) so
   multiple heights can be processed concurrently. Keep a single DB writer stage
   to preserve ordering guarantees.
3. **Bulk/efficient RPC usage.** Where Monero exposes batch endpoints (e.g.
   `/get_transactions`, `get_block_headers_range`, `/get_blocks_by_height.bin`),
   prefer them over per-block requests. Decode binary payloads locally to reduce
   round trips and JSON parsing overhead.
4. **Deferred analytics during bootstrap.** Allow expensive analyses (soft-fact
   aggregation, deep transaction metrics) to be skipped while backfilling. A
   follow-up job can recompute the data once the height catches the network tip.
5. **Optional snapshot ingestion.** Keep an opt-in path to hydrate the database
   from a prebuilt snapshot or LMDB export. This is a last-resort escape hatch
   when RPC-based catch-up is still too slow or infra-constrained.

## Details

### Rate limiting and configuration

- Replace the hard-coded `Quota::per_second(10)` with a configurable quota that
  can be tuned per environment.
- Add a boolean `--bootstrap` flag (and matching env var) that sets defaults to
  aggressive throughput and enables deferred analytics.
- Ensure the rate limiter still guards the daemon in steady-state by falling
  back to conservative defaults when `--bootstrap` is off.

### Pipelined architecture

- Split the ingestion workflow into separate async tasks linked via bounded
  channels:
  1. Height scheduler reads checkpoints and enqueues missing heights.
  2. Fetch workers grab headers/blocks in parallel and enqueue decoded blocks.
  3. Transaction workers resolve `tx_hashes` using bulk RPC calls.
  4. A single persistence worker writes blocks/transactions to Postgres.
- Use back-pressure (channel capacity) to prevent unbounded memory growth.
- Keep reorg handling and finality tracking in the writer so ordering remains
  deterministic.

### Bulk RPC adoption

- Continue batching `/get_transactions` requests (<=100 hashes). Detect daemon
  limits and adjust chunk sizes dynamically.
- Evaluate using `get_block_headers_range` for header fetches and
  `/get_blocks_by_height.bin` for block bodies to reduce request count and
  payload size.
- Handle legacy/stagenet nodes gracefully by feature-detecting supported
  endpoints at startup and falling back when necessary.

### Deferred analytics

- Gate the expensive `analyze_tx` and `Store::upsert_soft_facts_for_block`
  calls behind the bootstrap flag.
- Implement a background job (or CLI subcommand) that recomputes analytics for a
  given height range once bootstrap completes.
- Record whether analytics were skipped so the follow-up job can resume
  accurately.

### Snapshot ingestion (optional)

- Define an interface to import precomputed state (e.g. SQL dump, LMDB export)
  into the same schema the ingestor expects.
- Treat snapshot bootstrap as a separate entry point that still relies on the
  persistence layer for consistency checks.
- Document operational requirements (trusted snapshot, schema version parity).

## Consequences

- Bootstrap mode can drive the daemon harder, so observability must track RPC
  error rates, queue depths, and long-tail latencies to dial settings safely.
- The ingestor codebase becomes more modular, making it easier to add further
  pipeline stages or dynamic tuning in the future.
- Deferred analytics introduce eventual consistency; dashboards must tolerate
  temporarily missing soft facts until the catch-up job runs.
- Snapshot ingestion adds maintenance overhead (snapshot generation, validation)
  but remains optional and isolated from the core path.

## Follow-up

- Implement CLI/environment knobs for rate limiting and bootstrap behavior.
- Refactor the ingestion loop into staged workers with bounded channels.
- Investigate and integrate suitable bulk RPC endpoints across supported daemon
  versions.
- Add a background analytics backfill job and tracking for skipped blocks.
- Design and document the snapshot import workflow, ensuring it reuses existing
  store APIs.
