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
    source_frame_id INTEGER,
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

-- Query expansion cache
CREATE TABLE query_expansions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    original_query TEXT NOT NULL,
    expanded_query TEXT NOT NULL,
    expansion_type VARCHAR(32) NOT NULL, -- or_query, stopwords, expanded
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_expansions_original ON query_expansions(original_query);
```

## Module Structure

```
nanors_memory/src/
├── extraction/
│   ├── mod.rs              # Extraction module entry point
│   ├── cards.rs            # MemoryCard types (based on memvid)
│   ├── patterns.rs         # Regex patterns for Chinese/English
│   ├── engine.rs           # ExtractionEngine trait and impl
│   └── chinese.rs          # Chinese-specific patterns
├── query/
│   ├── mod.rs              # Query analysis module
│   ├── detector.rs         # QuestionTypeDetector
│   ├── expander.rs         # QueryExpander
│   └── stopwords.rs        # Stopword lists (Chinese/English)
└── lib.rs                  # Add new exports
```

## Implementation Phases

### Phase 1: Database Schema & Core Types

**Files to create:**
1. `migration/src/m010_create_memory_cards.rs`
2. `nanors_core/src/memory/cards.rs` - Export `MemoryCard` types
3. `nanors_entities/src/memory_cards.rs` - SeaORM entities (via CLI)

**Core Types:**
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
```

**Commands:**
```bash
# Generate entities after migration
sea-orm-cli generate entity --with-serde=both -o nanors_entities/src
```

### Phase 2: Structured Memory Extraction

**Files to create:**
1. `nanors_memory/src/extraction/mod.rs`
2. `nanors_memory/src/extraction/cards.rs` - MemoryCard types
3. `nanors_memory/src/extraction/patterns.rs` - Regex patterns
4. `nanors_memory/src/extraction/chinese.rs` - Chinese patterns
5. `nanors_memory/src/extraction/engine.rs` - ExtractionEngine

**Chinese Extraction Patterns:**
```rust
// User identity statements
r"(?i)我是(.{1,20})(用户|玩机党|开发者|学生|工程师)"

// Location statements
r"(?i)我住在?(.{1,30})"

// Device ownership
r"(?i)我(的|用)?(手机|电脑|设备)是?(.{1,30})"

// Action/behavior
r"(?i)我(喜欢|爱|讨厌)(.{1,30})"
```

**Extraction Flow:**
```rust
pub trait ExtractionEngine {
    fn extract(&self, text: &str, scope: &str) -> Vec<MemoryCard>;
}

impl ExtractionEngine for RulesEngine {
    fn extract(&self, text: &str, scope: &str) -> Vec<MemoryCard> {
        let mut cards = Vec::new();

        // Apply all patterns
        for pattern in &self.patterns {
            if let Some(card) = pattern.apply(text, scope) {
                cards.push(card);
            }
        }

        cards
    }
}
```

### Phase 3: Question Type Detection

**Files to create:**
1. `nanors_memory/src/query/mod.rs`
2. `nanors_memory/src/query/detector.rs`

**Question Types:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionType {
    // Identity questions: "我是什么用户", "who am I"
    WhatKind,

    // Counting questions: "有多少个", "how many"
    HowMany,

    // Recency questions: "现在的", "最新", "current", "latest"
    Recency,

    // Update questions: "之前vs现在", "changed from"
    Update,

    // Location questions: "在哪", "where"
    Where,

    // Preference questions: "喜欢什么", "what do you like"
    Preference,

    // Generic/unrecognized
    Generic,
}
```

**Detection Patterns:**
```rust
impl QuestionTypeDetector {
    pub fn detect(&self, query: &str) -> QuestionType {
        let query = query.to_lowercase();

        // Check in priority order
        if self.is_what_kind(&query) {
            return QuestionType::WhatKind;
        }
        if self.is_how_many(&query) {
            return QuestionType::HowMany;
        }
        if self.is_recency(&query) {
            return QuestionType::Recency;
        }
        // ... more patterns

        QuestionType::Generic
    }
}
```

**Chinese Patterns:**
```rust
// WhatKind: "我是什么", "我是谁", "我的身份"
const WHAT_KIND_PATTERNS: &[&str] = &[
    "我是什么", "我是谁", "我的身份", "我是.*用户",
    "我属于", "我算.*用户", "我是.*吗",
];

