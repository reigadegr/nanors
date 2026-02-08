-- Memory Tables Migration for PostgreSQL
-- This migration creates the tables needed for the memU-style persistent memory system.

-- Resources table: stores references to external resources (images, files, etc.)
CREATE TABLE IF NOT EXISTS resources (
    id UUID PRIMARY KEY,
    user_scope VARCHAR(255) NOT NULL,
    url TEXT,
    modality VARCHAR(64) NOT NULL,
    local_path TEXT,
    caption TEXT,
    embedding JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_resources_user_scope ON resources(user_scope);

-- Memory items table: stores individual memories with embeddings and deduplication
CREATE TABLE IF NOT EXISTS memory_items (
    id UUID PRIMARY KEY,
    user_scope VARCHAR(255) NOT NULL,
    resource_id UUID,
    memory_type VARCHAR(64) NOT NULL,
    summary TEXT NOT NULL,
    embedding JSONB,
    happened_at TIMESTAMPTZ NOT NULL,
    extra JSONB,
    content_hash VARCHAR(64) NOT NULL,
    reinforcement_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_memory_items_resource FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_memory_items_user_scope ON memory_items(user_scope);
CREATE INDEX IF NOT EXISTS idx_memory_items_content_hash ON memory_items(content_hash);
CREATE INDEX IF NOT EXISTS idx_memory_items_user_scope_hash ON memory_items(user_scope, content_hash);

-- Memory categories table: stores category/group information
CREATE TABLE IF NOT EXISTS memory_categories (
    id UUID PRIMARY KEY,
    user_scope VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    embedding JSONB,
    summary TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT unique_category_name UNIQUE (user_scope, name)
);

CREATE INDEX IF NOT EXISTS idx_memory_categories_user_scope ON memory_categories(user_scope);

-- Category items junction table: many-to-many relationship between memories and categories
CREATE TABLE IF NOT EXISTS category_items (
    item_id UUID NOT NULL,
    category_id UUID NOT NULL,
    PRIMARY KEY (item_id, category_id),
    CONSTRAINT fk_category_items_item FOREIGN KEY (item_id) REFERENCES memory_items(id) ON DELETE CASCADE,
    CONSTRAINT fk_category_items_category FOREIGN KEY (category_id) REFERENCES memory_categories(id) ON DELETE CASCADE
);
