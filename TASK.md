# 0) Bootstrap & Repo Hygiene

1. **Create mono-repo scaffold**

   * Structure:

     ```
     explorer/
       api/        # Rust (Axum)
       ingestor/   # Rust daemons: block, mempool, reorg sentinel
       web/        # Vite + React + Tailwind v4
       db/         # migrations, seeds, runbooks
       ops/        # Dockerfiles, compose, Makefile, Grafana dashboards
       docs/       # ADRs, runbooks
     ```
   * DoD: Repo exists with directories, `README.md` (high-level), root `LICENSE`, `.editorconfig`, `.gitignore`.

2. **Rust & Node toolchain pins**

   * Files: `rust-toolchain.toml` (stable), `Cargo.toml` (workspace), `package.json` (root scripts), `.nvmrc`.
   * DoD: `cargo build -p ingestor -p api` and `npm --version` runs; versions documented in `docs/dev-setup.md`.

3. **Docker base images**

   * Create minimal Dockerfiles:

     * `ops/Dockerfile.api` (Rust distroless/ubi-micro)
     * `ops/Dockerfile.ingestor`
     * `ops/Dockerfile.web` (Node build → static assets served by `caddy`/`nginx`)
   * DoD: `docker build` succeeds for each; images run `--help` without crashing.

4. **Compose & Makefile**

   * `ops/docker-compose.yml`: `monerod`, `postgres`, `redis`, `api`, `ingestor`, `web` (ClickHouse optional, commented).
   * `Makefile`: `bootstrap`, `migrate`, `up`, `down`, `logs`, `psql`, `tail`.
   * DoD: `make up` brings up stack; containers healthy (except api/ingestor until config).

---

# 1) Database Groundwork

5. **Choose schema libs & extensions**

   * Pick `sqlx` (runtime-checked SQL); enable Postgres 16; install `pg_partman`.
   * DoD: `db/README.md` explains extensions and why.

6. **Migration 0001 — Core tables**

   * `blocks`, `txs`, `tx_inputs`, `outputs`, indexes on `blocks.height`, `txs.block_height`, `tx_inputs.key_image`, `outputs.global_index`.
   * DoD: `make migrate` applies successfully; `SELECT` from tables works.

7. **Migration 0002 — Rings & mempool**

   * `rings`, `ring_members`, `mempool_txs`, `mempool_tx_stats`, foreign keys, sensible cascades.
   * DoD: Schema validates; FK constraints enforced.

8. **Migration 0003 — Soft facts & partitions**

   * `soft_facts` (per block aggregates) + pg_partman partitioning (by month) for `blocks` and `txs`.
   * DoD: Partition parent/child tables exist; `partman.run_maintenance()` documented.

9. **DB user & secrets**

   * `.env.example` with `DATABASE_URL`, `XMR_RPC_URL`, `XMR_ZMQ_URL`, `FINALITY_WINDOW`, `NETWORK`.
   * DoD: `docs/runbooks/env.md` lists all env vars & defaults.

---

# 2) Ingestor: Linear Sync (Blocks + TXs)

10. **RPC client module**

    * `ingestor/src/rpc.rs`: typed calls for `get_block_header_by_height`, `get_block`, `get_transactions`.
    * DoD: Unit test hits stagenet RPC (behind feature flag `INTEGRATION`).

11. **Codec module (parsers)**

    * `tx_extra` parser, CLSAG/BP+ counters, commitment extraction, sizes.
    * DoD: Golden-vector unit tests for at least 3 stagenet blocks.

12. **Store module (sqlx)**

    * Batched inserts with `ON CONFLICT DO NOTHING` where appropriate; transaction boundaries per block.
    * DoD: Syncing first 500 heights populates `blocks` + `txs` deterministically.

13. **Checkpoint & resume**

    * `ingestor.checkpoint` table with `last_height`; resume on restart.
    * DoD: Kill/restart daemon resumes without data loss.

14. **Backfill driver**

    * Concurrency knob (e.g., 8 parallel `get_transactions`), rate-limit to respect monerod.
    * DoD: Backfills a stagenet slice (e.g., 10k blocks) without errors; idempotent reruns.

