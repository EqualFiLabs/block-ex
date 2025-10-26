-- A ring per input; ring_members link inputs to referenced outputs by global index
CREATE TABLE IF NOT EXISTS public.rings (
  tx_hash        BYTEA    NOT NULL,
  input_idx      INTEGER  NOT NULL,
  ring_index     INTEGER  NOT NULL,
  referenced_output_id BIGINT NOT NULL, -- FK to outputs.output_id (not global_index)
  PRIMARY KEY (tx_hash, input_idx, ring_index),
  CONSTRAINT fk_rings_tx_input FOREIGN KEY (tx_hash, input_idx)
    REFERENCES public.tx_inputs (tx_hash, idx) ON DELETE CASCADE,
  CONSTRAINT fk_rings_output FOREIGN KEY (referenced_output_id)
    REFERENCES public.outputs (output_id) ON DELETE RESTRICT
);

-- Optional denormalized table (if you want both)
-- Note: You may omit ring_members if rings already hold member rows.
CREATE TABLE IF NOT EXISTS public.ring_members (
  tx_hash        BYTEA    NOT NULL,
  input_idx      INTEGER  NOT NULL,
  member_pos     INTEGER  NOT NULL,
  global_index   BIGINT   NULL,   -- convenience when you have it
  output_id      BIGINT   NULL,   -- same as referenced_output_id when known
  PRIMARY KEY (tx_hash, input_idx, member_pos)
);

-- Mempool transactions (shadow rows while txs.in_mempool=TRUE)
CREATE TABLE IF NOT EXISTS public.mempool_txs (
  tx_hash        BYTEA       PRIMARY KEY,
  first_seen     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_seen      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  fee_rate       NUMERIC     NULL, -- fee per byte
  relayed_by     TEXT        NULL
);

-- Optional roll-up
CREATE TABLE IF NOT EXISTS public.mempool_tx_stats (
  tx_hash        BYTEA       NOT NULL,
  stat_ts        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  fee_rate       NUMERIC     NULL,
  size_bytes     INTEGER     NULL,
  PRIMARY KEY (tx_hash, stat_ts),
  CONSTRAINT fk_mpool_stats_tx FOREIGN KEY (tx_hash) REFERENCES public.mempool_txs(tx_hash) ON DELETE CASCADE
);

-- FK to keep mempool and txs aligned (soft link – no cascade)
ALTER TABLE public.txs
  ADD CONSTRAINT chk_mempool_flag
  CHECK ((in_mempool = TRUE AND block_height IS NULL AND block_timestamp IS NULL)
         OR (in_mempool = FALSE));
