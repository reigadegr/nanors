-- Migration: Remove unused tiered retrieval tables
-- This migration removes tables that were part of the tiered retrieval design
-- but never had data populated or fully implemented:
-- 1. resources table - for storing external resources (images, files)
-- 2. memory_categories table - for storing memory categories/summaries
-- 3. category_items table - junction table for many-to-many relationship

-- Drop junction table first (has foreign keys)
DROP TABLE IF EXISTS category_items CASCADE;

-- Drop main tables
DROP TABLE IF EXISTS memory_categories CASCADE;
DROP TABLE IF EXISTS resources CASCADE;
