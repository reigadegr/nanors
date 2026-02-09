-- Migration: Remove unused version fields from memory_items
-- This migration removes version tracking fields that were added but never fully implemented:
-- 1. version - version number (no version history table anymore)
-- 2. parent_version_id - reference to parent version
-- 3. version_relation - type of version relationship (Sets, Updates, etc.)

-- Remove version tracking columns from memory_items
ALTER TABLE memory_items
DROP COLUMN IF EXISTS version,
DROP COLUMN IF EXISTS parent_version_id,
DROP COLUMN IF EXISTS version_relation;
