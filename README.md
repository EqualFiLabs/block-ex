# Block Explorer Monorepo

This repository contains the scaffolding for a modular blockchain explorer comprised of Rust back-end services and a modern React front end. The codebase is organized as a mono-repo under the `explorer/` directory.

## Directory structure

- `explorer/api/` – Axum-based HTTP API service exposing explorer data.
- `explorer/ingestor/` – Rust daemons responsible for synchronizing blockchain data:
  - `block/` – Handles block ingestion from the network.
  - `mempool/` – Tracks unconfirmed transactions.
  - `reorg-sentinel/` – Monitors and responds to chain reorganizations.
- `explorer/web/` – Vite + React + Tailwind v4 single-page application (scaffolded).
- `explorer/db/` – Database migrations, seeds, and operational runbooks.
- `explorer/ops/` – Operational assets such as Dockerfiles, Compose files, Makefile, and Grafana dashboards.
- `explorer/docs/` – Architectural decision records and additional runbooks.

Additional scaffolding such as service manifests, migrations, and implementation code will be added in subsequent iterations.
