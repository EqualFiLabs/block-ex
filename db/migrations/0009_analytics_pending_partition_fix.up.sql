ALTER TABLE public.blocks ADD COLUMN IF NOT EXISTS analytics_pending BOOLEAN NOT NULL DEFAULT TRUE;
CREATE INDEX IF NOT EXISTS idx_blocks_analytics_pending ON public.blocks (analytics_pending) WHERE analytics_pending;
