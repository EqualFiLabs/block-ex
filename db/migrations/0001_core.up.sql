-- migrate:up

-- Extensions we rely on (safe if already installed)
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ===== Core: Blocks & TXs =====
-- Blocks are partitioned by block_timestamp (timestamptz), monthly
CREATE TABLE IF NOT EXISTS public.blocks (
  height               BIGINT        NOT NULL,
  hash                 BYTEA         NOT NULL,
  prev_hash            BYTEA         NOT NULL,
  block_timestamp      TIMESTAMPTZ   NOT NULL,
  size_bytes           INTEGER       NOT NULL,
  major_version        INTEGER       NOT NULL,
  minor_version        INTEGER       NOT NULL,
  nonce                BIGINT        NOT NULL,
  tx_count             INTEGER       NOT NULL,
  reward_nanos         BIGINT        NOT NULL,
  CONSTRAINT pk_blocks PRIMARY KEY (block_timestamp, height),
  CONSTRAINT uq_blocks_hash UNIQUE (block_timestamp, hash)
) PARTITION BY RANGE (block_timestamp);

-- Create a default partition to catch out-of-range data
CREATE TABLE IF NOT EXISTS public.blocks_default PARTITION OF public.blocks DEFAULT;

-- Transactions are partitioned by block_timestamp as well
CREATE TABLE IF NOT EXISTS public.txs (
  tx_hash              BYTEA         NOT NULL,
  block_height         BIGINT        NULL,
  block_timestamp      TIMESTAMPTZ   NOT NULL DEFAULT 'infinity',
  in_mempool           BOOLEAN       NOT NULL DEFAULT FALSE,
  fee_nanos            BIGINT        NULL,
  size_bytes           INTEGER       NOT NULL,
  version              INTEGER       NOT NULL,
  unlock_time          BIGINT        NOT NULL,
  extra                JSONB         NOT NULL DEFAULT '{}'::jsonb,
  rct_type             INTEGER       NOT NULL,
  proof_type           TEXT          NULL,
  bp_plus              BOOLEAN       NOT NULL DEFAULT TRUE,
  num_inputs           INTEGER       NOT NULL,
  num_outputs          INTEGER       NOT NULL,
  received_ts          TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
  CONSTRAINT pk_txs PRIMARY KEY (block_timestamp, tx_hash)
) PARTITION BY RANGE (block_timestamp);

CREATE TABLE IF NOT EXISTS public.txs_default PARTITION OF public.txs DEFAULT;

-- Inputs per transaction (non-partitioned)
CREATE TABLE IF NOT EXISTS public.tx_inputs (
  tx_hash              BYTEA         NOT NULL,
  tx_block_timestamp   TIMESTAMPTZ   NOT NULL DEFAULT 'infinity',
  idx                  INTEGER       NOT NULL,
  key_image            BYTEA         NOT NULL,
  ring_size            INTEGER       NOT NULL,
  pseudo_out           BYTEA         NULL,
  PRIMARY KEY (tx_hash, idx),
  CONSTRAINT fk_tx_inputs_tx FOREIGN KEY (tx_block_timestamp, tx_hash)
    REFERENCES public.txs(block_timestamp, tx_hash)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

-- Outputs table (global index + producer linkage)
CREATE TABLE IF NOT EXISTS public.outputs (
  output_id            BIGSERIAL     PRIMARY KEY,
  global_index         BIGINT        UNIQUE,
  tx_hash              BYTEA         NOT NULL,
  tx_block_timestamp   TIMESTAMPTZ   NOT NULL DEFAULT 'infinity',
  idx_in_tx            INTEGER       NOT NULL,
  commitment           BYTEA         NOT NULL,
  amount               NUMERIC       NULL,
  stealth_public_key   BYTEA         NOT NULL,
  spent_by_key_image   BYTEA         NULL,
  spent_in_tx          BYTEA         NULL,
  CONSTRAINT fk_outputs_tx FOREIGN KEY (tx_block_timestamp, tx_hash)
    REFERENCES public.txs(block_timestamp, tx_hash)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

-- ===== Indexes =====
CREATE INDEX IF NOT EXISTS idx_blocks_height ON public.blocks (height);
CREATE INDEX IF NOT EXISTS idx_txs_block_height ON public.txs (block_height);
CREATE INDEX IF NOT EXISTS idx_tx_inputs_key_image ON public.tx_inputs (key_image);
CREATE INDEX IF NOT EXISTS idx_outputs_global_index ON public.outputs (global_index);
