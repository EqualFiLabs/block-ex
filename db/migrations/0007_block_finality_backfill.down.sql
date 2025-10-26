ALTER TABLE public.blocks
  DROP COLUMN IF EXISTS confirmations;

ALTER TABLE public.blocks
  DROP COLUMN IF EXISTS is_final;

ALTER TABLE ingestor_checkpoint
  DROP COLUMN IF EXISTS finalized_height;
