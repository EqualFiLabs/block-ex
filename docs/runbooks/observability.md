# Observability

## Metrics endpoint

The ingestor exposes Prometheus metrics on `http://<host>:9898/metrics`. The
endpoint is enabled automatically on startup and can be scraped by Prometheus or
any compatible collector.

## Exported metrics

- `queue_depth` (gauge): depth of internal worker queues. The `queue` label is
  one of `sched`, `block`, or `tx` and corresponds to the scheduler, block
  processing, and transaction persistence stages respectively.
- `rpc_errors_total` (counter): RPC failure counter, partitioned by Monero RPC
  method via the `method` label. Increases whenever a JSON-RPC or REST request
  fails or returns a non-OK status.
- `block_process_ms` (histogram): end-to-end latency from scheduling a block
  until it is persisted. Useful for detecting backpressure during spikes.

## Grafana dashboard ideas

A starter dashboard can include the following panels:

1. **Queue backlog**: graph `queue_depth{queue="sched"}` (and the other queues)
   to watch for persistent backlog or saturation.
2. **RPC errors/sec**: rate-convert `rpc_errors_total` to highlight upstream RPC
   instability (`increase(rpc_errors_total[5m])` or `rate` variants).
3. **Block processing latency**: heatmap or percentile panel on
   `histogram_quantile(0.95, rate(block_process_ms_bucket[5m]))` to spot slow
   commits.

Tune alert thresholds based on normal operating ranges for your deployment.
