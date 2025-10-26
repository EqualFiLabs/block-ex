-- Install pg_partman (safe if already installed)
CREATE SCHEMA IF NOT EXISTS partman;
CREATE EXTENSION IF NOT EXISTS pg_partman WITH SCHEMA partman;

-- Soft facts (per-block aggregation to speed charts)
CREATE TABLE IF NOT EXISTS public.soft_facts (
  block_height     BIGINT      PRIMARY KEY,
  block_timestamp  TIMESTAMPTZ NOT NULL,
  total_fee        BIGINT      NOT NULL,
  avg_ring_size    NUMERIC     NOT NULL,
  median_fee_rate  NUMERIC     NOT NULL,
  bp_total_bytes   BIGINT      NOT NULL,
  clsag_count      INTEGER     NOT NULL
);

-- Attach pg_partman to existing partitioned parents (monthly by timestamp)
-- NOTE: We use monthly partitions on the timestamp fields.
-- pg_partman expects the parent table to be declaratively partitioned by RANGE(timestamptz).
-- The following calls configure automatic creation of next partitions.

-- Configure blocks monthly partitions
SELECT partman.create_parent(
  p_parent_table := 'public.blocks',
  p_control      := 'block_timestamp',
  p_type         := 'range',
  p_interval     := '1 month',
  p_default_table := false
);

-- Configure txs monthly partitions
SELECT partman.create_parent(
  p_parent_table := 'public.txs',
  p_control      := 'block_timestamp',
  p_type         := 'range',
  p_interval     := '1 month',
  p_default_table := false
);

-- Precreate current and next month partitions so inserts never hit DEFAULT
SELECT partman.run_maintenance();

-- Helpful indexes on soft_facts
CREATE INDEX IF NOT EXISTS idx_soft_facts_ts ON public.soft_facts (block_timestamp);
CREATE INDEX IF NOT EXISTS idx_soft_facts_fee ON public.soft_facts (total_fee);