---

# 3) Mempool Watcher & Reorg Sentinel

15. **ZMQ subscriber**

    * `ingestor/src/mempool.rs`: subscribe `raw_tx`, `raw_block`; decode, stash mempool txs with `first_seen`.
    * DoD: New mempool tx appears in `mempool_txs` within seconds.

16. **Inclusion & eviction logic**

    * On block arrival: mark included txs, delete from mempool tables.
    * DoD: Ingest → inclusion removes entries; orphan puts back (next task).

17. **Reorg detection**

    * Track `chain_tips` table; verify `prev_hash` each height; detect divergence.
    * DoD: Synthetic fork test (script) shows detection at the first mismatched parent.

18. **Reorg healing**

    * Walk back to common ancestor ≤ `FINALITY_WINDOW`; single SQL transaction: delete rows ≥ fork height, restore mempool entries, replay.
    * DoD: Automated test simulates 3-block reorg; DB returns to consistent state and resumes tailing.

19. **Soft facts computation**

    * After each block commit, compute per-block aggregates into `soft_facts`.
    * DoD: `soft_facts` rows match expected sums for a reference window.

---

# 4) API v1 (Axum)

20. **Project skeleton**

    * `api/src/main.rs` with health endpoint `/healthz`; config loader.
    * DoD: `curl /healthz` → `200 OK {"status":"ok"}`.

21. **Models & DTOs**

    * Read-only structs: `BlockView`, `TxView`, `RingView`, `OutputView`, `SearchResult`.
    * DoD: Types compile; simple unit test serializes JSON.

22. **Endpoints: blocks**

    * `GET /api/v1/block/{id}` (hash or height), `GET /api/v1/blocks?start&limit`.
    * DoD: Responses validated with snapshot tests; pagination stable.

23. **Endpoints: transactions**

    * `GET /api/v1/tx/{hash}`, plus `GET /api/v1/mempool`.
    * DoD: Known stagenet tx returns inputs/outputs summary; mempool lists current items.

24. **Endpoints: rings & key images**

    * `GET /api/v1/tx/{hash}/rings`, `GET /api/v1/key_image/{hex}`.
    * DoD: A tx with N inputs returns N ring sets with ring_size entries each.

25. **Endpoint: search**

    * `GET /api/v1/search?q=` — detects height, 64-hex, key image, or global index.
    * DoD: Fuzz tests for input kinds; returns typed result or 404.

26. **Rate limits & caching**

    * `tower` middlewares: rate limit, ETag on block/tx; Redis cache for hot paths.
    * DoD: Cache hits logged; P99 < 300ms on hot endpoints in local benchmarks.

27. **OpenAPI spec**

    * `openapi.yaml` generated from annotations or hand-written; served at `/api-docs`.
    * DoD: `npx openapi-typescript openapi.yaml` generates types; CI validates schema.

---

# 5) Web UI (Vite + React + Tailwind v4)

28. **Front-end scaffold**

    * Vite React app; Tailwind v4 (no `tailwind.config`); basic layout; env for API base.
    * DoD: `npm run dev` serves a page with a header and search bar.

29. **Home (latest blocks + mempool)**

    * Cards: recent blocks table, mempool pressure, median fee.
    * DoD: Data loads from API; skeleton loaders present.

30. **Block page**

    * Header details, miner reward, tx list with pagination; “copy JSON” button.
    * DoD: Height navigation (prev/next); deep links shareable.

31. **Transaction page**

    * Summary (size, fee, version, rct type), tabs: Inputs (ring viz), Outputs, Proofs (BP+ sizes, CLSAG count).
    * DoD: Ring visual renders N members per input; disclaimer banner visible.

32. **Key image page**

    * Shows inclusion (spent tx), first seen, status.
    * DoD: Sample key image search resolves deterministically.

33. **Global stats**

    * Fee rate chart, ring size distribution, BP+ bytes time series (basic).
    * DoD: Charts render from `/soft_facts` aggregation.

34. **Error states & UX polish**

    * 404s, network errors, retry; copy-to-clipboard; monospace hex blocks.
    * DoD: Lighthouse basic pass; mobile breakpoints OK.

