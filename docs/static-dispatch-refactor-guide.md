# nanors_tools 静态分发重构指南

## 概述

将 `ToolRegistry` 从动态分发 (`Vec<Box<dyn Tool>>`) 改为静态分发 (枚举+匹配)，消除 trait 对象开销。

## 当前设计分析

### 当前实现 (动态分发)

```rust
// lib.rs:102-103
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}
```

**性能开销：**
- 每个 `Box<dyn Tool>` 包含：fat pointer (data + vtable) + 堆分配
- 虚函数调用：每次 `execute()` 需要通过 vtable 间接调用
- 缓存不友好：工具分散在堆的不同位置

## 静态分发方案

### 设计原则

来自 `m04-zero-cost` 的决策指南：

| Scenario | Choose | Why |
|----------|--------|-----|
| **Small, known type set** | `enum` | No indirection, compile-time dispatch |
| Heterogeneous collection | `dyn Trait` | Different types at runtime |
| Plugin architecture | `dyn Trait` | Unknown types at compile time |

nanors_tools 的工具集是**固定的 6 个**：`bash`, `read_file`, `apply_patch`, `glob`, `grep`, `web_fetch`。

→ **选择 enum 静态分发**

### 架构对比

```
┌─────────────────────────────────────────────────────────────┐
│                    动态分发 (当前)                        │
├─────────────────────────────────────────────────────────────┤
│  ToolRegistry                                            │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐      │
│  │ Box │ │ Box │ │ Box │ │ Box │ │ Box │ │ Box │      │
│  │  →  │ │  →  │ │  →  │ │  →  │ │  →  │ │  →  │      │
│  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘      │
│     ↓       ↓       ↓       ↓       ↓       ↓             │
│  ┌──────────────────────────────────────────────────┐     │
│  │            Heap (分散布局)                      │     │
│  └──────────────────────────────────────────────────┘     │
│                                                             │
│  execute() → vtable lookup → indirect call                   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    静态分发 (重构后)                       │
├─────────────────────────────────────────────────────────────┤
│  StaticToolRegistry                                       │
│  ┌──────────────────────────────────────────────────┐       │
│  │ Vec<StaticTool> (连续内存，栈上布局)           │       │
│  └──────────────────────────────────────────────────┘       │
│                                                             │
│  execute() → match → direct call (可内联)                   │
└─────────────────────────────────────────────────────────────┘
```

## 实现步骤

### Step 1: 定义工具枚举

**文件**: `nanors_tools/src/lib.rs`

```rust
/// 静态分发工具枚举
pub enum StaticTool {
    Bash(BashTool),
    ReadFile(ReadFileTool),
    ApplyPatch(ApplyPatchTool),
    Glob(GlobTool),
    Grep(GrepTool),
    WebFetch(WebFetchTool),
}
```

### Step 2: 为枚举实现统一接口

```rust
impl StaticTool {
    /// 获取工具名称 (静态分发，零成本)
    pub fn name(&self) -> &'static str {
        match self {
            Self::Bash(t) => t.name(),
            Self::ReadFile(t) => t.name(),
            Self::ApplyPatch(t) => t.name(),
            Self::Glob(t) => t.name(),
            Self::Grep(t) => t.name(),
            Self::WebFetch(t) => t.name(),
        }
    }

    /// 获取工具定义 (静态分发)
    pub fn definition(&self) -> ToolDefinition {
        match self {
            Self::Bash(t) => t.definition(),
            Self::ReadFile(t) => t.definition(),
            Self::ApplyPatch(t) => t.definition(),
            Self::Glob(t) => t.definition(),
            Self::Grep(t) => t.definition(),
            Self::WebFetch(t) => t.definition(),
        }
    }

    /// 执行工具 (静态分发，可内联优化)
    pub async fn execute(&self, input: serde_json::Value) -> ToolResult {
        match self {
            Self::Bash(t) => t.execute(input).await,
            Self::ReadFile(t) => t.execute(input).await,
            Self::ApplyPatch(t) => t.execute(input).await,
            Self::Glob(t) => t.execute(input).await,
            Self::Grep(t) => t.execute(input).await,
            Self::WebFetch(t) => t.execute(input).await,
        }
    }

    /// 快速名称匹配 (编译时优化)
    pub fn name_str(&self) -> &str {
        match self {
            Self::Bash(_) => "bash",
            Self::ReadFile(_) => "read_file",
            Self::ApplyPatch(_) => "apply_patch",
            Self::Glob(_) => "glob",
            Self::Grep(_) => "grep",
            Self::WebFetch(_) => "web_fetch",
        }
    }
}
```

