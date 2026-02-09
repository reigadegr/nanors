-- Migration: Remove keyword-triggered memory versioning support
-- This migration removes fields for keyword-based fact versioning
-- as the system is being simplified to use only vector similarity search

-- Drop indexes created for keyword versioning
DROP INDEX IF EXISTS idx_items_fact_active;
DROP INDEX IF EXISTS idx_items_parent_id;

-- Remove parent_id field (version chain)
ALTER TABLE memory_items
DROP COLUMN IF EXISTS parent_id;

-- Remove is_active field (active version marker)
ALTER TABLE memory_items
DROP COLUMN IF EXISTS is_active;

-- Remove fact_type field (fact type classifier)
ALTER TABLE memory_items
DROP COLUMN IF EXISTS fact_type;
