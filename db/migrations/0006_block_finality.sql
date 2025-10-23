-- migrate:up

ALTER TABLE public.blocks
  ADD COLUMN IF NOT EXISTS confirmations INTEGER NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS is_final BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE ingestor_checkpoint
  ADD COLUMN IF NOT EXISTS finalized_height BIGINT NOT NULL DEFAULT 0;

-- Ensure existing rows inherit defaults
UPDATE public.blocks SET confirmations = COALESCE(confirmations, 0), is_final = COALESCE(is_final, false);
UPDATE ingestor_checkpoint SET finalized_height = COALESCE(finalized_height, 0);

-- migrate:down

ALTER TABLE public.blocks
  DROP COLUMN IF EXISTS confirmations,
  DROP COLUMN IF EXISTS is_final;

ALTER TABLE ingestor_checkpoint
  DROP COLUMN IF EXISTS finalized_height;
