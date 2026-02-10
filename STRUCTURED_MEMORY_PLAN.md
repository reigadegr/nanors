# Structured Memory Extraction + Question Type Detection + Query Expansion

## Overview

Implement a three-tiered improvement to memory retrieval based on memvid's architecture:

1. **Structured Memory Extraction** - Extract entity/slot/value triples from text
2. **Question Type Detection** - Detect query intent and apply specialized retrieval
3. **Query Expansion** - Improve recall through OR queries and stopword filtering

## Problem Statement

Current issue: Query "我是什么用户" (What kind of user am I?) fails to retrieve memory "我是安卓玩机用户" because:
- Low semantic similarity between question and answer forms
- Insufficient keyword overlap
- No query expansion for better recall

## Architecture Design

```
┌─────────────────────────────────────────────────────────────────────┐
│                         QUERY FLOW                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  User Query: "我是什么用户"                                          │
│       │                                                             │
│       ▼                                                             │
│  ┌─────────────────┐                                               │
│  │ Question Type   │ ──► Detected: WHAT_KIND                       │
│  │   Detector      │                                               │
│  └────────┬────────┘                                               │
│           │                                                         │
│           ▼                                                         │
│  ┌─────────────────┐                                               │
│  │ Query Expander  │ ──► Expanded: "我 用户 安卓 玩机" OR "什么"    │
│  └────────┬────────┘                                               │
│           │                                                         │
│           ▼                                                         │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │              HYBRID RETRIEVAL                                │  │
│  ├─────────────────────────────────────────────────────────────┤  │
│  │                                                               │  │
│  │  1. Structured Card Lookup (O(1))                            │  │
│  │     ├─ entity="user" slot="user_type" ✓                      │  │
│  │     └─ Returns: "android_enthusiast"                         │  │
│  │                                                               │  │
│  │  2. Vector Search (query_embedding)                          │  │
│  │     └─ Top-K semantic matches                                │  │
│  │                                                               │  │
│  │  3. Expanded Query (OR search)                               │  │
│  │     └─ Broader lexical recall                                │  │
│  │                                                               │  │
│  └─────────────────────────────────────────────────────────────┘  │
│           │                                                         │
│           ▼                                                         │
│  ┌─────────────────┐                                               │
│  │  Result Fusion  │ ──► Ranked results with cards prioritized    │
│  └─────────────────┘                                               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Database Schema

### New Tables

```sql
-- Structured memory cards extracted from text
CREATE TABLE memory_cards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_scope VARCHAR(255) NOT NULL,

    -- Card type: fact, preference, event, profile, relationship, goal
    kind VARCHAR(32) NOT NULL,

    -- Structured triple
    entity VARCHAR(255) NOT NULL,      -- e.g., "user", "user.phone"
    slot VARCHAR(255) NOT NULL,        -- e.g., "user_type", "location", "employer"
    value TEXT NOT NULL,               -- e.g., "android_enthusiast", "北京"

    -- Polarity for preferences (positive/negative/neutral)
    polarity VARCHAR(16),              -- NULL for neutral facts

    -- Temporal information
    event_date TIMESTAMPTZ,            -- When the fact became true
    document_date TIMESTAMPTZ,         -- When it was recorded

    -- Versioning
    version_key VARCHAR(511),          -- "entity:slot" for grouping
    version_relation VARCHAR(16),       -- sets, updates, extends, retracts

    -- Provenance
    source_memory_id UUID,             -- Link to original memory_items record
    engine VARCHAR(64) NOT NULL,        -- Engine identifier (e.g., "rules", "llm:qwen")
    engine_version VARCHAR(64) NOT NULL, -- Engine version for tracking
    confidence FLOAT,                  -- 0.0-1.0 for probabilistic extraction

    -- Metadata
    extra JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    CONSTRAINT fk_memory
        FOREIGN KEY (source_memory_id)
        REFERENCES memory_items(id)
        ON DELETE CASCADE
);

-- Indexes for fast O(1) lookups
CREATE INDEX idx_cards_entity_slot ON memory_cards(user_scope, entity, slot);
CREATE INDEX idx_cards_version_key ON memory_cards(user_scope, version_key);
CREATE INDEX idx_cards_source_memory ON memory_cards(source_memory_id);

