# Keyword-Triggered Memory Versioning System

## Overview

A rule-based memory versioning system that detects fact updates using keyword patterns, without requiring any LLM API calls.

## Features

- **Memory Isolation**: Strictly separates user input (`User:` prefix) from assistant responses (`Assistant:` prefix)
- **Keyword Detection**: Predefined patterns automatically detect fact type changes (address, nickname, workplace, etc.)
- **Version Chain**: When a fact is updated, the old version is marked as inactive and a new active version is created
- **Retrieval Optimization**: Only active memories are returned during retrieval, ensuring users always get the latest information

## Database Schema

New fields added to `memory_items` table:

| Field | Type | Description |
|-------|------|-------------|
| `fact_type` | VARCHAR(64) | Type of fact: `address`, `nickname`, `workplace`, etc. (NULL for non-facts) |
| `is_active` | BOOLEAN | Whether this is the active version. Only one per fact_type should be true |
| `parent_id` | UUID | Parent memory ID for version chain. Links to previous version of same fact |

## Keyword Patterns

### Default Patterns

| Fact Type | Patterns | Priority |
|-----------|----------|----------|
| Address | `搬家到X`, `搬到X`, `现住X`, `居住在X`, `住址改为X` | 100 |
| Address | `我住在X` | 90 |
| Address | `地址是X`, `地址改为X` | 80 |
| Nickname | `改名为X`, `叫我X`, `昵称是X` | 100 |
| Nickname | `我是X` | 80 |
| Workplace | `入职X`, `就职于X`, `工作单位改为X`, `在X工作` | 100 |
| Workplace | `公司是X` | 80 |
| Phone | `手机号是X`, `手机改为X`, `联系电话X` | 100 |
| Email | `邮箱是X`, `邮箱改为X` | 100 |
| Generic | `更换为X`, `更新为X`, `现在是X` | 50 |

Note: X represents 1-50 characters of content.

## Extending Keywords

To add custom keyword patterns, modify `nanors_core/src/memory/keyword_versioning.rs`:

```rust
// In KeywordLibrary::default_patterns(), add:
(FactType::CustomType, r"your_pattern_here.{1,50}", priority),
```

## Configuration

No configuration file needed. The system works out of the box with default patterns.

## Usage Examples

### Basic Address Update

```bash
# User sets initial address
cargo run agent -m "我住朝阳区"

# User updates address (automatically creates version 2)
cargo run agent -m "我搬家到了海淀区"

# Query returns latest version
cargo run agent -m "我住哪里"
# Output: "您住在海淀区"
```

### Mixed Conversation

```bash
cargo run agent -m "你好"
cargo run agent -m "我住朝阳区"
cargo run agent -m "今天天气怎么样?"
cargo run agent -m "我搬家到了海淀区"
cargo run agent -m "我住哪里"
# Output: "您住在海淀区" (latest version)
```

### Multiple Fact Types

```bash
# Set nickname
cargo run agent -m "叫我小明"

# Set address
cargo run agent -m "我住朝阳区"

# Update nickname (creates version 2 of nickname)
cargo run agent -m "改名为小红"

# Both facts tracked independently
cargo run agent -m "我叫什么名字?"
# Output: "您叫小红"

cargo run agent -m "我住哪里?"
# Output: "您住在朝阳区"
```

## API Reference

### `MemoryManager::keyword_versioned_insert()`

Main entry point for storing memories with keyword versioning.

```rust
let memory = MemoryItem {
    id: Uuid::now_v7(),
    user_scope: "user123".to_string(),
    memory_type: MemoryType::Episodic,
    summary: "User: 我住朝阳区".to_string(),
    // ... other fields
    version: 1,
    parent_version_id: None,
    version_relation: None,
    fact_type: None,  // Will be auto-detected
    is_active: true,
    parent_id: None,
};

memory_manager.keyword_versioned_insert(&memory).await?;
```

### `MemoryVersioner::analyze()`

Analyze a memory to determine versioning action.

```rust
use nanors_core::memory::{MemoryVersioner, VersioningAction};

let versioner = MemoryVersioner::new();
let result = versioner.analyze(&memory_item);

match result.action {
    VersioningAction::NoVersioning => { /* Assistant response */ }
    VersioningAction::NewFact { fact_type } => { /* New fact detected */ }
    VersioningAction::NonFact => { /* No fact keyword matched */ }
    _ => {}
}
```

## Memory Storage Logic

1. **Assistant Response Detection**
   - If summary starts with "Assistant:" → Store as-is, no versioning

2. **Keyword Matching**
   - If user input matches any keyword pattern → Determine fact_type

3. **Version Chain Management**
   - If existing active memory with same fact_type exists:
     - Mark old memory as `is_active=false`
     - Create new memory with `is_active=true`, `parent_id=old_id`
   - Otherwise → Create new fact memory

4. **Non-Fact Storage**
   - If no keyword matches → Store as regular memory (`fact_type=NULL`)

## Retrieval Logic

When users query their memories:

1. **Keyword Match**: If query matches a fact_type, return the active memory for that type
2. **Fallback**: If no keyword match, use semantic search with threshold 0.6
3. **Active Only**: Only `is_active=true` memories are returned; deprecated versions are excluded

## Testing

Run manual tests to verify functionality:

```bash
# Address update test
cargo run agent -m "我住丰台区"
cargo run agent -m "我搬家到了东城"
cargo run agent -m "我住哪里"
# Should return "东城"

# No keyword test
cargo run agent -m "今天天气很好"
# Stored as non-fact memory
```

## Troubleshooting

### Keywords Not Detected

- Check the pattern in `KeywordLibrary::default_patterns()`
- Regex patterns are case-insensitive
- Use regex tester to validate patterns: `https://rustexp.com/`

### Wrong Version Returned

- Verify database: `SELECT * FROM memory_items WHERE fact_type='address' ORDER BY version;`
- Only one row should have `is_active=true`
- Check `parent_id` linking between versions

### Memory Not Stored

- Check logs for "keyword_versioned_insert" calls
- Verify user_scope matches database queries
- Check for "Failed to store" error messages
