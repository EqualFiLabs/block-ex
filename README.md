# Block Explorer Monorepo

This repository contains the scaffolding for a modular blockchain explorer comprised of Rust back-end services and a modern React front end. The codebase is organized as a mono-repo under the `explorer/` directory.

## Directory structure


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
