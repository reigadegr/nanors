-- Migration: Add keyword-triggered memory versioning support
-- This migration adds fields for keyword-based fact versioning without LLM

-- Add fact_type field to store the type of fact (address, nickname, workplace, etc.)
ALTER TABLE memory_items
ADD COLUMN IF NOT EXISTS fact_type VARCHAR(64);

-- Add is_active field to mark if this is the active version
ALTER TABLE memory_items
ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT true;

-- Add parent_id field for version chain (separate from parent_version_id)
ALTER TABLE memory_items
ADD COLUMN IF NOT EXISTS parent_id UUID;

-- Create index for fact_type + is_active queries
CREATE INDEX IF NOT EXISTS idx_items_fact_active ON memory_items(user_scope, fact_type, is_active);
CREATE INDEX IF NOT EXISTS idx_items_parent_id ON memory_items(parent_id);

-- Add comment for documentation
COMMENT ON COLUMN memory_items.fact_type IS 'Type of fact: address, nickname, workplace, etc. NULL for non-fact memories';
COMMENT ON COLUMN memory_items.is_active IS 'Whether this is the active version. Only one version per fact_type should be active';
COMMENT ON COLUMN memory_items.parent_id IS 'Parent memory ID for version chain. Links to previous version of the same fact';
