-- Hybrid semantic retrieval for memory items (roadmap 6.25).
-- Local feature-hash embeddings stored as float arrays (no pgvector required).

ALTER TABLE memory_items
    ADD COLUMN IF NOT EXISTS embedding real[],
    ADD COLUMN IF NOT EXISTS embedding_version integer NOT NULL DEFAULT 0;

COMMENT ON COLUMN memory_items.embedding IS 'Unit L2 feature-hash embedding for hybrid retrieval';
COMMENT ON COLUMN memory_items.embedding_version IS 'Encoder version; 0 means missing/stale';
