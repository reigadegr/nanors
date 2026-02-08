-- Sessions Table Migration for PostgreSQL
-- This migration creates the table needed for session management.

-- Sessions table: stores chat session data with serialized messages
CREATE TABLE IF NOT EXISTS sessions (
    id UUID PRIMARY KEY,
    messages TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
