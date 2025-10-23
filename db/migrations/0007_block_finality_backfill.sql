-- migrate:up

ALTER TABLE public.blocks
  ADD COLUMN IF NOT EXISTS confirmations INTEGER DEFAULT 0;

ALTER TABLE public.blocks
  ADD COLUMN IF NOT EXISTS is_final BOOLEAN DEFAULT false;

ALTER TABLE public.blocks
  ALTER COLUMN confirmations SET NOT NULL;

ALTER TABLE public.blocks
  ALTER COLUMN is_final SET NOT NULL;

UPDATE public.blocks SET confirmations = COALESCE(confirmations, 0), is_final = COALESCE(is_final, false);

ALTER TABLE ingestor_checkpoint
  ADD COLUMN IF NOT EXISTS finalized_height BIGINT DEFAULT 0;

ALTER TABLE ingestor_checkpoint
  ALTER COLUMN finalized_height SET NOT NULL;

UPDATE ingestor_checkpoint SET finalized_height = COALESCE(finalized_height, 0);

-- migrate:down

ALTER TABLE public.blocks
  DROP COLUMN IF EXISTS confirmations;

ALTER TABLE public.blocks
  DROP COLUMN IF EXISTS is_final;

ALTER TABLE ingestor_checkpoint
  DROP COLUMN IF EXISTS finalized_height;