---

# 6) Privacy-Respecting Tools

35. **Payment proof verify (server)**

    * `POST /api/v1/verify-payment-proof` accepts inputs, returns validity only.
    * DoD: Known valid/invalid vectors pass tests; no payloads logged.

36. **Client-side view-key decode**

    * Web worker to do local decode (scaffold stub until wired to a library).
    * DoD: UI page accepts tx + view key; runs offline logic (placeholder) with clear warnings.

37. **(Optional) Server decode helper (off by default)**

    * Endpoint behind feature flag; in-memory only; strict rate limit.
    * DoD: Feature flag default OFF; turning ON works; privacy doc updated.

---

# 7) Observability, Ops & Reproducibility

38. **Prometheus metrics**

    * Ingest lag, mempool size, reorg count, API latency histograms.
    * DoD: `/metrics` exported; Grafana dashboard JSON in `ops/grafana/`.

39. **Structured logging**

    * JSON logs for api/ingestor with trace IDs.
    * DoD: `docker logs` readable; fields parse in Loki/Grafana.

40. **Runbooks**

    * `docs/runbooks/monerod.md`, `ingestion.md`, `reorg.md`, `deploy.md`.
    * DoD: Fresh machine can follow runbooks to a healthy stack.

41. **Backups**

    * Nightly `pg_dump` cron (or WAL archiving note); retention policy.
    * DoD: Restore test creates a working clone.

---

# 8) Security, Limits, and Hardening

42. **Input validation & canonical encodings**

    * Strict hex lengths, integer bounds, sanitized query params.
    * DoD: Fuzz tests across `/search` and `/key_image/*` endpoints show no panics.

43. **CORS/CSRF/Headers**

    * CORS disabled by default; CSRF on POST; security headers added.
    * DoD: `owasp-zap` baseline scan is clean.

44. **Rate-limit profiles**

    * Stricter for decode endpoints; reasonable for reads.
    * DoD: Benchmarks maintain P99 while abusive patterns get 429s.

---

# 9) FCMP/Jamtis Guardrails (Dormant Hooks)

45. **Schema fields for future proofs**

    * Add nullable columns for `proof_type` and (future) SA+L metadata; migrations written but disabled until tag.
    * DoD: Migrations present; feature flag hides UI/API fields.

46. **UI “Proof metadata” tab (placeholder)**

    * Neutral language; toggled off for mainnet.
    * DoD: Stagenet/dev toggles display stub content without errors.

---

# 10) CI/CD & Production

47. **CI pipelines**

    * GitHub Actions: Rust fmt/clippy/tests, Web typecheck/build, DB migration dry-run, OpenAPI validation.
    * DoD: PR must pass all checks; cache speeds builds.

48. **Container builds**

    * Multi-stage builds → small final images; SBOM (`syft`) exported.
    * DoD: Images published to registry with `latest` and git-sha tags.

49. **Staging environment**

    * Compose/Swarm/K8s (choose one) staging with stagenet; secrets via `.env` or vault.
    * DoD: Public staging URL, basic auth enabled, dashboards wired.

50. **Load & reorg drills**

    * Synthetic mempool flood; 5-block reorg simulation; node restart chaos test.
    * DoD: No data loss; API remains responsive; alerts trigger.

51. **Production cutover**

    * Mainnet `monerod` (remote or local), bigger Postgres instance, TLS termination, CDN for web.
    * DoD: Production URL live; status page shows healthy; on-call runbook finalized.

---

## Production Readiness Checklist (final gate)

* [ ] Full backfill on target hardware completes and tailing stays ≤1 block behind.
* [ ] Reorg simulation passes (rollback + replay) within finality window.
* [ ] API P99 < 300ms for `/block`, `/tx` at steady state; 95% cache hit for hot paths.
* [ ] Privacy posture documented; decode helper OFF by default; no secrets persisted.
* [ ] Dashboards/alerts live; backup/restore verified; disaster test documented.
* [ ] OpenAPI published; front-end consumes only documented endpoints.
* [ ] SBOMs for images stored; image signatures verified in deploy.