### Step 3: 新的注册表实现

```rust
/// 静态分发工具注册表
pub struct StaticToolRegistry {
    tools: Vec<StaticTool>,
}

impl StaticToolRegistry {
    /// 创建默认工具集
    #[must_use]
    pub fn with_default_tools(working_dir: &str) -> Self {
        Self {
            tools: vec![
                StaticTool::Bash(BashTool::new(working_dir)),
                StaticTool::ReadFile(ReadFileTool::new(working_dir)),
                StaticTool::ApplyPatch(ApplyPatchTool::new(working_dir)),
                StaticTool::Glob(GlobTool::new(working_dir)),
                StaticTool::Grep(GrepTool::new(working_dir)),
                StaticTool::WebFetch(
                    WebFetchTool::new(WebFetchConfig::default())
                        .expect("Failed to create WebFetchTool"),
                ),
            ],
        }
    }

    /// 获取所有工具定义
    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    /// 按名称执行工具 (静态分发)
    pub async fn execute(&self, name: &str, input: serde_json::Value) -> ToolResult {
        let started = std::time::Instant::now();

        let result = match self.tools.iter().find(|t| t.name_str() == name) {
            Some(tool) => tool.execute(input).await,
            None => return ToolResult::error(format!("Unknown tool: {name}"))
                .with_error_type("unknown_tool"),
        };

        let mut result = result;
        result.duration_ms = Some(started.elapsed().as_millis());
        result
    }
}
```

### Step 4: 兼容性策略

保留旧的 `ToolRegistry` 作为 `Deprecated`：

```rust
#[deprecated(since = "0.2.0", note = "Use StaticToolRegistry instead")]
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

// 保留旧实现用于平滑迁移...
```

## 性能分析

### 内存对比

| 组件 | 动态分发 | 静态分发 | 节省 |
|------|---------|---------|------|
| 每个工具指针 | 16 bytes (fat ptr) | 0 (内联) | 16 bytes |
| 堆分配开销 | 6 次 | 0 次 | 6 次分配 |
| vtable 查找 | 每次 execute | 0 | - |

### 执行速度对比

```rust
// 动态分发
tool.execute(input).await
// ↓
// 1. 加载 vtable 指针
// 2. 查找 execute 函数指针
// 3. 间接调用

// 静态分发
match self { Self::Bash(t) => t.execute(input).await, ... }
// ↓
// 1. 编译时分支 (CPU 分支预测器优化)
// 2. 直接调用 (可内联)
```

### 来自 m10-performance 的优化建议

**优化收益评估：**
- 分发开销减少：~5-10ns per call (vtable vs match)
- 内存占用减少：~96 bytes (6 tools × 16 bytes)
- 缓存局部性：连续内存布局提升缓存命中率

**注意**: 工具内部执行时间 (如 bash 命令) 远大于分发开销，实际收益取决于使用模式。
高频调用场景收益更明显。

## 迁移计划

### Phase 1: 准备
1. 添加 `StaticTool` 枚举
2. 实现 `StaticToolRegistry`
3. 保留 `ToolRegistry` (deprecated)

### Phase 2: 测试
```rust
#[cfg(test)]
mod static_dispatch_tests {
    use super::*;

    #[tokio::test]
    async fn static_dispatch_bash() {
        let registry = StaticToolRegistry::with_default_tools(".");
        let result = registry.execute(
            "bash",
            json!({"command": "echo hello"})
        ).await;
        assert!(!result.is_error);
    }

    // 为每个工具添加类似测试...
}
```

### Phase 3: 集成迁移
1. 更新 `nanors_core` 使用 `StaticToolRegistry`
2. 更新 `app` 使用新 API
3. 运行完整测试套件

### Phase 4: 清理
1. 移除 `#[deprecated]` 的旧代码
2. 移除 `async_trait` 依赖 (如不再需要)

## 设计权衡

