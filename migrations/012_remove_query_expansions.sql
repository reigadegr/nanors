-- Migration: Remove unused query_expansions table
--
-- The query_expansions table was created to cache expanded queries, but:
-- 1. The table remained empty (0 rows) since creation
-- 2. QueryExpander is purely in-memory and doesn't use database caching
-- 3. The entity was never used for any database operations
--
-- This migration removes the unused table to reduce database complexity.

DROP TABLE IF EXISTS query_expansions CASCADE;
