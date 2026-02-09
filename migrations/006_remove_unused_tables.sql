-- Migration: Remove unused versioning and graph tables
-- This migration removes tables that were created but never used in business logic:
-- 1. memory_item_versions table (no repo implementation)
-- 2. memory_cards table (no repo implementation)
-- 3. memory_card_versions table (no repo implementation)

-- Drop memory_card_versions (depends on memory_cards)
DROP TABLE IF EXISTS memory_card_versions CASCADE;

-- Drop memory_cards
DROP TABLE IF EXISTS memory_cards CASCADE;

-- Drop memory_item_versions
DROP TABLE IF EXISTS memory_item_versions CASCADE;