### 优势
- **零成本抽象**: match 在编译时展开
- **无堆分配**: 所有工具在 Vec 中连续存储
- **可内联**: 编译器可内联 execute 调用
- **内存局部性**: 提升缓存命中率

### 代价
- 添加新工具需修改枚举 (但工具集固定，可接受)
- 枚举变体数量增加 (当前 6 个，规模可控)

## 参考

### 相关 Skills
- **m04-zero-cost**: 静态 vs 动态分发决策
- **m07-concurrency**: Send/Sync 约束处理
- **m10-performance**: 性能优化策略

### 决策依据 (来自 m04-zero-cost)

```
"Need collection of different types"
    ↓ Closed set → enum          ✓ nanors_tools (6 tools)
    ↓ Open set → Vec<Box<dyn Trait>>

"Types known at compile time"
    ↓ Yes → static dispatch     ✓ All tools known at compile
    ↓ No → dynamic dispatch
```

## 附录: 完整代码示例

```rust
// nanors_tools/src/lib.rs

#[derive(Debug, Clone)]
pub enum StaticTool {
    Bash(BashTool),
    ReadFile(ReadFileTool),
    ApplyPatch(ApplyPatchTool),
    Glob(GlobTool),
    Grep(GrepTool),
    WebFetch(WebFetchTool),
}

impl StaticTool {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Bash(t) => t.name(),
            Self::ReadFile(t) => t.name(),
            Self::ApplyPatch(t) => t.name(),
            Self::Glob(t) => t.name(),
            Self::Grep(t) => t.name(),
            Self::WebFetch(t) => t.name(),
        }
    }

    pub fn definition(&self) -> ToolDefinition {
        match self {
            Self::Bash(t) => t.definition(),
            Self::ReadFile(t) => t.definition(),
            Self::ApplyPatch(t) => t.definition(),
            Self::Glob(t) => t.definition(),
            Self::Grep(t) => t.definition(),
            Self::WebFetch(t) => t.definition(),
        }
    }

    pub async fn execute(&self, input: serde_json::Value) -> ToolResult {
        match self {
            Self::Bash(t) => t.execute(input).await,
            Self::ReadFile(t) => t.execute(input).await,
            Self::ApplyPatch(t) => t.execute(input).await,
            Self::Glob(t) => t.execute(input).await,
            Self::Grep(t) => t.execute(input).await,
            Self::WebFetch(t) => t.execute(input).await,
        }
    }

    pub fn name_str(&self) -> &str {
        match self {
            Self::Bash(_) => "bash",
            Self::ReadFile(_) => "read_file",
            Self::ApplyPatch(_) => "apply_patch",
            Self::Glob(_) => "glob",
            Self::Grep(_) => "grep",
            Self::WebFetch(_) => "web_fetch",
        }
    }
}

pub struct StaticToolRegistry {
    tools: Vec<StaticTool>,
}

impl StaticToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    #[must_use]
    pub fn with_default_tools(working_dir: &str) -> Self {
        Self {
            tools: vec![
                StaticTool::Bash(BashTool::new(working_dir)),
                StaticTool::ReadFile(ReadFileTool::new(working_dir)),
                StaticTool::ApplyPatch(ApplyPatchTool::new(working_dir)),
                StaticTool::Glob(GlobTool::new(working_dir)),
                StaticTool::Grep(GrepTool::new(working_dir)),
                StaticTool::WebFetch(
                    WebFetchTool::new(WebFetchConfig::default())
                        .expect("Failed to create WebFetchTool"),
                ),
            ],
        }
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    pub async fn execute(&self, name: &str, input: serde_json::Value) -> ToolResult {
        let started = std::time::Instant::now();

        let result = match self.tools.iter().find(|t| t.name_str() == name) {
            Some(tool) => tool.execute(input).await,
            None => return ToolResult::error(format!("Unknown tool: {name}"))
                .with_error_type("unknown_tool"),
        };

        let mut result = result;
        result.duration_ms = Some(started.elapsed().as_millis());
        result.bytes = result.content.len();
        if result.is_error && result.error_type.is_none() {
            result.error_type = Some("tool_error".to_string());
        }
        if result.status_code.is_none() {
            result.status_code = Some(i32::from(result.is_error));
        }
        result
    }
}

impl Default for StaticToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```
