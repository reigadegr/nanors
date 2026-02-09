-- Migration: Remove resource_id column from memory_items
-- The resources table has been removed, so the foreign key column is no longer needed.

ALTER TABLE memory_items
DROP COLUMN IF EXISTS resource_id;