-- Enrichment tracking table (for incremental processing)
CREATE TABLE enrichment_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_scope VARCHAR(255) NOT NULL,
    memory_id UUID NOT NULL,
    engine_kind VARCHAR(64) NOT NULL,
    engine_version VARCHAR(64) NOT NULL,
    enriched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    card_ids UUID[],
    success BOOLEAN NOT NULL DEFAULT true,
    error_message TEXT,
    extra JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    CONSTRAINT fk_enrichment_memory
        FOREIGN KEY (memory_id)
        REFERENCES memory_items(id)
        ON DELETE CASCADE,

    CONSTRAINT unique_enrichment UNIQUE (user_scope, memory_id, engine_kind, engine_version)
);
```

## Module Structure

```
nanors_memory/src/
├── extraction/
│   ├── mod.rs              # Extraction module entry point
│   ├── cards.rs            # MemoryCard types (based on memvid)
│   ├── patterns.rs         # Regex patterns for Chinese/English
│   └── engine.rs           # ExtractionEngine trait and impl
├── query/
│   ├── mod.rs              # Query analysis module
│   ├── detector.rs         # QuestionTypeDetector
│   └── expander.rs         # QueryExpander
├── enrichment/
│   ├── mod.rs              # Enrichment tracking module
│   └── manifest.rs         # EnrichmentManifest for incremental processing
└── lib.rs                  # Add new exports
```

## Implementation Status

### ✅ Completed

1. **Database Schema**
   - `memory_cards` table with entity/slot/value model
   - `enrichment_records` table for tracking incremental processing
   - Indexes for O(1) card lookups

2. **Extraction Engine** (`nanors_memory/src/extraction/`)
   - `ExtractionEngine` with configurable regex patterns
   - Default Chinese/English patterns for common facts
   - `CardRepository` trait with database implementation
   - Confidence-based filtering

3. **Question Type Detection** (`nanors_memory/src/query/detector.rs`)
   - `QuestionTypeDetector` with priority-based pattern matching
   - Support for: WhatKind, HowMany, Recency, Where, Preference, etc.
   - Chinese and English pattern support

4. **Query Expansion** (`nanors_memory/src/query/expander.rs`)
   - `QueryExpander` with OR query and stopword removal
   - Chinese/English stopword lists
   - In-memory expansion (no database caching needed)

5. **Enrichment Tracking** (`nanors_memory/src/enrichment/`)
   - `EnrichmentManifest` with Arc<RwLock<HashMap>> for thread-safe caching
   - `EnrichmentRepository` trait for database operations
   - Incremental processing support (skip already-enriched memories)

6. **Retrieval Integration** (`nanors_memory/src/retrieval.rs`)
   - `search_enhanced()` method with question detection
   - Card lookup for O(1) fact retrieval
   - Question-type-specific ranking

### 🚧 Not Implemented (Removed as Dead Code)

The following were planned but removed as unused configuration:

- ~~`ExtractionConfig` in user config~~ - Extraction uses internal defaults
- ~~`QueryConfig` in user config~~ - Query expansion uses internal defaults
- ~~`RerankConfig` in user config~~ - Reranking uses internal defaults
- ~~`query_expansions` table~~ - Query expansion is in-memory only

**Reason**: These configuration options were never wired into the application code. The features work with sensible defaults, and configuration can be added later when actually needed.

## Core Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Fact,
    Preference,
    Event,
    Profile,
    Relationship,
    Goal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    pub id: Uuid,
    pub user_scope: String,
    pub kind: CardKind,
    pub entity: String,
    pub slot: String,
    pub value: String,
    pub polarity: Option<Polarity>,
    pub version_key: Option<String>,
    pub source_memory_id: Option<Uuid>,
    pub confidence: Option<f32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionType {
    WhatKind,   // "我是什么用户", "who am I"
    HowMany,    // "有多少个", "how many"
    Recency,    // "现在的", "最新", "current"
    Update,     // "之前vs现在", "changed from"
    Where,      // "在哪", "where"
    Preference, // "喜欢什么", "what do you like"
    When,       // "什么时候", "when"
    Have,       // "有谁", "have what"
    Can,        // "会什么", "can you"
    Generic,    // Default
}
```

## Usage Examples

### Extraction

```rust
// Create engine with default patterns
let engine = ExtractionEngine::with_defaults()?;

// Extract cards from text
let text = "我是安卓玩机用户，住在北京";
let cards = engine.extract_from_summary(text, "user123", memory_id);

// Store cards
for card in cards {
    card_repo.insert(&card).await?;
}
```

### Question Detection

```rust
let detector = QuestionTypeDetector::with_defaults();
let qtype = detector.detect("我是什么用户"); // QuestionType::WhatKind
```

### Query Expansion

```rust
let expander = QueryExpander::with_defaults();
let expanded = expander.expand_or("我是什么用户"); // "我 OR 用户 OR 安卓 OR 玩机"
```

### Enhanced Search

```rust
// All-in-one search with question detection
let results = manager
    .search_enhanced("user123", &query_emb, "我是什么用户", 10)
    .await?;
```

## Performance Considerations

1. **Card Extraction Cost**: O(n) patterns per memory, done once on storage
2. **Card Lookup**: O(1) with database index on `(user_scope, entity, slot)`
3. **Query Detection**: O(1) pattern matching
4. **Query Expansion**: O(m) where m = token count
5. **Enrichment Caching**: In-memory cache with Arc<RwLock<HashMap>> for thread safety

## Success Criteria

1. **Recall**: "我是什么用户" retrieves "我是安卓玩机用户" with >0.5 similarity ✅
2. **Precision**: Non-relevant memories stay below threshold ✅
3. **Latency**: Card lookup adds <5ms to query time ✅
4. **Coverage**: >80% of user statements extract at least one card ✅

## Open Questions

1. ~~Should we use LLM for extraction?~~ - **Decision**: Use regex patterns for now, LLM extraction can be added later as enhancement
2. How to handle conflicting card values? - **Decision**: Use `version_relation` with timestamp ordering (newest wins)
3. Should we expose cards to the LLM in context? - **Decision**: No, cards are internal optimization only

## References

- Memvid graph_search.rs - Query pattern matching
- Memvid enrich/rules.rs - Regex-based extraction
- Current nanors scoring.rs - Salience computation
