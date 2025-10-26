CREATE TABLE IF NOT EXISTS public.chain_tips (
  height BIGINT PRIMARY KEY,
  hash   BYTEA NOT NULL,
  prev_hash BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_chain_tips_height ON public.chain_tips (height);
