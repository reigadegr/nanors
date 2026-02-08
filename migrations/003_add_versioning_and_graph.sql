-- Migration: Add versioning and graph-aware memory support
-- This migration adds:
-- 1. Version control fields to memory_items
-- 2. memory_item_versions table for history tracking
-- 3. memory_cards table for structured graph-based memory

-- Add version control fields to memory_items
ALTER TABLE memory_items
ADD COLUMN IF NOT EXISTS version INTEGER NOT NULL DEFAULT 1,
ADD COLUMN IF NOT EXISTS parent_version_id UUID,
ADD COLUMN IF NOT EXISTS version_relation VARCHAR(32) DEFAULT 'Sets';

-- Create memory_item_versions table for tracking version history
CREATE TABLE IF NOT EXISTS memory_item_versions (
    id UUID PRIMARY KEY,
    memory_item_id UUID NOT NULL,
    version INTEGER NOT NULL,
    parent_version_id UUID,
    version_relation VARCHAR(32) NOT NULL,
    summary TEXT NOT NULL,
    extra JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_version_memory FOREIGN KEY (memory_item_id) REFERENCES memory_items(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_versions_memory_item_id ON memory_item_versions(memory_item_id, version);
CREATE INDEX IF NOT EXISTS idx_versions_parent_id ON memory_item_versions(parent_version_id);

-- Create memory_cards table for structured facts
CREATE TABLE IF NOT EXISTS memory_cards (
    id UUID PRIMARY KEY,
    user_scope VARCHAR(255) NOT NULL,
    memory_item_id UUID,
    kind VARCHAR(32) NOT NULL DEFAULT 'fact',
    entity VARCHAR(255) NOT NULL,
    slot VARCHAR(255) NOT NULL,
    value TEXT NOT NULL,
    polarity VARCHAR(16),
    event_date TIMESTAMPTZ,
    document_date TIMESTAMPTZ,
    version_key VARCHAR(511),
    version_relation VARCHAR(32) NOT NULL DEFAULT 'Sets',
    source_uri TEXT,
    engine VARCHAR(64) NOT NULL,
    engine_version VARCHAR(64) NOT NULL,
    confidence FLOAT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_card_memory FOREIGN KEY (memory_item_id) REFERENCES memory_items(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_cards_user_scope ON memory_cards(user_scope);
CREATE INDEX IF NOT EXISTS idx_cards_entity_slot ON memory_cards(user_scope, entity, slot);
CREATE INDEX IF NOT EXISTS idx_cards_slot_value ON memory_cards(user_scope, slot, value);
CREATE INDEX IF NOT EXISTS idx_cards_version_key ON memory_cards(user_scope, version_key);
CREATE INDEX IF NOT EXISTS idx_cards_memory_item ON memory_cards(memory_item_id);

-- Create memory_card_versions table for tracking card version history
CREATE TABLE IF NOT EXISTS memory_card_versions (
    id UUID PRIMARY KEY,
    memory_card_id UUID NOT NULL,
    version INTEGER NOT NULL,
    parent_version_id UUID,
    version_relation VARCHAR(32) NOT NULL,
    entity VARCHAR(255) NOT NULL,
    slot VARCHAR(255) NOT NULL,
    value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_card_version_card FOREIGN KEY (memory_card_id) REFERENCES memory_cards(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_card_versions_card_id ON memory_card_versions(memory_card_id, version);
CREATE INDEX IF NOT EXISTS idx_card_versions_parent_id ON memory_card_versions(parent_version_id);
