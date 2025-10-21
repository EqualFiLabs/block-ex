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

## Running the local infrastructure stack

The repository includes a Docker Compose stack under `ops/docker-compose.yml` that orchestrates Monero node access alongside the
API, ingestor, PostgreSQL, Redis, and web front end services. The top-level `Makefile` exposes convenience targets for working
with this stack:

- `make build-images` – builds the development images defined in `ops/Dockerfile.*`.
- `make up` – builds (if necessary) and starts the stack in detached mode.
- `make down` – stops the stack and removes associated volumes.
- `make health` – displays the health status reported by Docker for each service.

> **Prerequisites:** Docker Engine with Compose v2 support is required. When running inside a constrained container or VM,
> ensure the Docker daemon is available and that networking capabilities (e.g., `CAP_NET_ADMIN`) are enabled; otherwise, the
> services cannot be started.
