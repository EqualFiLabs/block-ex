DROP INDEX IF EXISTS idx_blocks_analytics_pending;
ALTER TABLE public.blocks DROP COLUMN IF EXISTS analytics_pending;
