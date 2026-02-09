-- Migration: Add enrichment tracking for incremental processing
-- This migration adds:
-- 1. enrichment_records table for tracking which memories have been processed by which engines
-- 2. Indexes for efficient lookup of unenriched memories

-- Create enrichment_records table
CREATE TABLE IF NOT EXISTS enrichment_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_scope VARCHAR(255) NOT NULL,
    memory_id UUID NOT NULL,

    -- Engine identification
    engine_kind VARCHAR(64) NOT NULL,      -- e.g., "rules", "llm:qwen"
    engine_version VARCHAR(64) NOT NULL,   -- e.g., "1.0.0"

    -- Processing metadata
    enriched_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    card_ids UUID[],                       -- Cards produced by this enrichment run
    success BOOLEAN NOT NULL DEFAULT true,
    error_message TEXT,

    -- Metadata
    extra JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_enrichment_memory
        FOREIGN KEY (memory_id)
        REFERENCES memory_items(id)
        ON DELETE CASCADE,

    CONSTRAINT unique_enrichment UNIQUE (user_scope, memory_id, engine_kind, engine_version)
);

-- Index for finding unenriched memories (most common query)
CREATE INDEX IF NOT EXISTS idx_enrichment_lookup
    ON enrichment_records(user_scope, memory_id, engine_kind, engine_version);

-- Index for getting all enrichment records for a memory
CREATE INDEX IF NOT EXISTS idx_enrichment_memory
    ON enrichment_records(user_scope, memory_id);

-- Index for time-based queries
CREATE INDEX IF NOT EXISTS idx_enrichment_time
    ON enrichment_records(user_scope, engine_kind, enriched_at DESC);

-- Index for finding failed enrichments
CREATE INDEX IF NOT EXISTS idx_enrichment_failed
    ON enrichment_records(user_scope, success, enriched_at)
    WHERE success = false;

-- Add comments
COMMENT ON TABLE enrichment_records IS 'Tracks which memory items have been processed by which enrichment engines, enabling incremental processing and avoiding duplicate work';
COMMENT ON COLUMN enrichment_records.card_ids IS 'Array of memory card IDs produced from this enrichment run';
COMMENT ON COLUMN enrichment_records.engine_kind IS 'Engine type identifier (e.g., "rules", "llm:qwen")';
COMMENT ON COLUMN enrichment_records.engine_version IS 'Engine version for tracking when processing logic changes';
