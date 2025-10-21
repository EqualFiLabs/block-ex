# Database Stack (PostgreSQL 16 + sqlx + pg_partman)

This project uses:
- **PostgreSQL 16** for strong consistency, native partitioning, and mature tooling.
- **sqlx** for compile-time checked SQL and a simple migrations story.
- **pg_partman** to automate creation/retention of **time-based monthly partitions** for heavy tables.

## Why pg_partman?
- Native PG partitions are great, but **rotating monthly partitions** and **retention** is tedious.
- `pg_partman` automates: creating the next month's partitions ahead of time, running maintenance, and (optionally) pruning old partitions. We keep retention **off** for now; only automated creation is enabled.

## Partitioning strategy
- `blocks` and `txs` are **declaratively partitioned by timestamp** from day one.
- `pg_partman` is attached later to auto-create monthly partitions.
- All other tables remain non-partitioned for simplicity.

## Operational notes
- `pg_partman` runs as a normal extension. To pre-create upcoming partitions or maintain them periodically:
  - `SELECT partman.run_maintenance();`
- We do **not** enable retention in production until logs/backups are in place.
