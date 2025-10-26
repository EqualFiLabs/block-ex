ALTER TABLE IF EXISTS public.txs DROP CONSTRAINT IF EXISTS chk_mempool_flag;

DROP TABLE IF EXISTS public.mempool_tx_stats;
DROP TABLE IF EXISTS public.mempool_txs;

DROP TABLE IF EXISTS public.ring_members;
DROP TABLE IF EXISTS public.rings;