// Recency: "现在", "目前", "最新", "当前"
const RECENCY_PATTERNS: &[&str] = &[
    "现在", "目前", "最新", "当前", "最近",
    "latest", "current", "right now",
];
```

### Phase 4: Query Expansion

**Files to create:**
1. `nanors_memory/src/query/expander.rs`
2. `nanors_memory/src/query/stopwords.rs`

**Expansion Strategies:**
```rust
pub struct QueryExpander {
    stopwords: HashSet<String>,
}

impl QueryExpander {
    /// Generate OR query from tokens for better recall
    pub fn expand_or(&self, query: &str) -> String {
        let tokens = self.tokenize(query);
        let content_tokens: Vec<_> = tokens
            .into_iter()
            .filter(|t| !self.is_stopword(t))
            .collect();

        content_tokens.join(" OR ")
    }

    /// Remove question words to get core terms
    pub fn remove_stopwords(&self, query: &str) -> String {
        // Remove "什么", "怎么", "如何", "where", "what", etc.
    }

    /// Generate singular/plural variants
    pub fn expand_variants(&self, query: &str) -> Vec<String> {
        // "用户" -> "用户", "user"
    }
}
```

**Chinese Stopwords:**
```rust
const CHINESE_STOPWORDS: &[&str] = &[
    // Question words
    "什么", "怎么", "如何", "哪里", "哪个", "多少",
    "谁", "什么时候", "为什么", "咋",

    // Common particles
    "的", "了", "吗", "呢", "吧", "啊",

    // Copular verbs
    "是", "有", "在",
];

