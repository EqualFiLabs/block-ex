-- migrate:up
ALTER TABLE public.blocks ADD COLUMN IF NOT EXISTS analytics_pending BOOLEAN NOT NULL DEFAULT TRUE;
CREATE INDEX IF NOT EXISTS idx_blocks_analytics_pending ON public.blocks (analytics_pending) WHERE analytics_pending;

-- migrate:down
DROP INDEX IF EXISTS idx_blocks_analytics_pending;
ALTER TABLE public.blocks DROP COLUMN IF EXISTS analytics_pending;
