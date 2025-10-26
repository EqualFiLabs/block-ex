DROP INDEX IF EXISTS idx_soft_facts_fee;
DROP INDEX IF EXISTS idx_soft_facts_ts;

DROP TABLE IF EXISTS public.soft_facts;

-- pg_partman config removal (non-destructive; leaves existing partitions)
DROP EXTENSION IF EXISTS pg_partman;
DROP SCHEMA IF EXISTS partman CASCADE;
