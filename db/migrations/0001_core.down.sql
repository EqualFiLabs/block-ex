DROP INDEX IF EXISTS idx_outputs_global_index;
DROP INDEX IF EXISTS idx_tx_inputs_key_image;
DROP INDEX IF EXISTS idx_txs_block_height;
DROP INDEX IF EXISTS idx_blocks_height;

DROP TABLE IF EXISTS public.outputs;
DROP TABLE IF EXISTS public.tx_inputs;

DROP TABLE IF EXISTS public.txs_default;
DROP TABLE IF EXISTS public.txs;

DROP TABLE IF EXISTS public.blocks_default;
DROP TABLE IF EXISTS public.blocks;

DROP EXTENSION IF EXISTS "uuid-ossp";
