-- Migration: Add structured memory cards and query expansion support
-- This migration adds:
-- 1. memory_cards table for structured entity/slot/value triples
-- 2. query_expansions table for caching expanded queries
-- 3. Indexes for fast O(1) card lookups

-- Create memory_cards table for structured facts extracted from text
CREATE TABLE IF NOT EXISTS memory_cards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_scope VARCHAR(255) NOT NULL,

    -- Card type: fact, preference, event, profile, relationship, goal
    kind VARCHAR(32) NOT NULL DEFAULT 'fact',

    -- Structured triple (entity/slot/value model)
    entity VARCHAR(255) NOT NULL,
    slot VARCHAR(255) NOT NULL,
    value TEXT NOT NULL,

    -- Polarity for preferences (positive/negative/neutral)
    polarity VARCHAR(16),

    -- Temporal information
    event_date TIMESTAMPTZ,
    document_date TIMESTAMPTZ,

    -- Versioning for tracking changes to the same entity/slot
    version_key VARCHAR(511),
    version_relation VARCHAR(32) NOT NULL DEFAULT 'Sets',

    -- Provenance
    source_memory_id UUID,
    engine VARCHAR(64) NOT NULL DEFAULT 'rules',
    engine_version VARCHAR(64) NOT NULL DEFAULT '1.0.0',
    confidence FLOAT,

    -- Metadata
    extra JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_card_memory
        FOREIGN KEY (source_memory_id)
        REFERENCES memory_items(id)
        ON DELETE CASCADE
);

-- Index for fast O(1) lookups by entity/slot
CREATE INDEX IF NOT EXISTS idx_cards_entity_slot
    ON memory_cards(user_scope, entity, slot);

-- Index for version tracking
CREATE INDEX IF NOT EXISTS idx_cards_version_key
    ON memory_cards(user_scope, version_key);

-- Index for source memory lookups
CREATE INDEX IF NOT EXISTS idx_cards_source_memory
    ON memory_cards(source_memory_id);

-- Index for slot-based queries (e.g., all user_type cards)
CREATE INDEX IF NOT EXISTS idx_cards_slot_value
    ON memory_cards(user_scope, slot, value);

-- Create query_expansions table for caching expanded queries
CREATE TABLE IF NOT EXISTS query_expansions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_scope VARCHAR(255) NOT NULL,
    original_query TEXT NOT NULL,
    expanded_query TEXT NOT NULL,
    expansion_type VARCHAR(32) NOT NULL, -- or_query, stopwords, expanded
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for looking up cached expansions
CREATE INDEX IF NOT EXISTS idx_expansions_original
    ON query_expansions(user_scope, original_query);

-- Add comment for documentation
COMMENT ON TABLE memory_cards IS 'Structured memory cards extracted from text using entity/slot/value model';
COMMENT ON TABLE query_expansions IS 'Cache for expanded queries to improve recall';