// Keep these as they're content-bearing
const CONTENT_WORDS: &[&str] = &[
    "用户", "玩机", "开发者", "学生", "工程师",
    "手机", "电脑", "安卓", "kernel",
];
```

### Phase 5: Retrieval Integration

**Files to modify:**
1. `nanors_memory/src/manager.rs` - Add card lookup methods
2. `nanors_core/src/agent/agent_loop.rs` - Integrate new retrieval
3. `nanors_memory/src/scoring.rs` - Add card scoring boost

**New Manager Methods:**
```rust
impl MemoryManager {
    /// Fast O(1) lookup by entity/slot
    pub async fn get_card(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Option<MemoryCard>> {
        // Direct database query with index
    }

    /// Extract and store cards from a memory item
    pub async fn extract_and_store_cards(
        &self,
        memory: &MemoryItem,
    ) -> anyhow::Result<Vec<MemoryCard>> {
        // Use extraction engine
        // Store in database
    }
}
```

**Enhanced Search Flow:**
```rust
pub async fn search_by_embedding(
    &self,
    user_scope: &str,
    query_embedding: &[f32],
    query_text: &str,
    top_k: usize,
) -> anyhow::Result<Vec<SalienceScore<MemoryItem>>> {
    // 1. Detect question type
    let question_type = detector.detect(query_text);

    // 2. Expand query if needed
    let expanded_query = if question_type != QuestionType::Generic {
        Some(expander.expand_or(query_text))
    } else {
        None
    };

    // 3. Try structured card lookup first (O(1))
    if let Some(card) = self.lookup_card_for_query(user_scope, &question_type, query_text).await? {
        // Boost the source memory in results
    }

    // 4. Vector search with expanded query
    // ... existing logic

    // 5. Apply question-type-specific ranking
    let ranked = match question_type {
        QuestionType::WhatKind => self.rank_for_what_kind(results),
        QuestionType::Recency => self.rank_for_recency(results),
        _ => results,
    };

    Ok(ranked)
}
```

**Card Lookup Logic:**
```rust
async fn lookup_card_for_query(
    &self,
    user_scope: &str,
    question_type: &QuestionType,
    query_text: &str,
) -> anyhow::Result<Option<MemoryCard>> {
    match question_type {
        QuestionType::WhatKind => {
            // Look for entity="user", slot="user_type"
            self.get_card(user_scope, "user", "user_type").await
        }
        QuestionType::Where => {
            // Look for entity="user", slot="location"
            self.get_card(user_scope, "user", "location").await
        }
        QuestionType::Recency => {
            // Same slot but prefer newest
            self.get_card(user_scope, "user", "user_type").await
        }
        _ => Ok(None),
    }
}
```

## Configuration

**Add to `nanors_config/src/schema.rs`:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    pub enabled: bool,
    pub confidence_threshold: f32,
    pub extract_on_store: bool,  // Auto-extract when storing memories
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    pub enable_expansion: bool,
    pub enable_detection: bool,
    pub expansion_strategies: Vec<String>,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            confidence_threshold: 0.7,
            extract_on_store: true,
        }
    }
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            enable_expansion: true,
            enable_detection: true,
            expansion_strategies: vec!["or_query".to_string(), "stopwords".to_string()],
        }
    }
}

// Add to MemoryConfig
pub struct MemoryConfig {
    pub retrieval: RetrievalConfig,
    pub extraction: ExtractionConfig,
    pub query: QueryConfig,
    // ... existing fields
}
```

**Config file example (`~/nanors/config.json`):**
```json
{
  "memory": {
    "extraction": {
      "enabled": true,
      "extract_on_store": true
    },
    "query": {
      "enable_expansion": true,
      "enable_detection": true
    }
  }
}
```

## Testing Strategy

### Unit Tests

**Extraction Tests:**
```rust
#[test]
fn test_extract_user_type_chinese() {
    let text = "我是安卓玩机用户";
    let cards = engine.extract(text, "test_user");

    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].entity, "user");
    assert_eq!(cards[0].slot, "user_type");
    assert!(cards[0].value.contains("安卓"));
}

#[test]
fn test_extract_location() {
    let text = "我住在湖南长沙";
    let cards = engine.extract(text, "test_user");

    let location_card = cards.iter()
        .find(|c| c.slot == "location")
        .unwrap();
    assert!(location_card.value.contains("长沙"));
}
```

**Query Detection Tests:**
```rust
#[test]
fn test_detect_what_kind_chinese() {
    assert_eq!(detector.detect("我是什么用户"), QuestionType::WhatKind);
    assert_eq!(detector.detect("我是谁"), QuestionType::WhatKind);
}

#[test]
fn test_detect_recency_chinese() {
    assert_eq!(detector.detect("我现在在哪"), QuestionType::Recency);
    assert_eq!(detector.detect("我的最新地址是"), QuestionType::Recency);
}
```

**Query Expansion Tests:**
```rust
#[test]
fn test_expand_or_query() {
    let expanded = expander.expand_or("我是什么用户");
    assert!(expanded.contains("用户"));  // Content word preserved
    assert!(!expanded.contains("什么")); // Stopword removed
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_question_answer() {
    // 1. Store a memory
    let memory = MemoryItem {
        summary: "User: 我是安卓玩机用户",
        // ...
    };
    manager.semantic_upsert(&memory, 0.85).await.unwrap();

    // 2. Query with question
    let results = manager
        .search_by_embedding(&query_emb, "我是什么用户", 10)
        .await
        .unwrap();

    // 3. Verify the memory is found
    assert!(!results.is_empty());
    assert!(results[0].item.summary.contains("安卓"));
}
```

## Performance Considerations

1. **Card Extraction Cost**: O(n) patterns per memory, done once on storage
2. **Card Lookup**: O(1) with database index on `(user_scope, entity, slot)`
3. **Query Detection**: O(1) pattern matching
4. **Query Expansion**: O(m) where m = token count

**Optimization Strategies:**
- Cache extracted cards in memory (Redis or in-process)
- Batch card extraction on memory insertion
- Use prepared statements for card lookups
- Lazy expansion (only when needed)

## Migration Strategy

### Step 1: Add new tables (non-breaking)
```sql
-- Run migration 010
CREATE TABLE memory_cards (...);
```

### Step 2: Backfill existing memories
```bash
cargo run --bin backfill_cards
```

### Step 3: Enable extraction on new memories
```rust
// In semantic_upsert_memory
if config.extraction.extract_on_store {
    let cards = engine.extract(&item.summary, &item.user_scope);
    for card in cards {
        store_card(&card).await?;
    }
}
```

### Step 4: Enable query enhancements
```rust
// In search_by_embedding
if config.query.enable_detection {
    let qtype = detector.detect(query_text);
    // Apply detection-specific logic
}
```

## Success Criteria

1. **Recall**: "我是什么用户" retrieves "我是安卓玩机用户" with >0.5 similarity
2. **Precision**: Non-relevant memories stay below threshold
3. **Latency**: Card lookup adds <5ms to query time
4. **Coverage**: >80% of user statements extract at least one card

## Open Questions

1. Should we use LLM for extraction (higher accuracy) or regex (faster)?
   - **Decision**: Start with regex, add LLM extraction as enhancement later

2. How to handle conflicting card values?
   - **Decision**: Use version_relation with timestamp ordering (newest wins)

3. Should we expose cards to the LLM in context?
   - **Decision**: No, cards are internal optimization only

## References

- Memvid graph_search.rs - Query pattern matching
- Memvid enrich/rules.rs - Regex-based extraction
- Current nanors scoring.rs - Salience computation
