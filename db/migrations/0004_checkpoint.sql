-- migrate:up
CREATE TABLE IF NOT EXISTS ingestor_checkpoint (
  id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id=1),
  last_height BIGINT NOT NULL DEFAULT 0,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO ingestor_checkpoint (id, last_height) VALUES (1, 0)
ON CONFLICT (id) DO NOTHING;

-- migrate:down
DROP TABLE IF EXISTS ingestor_checkpoint;
