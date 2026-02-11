# Telegram Bot 集成计划

## 项目概述

将 nanobot 的 Telegram 交流功能移植到 nanors，使用 Teloxide 库实现。

## 架构设计

### 新建 Crate: `nanors_telegram`

**位置**: `/home/reigadegr/project/nanors/nanors_telegram/`

**依赖关系**:
```
nanors_telegram
    ├─> nanors_core (LLMProvider, ChatMessage, Role)
    ├─> nanors_providers (ZhipuProvider)
    ├─> nanors_config (Config)
    ├─> nanors_memory (MemoryManager)
    ├─> nanors_conversation (ConversationManager)
    ├─> nanors_entities (可选，数据库实体)
    └─> teloxide (Telegram Bot API)
```

### 核心模块设计

```
nanors_telegram/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 公共导出
│   ├── bot.rs              # TelegramBot 主结构
│   ├── handler.rs          # 消息处理器
│   ├── command.rs          # 命令定义和解析
│   ├── session.rs          # Telegram 会话管理
│  ── storage.rs            # PostgreSQL 对话存储 (可选)
│   └── error.rs            # 错误类型
└── examples/
    └── simple_bot.rs       # 示例
```

### 关键组件

#### 1. TelegramBot 结构

```rust
pub struct TelegramBot {
    bot: Bot,
    provider: Arc<dyn LLMProvider>,
    memory_manager: Arc<MemoryManager>,
    config: TelegramConfig,
    // 使用 Arc<Mutex<HashMap<>> 存储用户会话状态
    // 或使用 teloxide 的 Dialogue + PostgresStorage
}
```

#### 2. 命令定义

```rust
#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "支持以下命令")]
pub enum Command {
    #[command(description = "开始使用机器人")]
    Start,
    #[command(description = "重置对话历史")]
    Reset,
    #[command(description = "显示帮助信息")]
    Help,
}
```

#### 3. 消息处理流程

```
Telegram Message
    │
    v
Dispatcher (teloxide)
    │
    ├─> 命令处理 (/start, /reset, /help)
    │       └─> 更新会话状态
    │
    └─> 普通消息
            │
            v
    获取/创建用户会话 (按 chat_id 隔离)
            │
            v
    构造 ChatMessage{User, content}
            │
            v
    ConversationManager::process_turn()
            │
            ├─> 检索相关记忆 (MemoryManager::search_enhanced)
            ├─> 调用 LLMProvider::chat()
            └─> 保存新记忆
            │
            v
    构造响应消息
            │
            v
    发送到 Telegram (bot.send_message)
```

### 配置扩展

在 `nanors_config/src/schema.rs` 中添加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub token: String,
    pub allow_from: Vec<String>,  // 允许的用户/群组 ID
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            allow_from: vec![],
        }
    }
}

// 在 Config 中添加
pub struct Config {
    pub agents: AgentsConfig,
    pub providers: ProvidersConfig,
    pub database: DatabaseConfig,
    pub memory: MemoryConfig,
    pub telegram: TelegramConfig,  // 新增
}
```

## 实施步骤

### Phase 1: 基础框架 (第 1 步)

1. 创建 `nanors_telegram` crate
2. 添加 Teloxide 依赖
3. 定义基本结构和错误类型
4. 实现简单的 REPL 模式 bot

### Phase 2: 命令系统 (第 2 步)

1. 定义 Command 枚举
2. 实现命令处理器
3. 集成 Dispatcher

### Phase 3: 会话管理 (第 3 步)

1. 实现 Telegram 会话存储
2. 按 chat_id 隔离用户会话
3. 集成 ConversationManager

### Phase 4: AI 对话 (第 4 步)

1. 集成 LLMProvider
2. 集成 MemoryManager
3. 实现完整的对话流程

### Phase 5: 配置和启动 (第 5 步)

1. 扩展 nanors_config
2. 在 app/src/main.rs 添加 telegram 命令
3. 添加配置文件支持

### Phase 6: 高级功能 (可选)

1. 多媒体消息支持（图片、语音、文档）
2. Markdown/HTML 格式化
3. 打字指示器
4. 错误恢复和重试

## 依赖添加

### workspace Cargo.toml

```toml
[workspace.dependencies]
teloxide = { version = "0.17", features = ["macros", "ctrlc_handler"] }
teloxide-core = "0.13"
dptree = "0.3"
```

### nanors_telegram/Cargo.toml

```toml
[package]
name = "nanors_telegram"
version.workspace = true
edition.workspace = true

[dependencies]
nanors_core = { path = "../nanors_core" }
nanors_providers = { path = "../nanors_providers" }
nanors_config = { path = "../nanors_config" }
nanors_memory = { path = "../nanors_memory" }
nanors_conversation = { path = "../nanors_conversation" }

teloxide = { workspace = true, features = ["macros", "ctrlc_handler"] }
dptree = { workspace = true }

tokio = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
serde = { workspace = true }
```

## 设计原则

1. **最少代码**: 保持代码简洁，避免过度工程化
2. **可读性**: 清晰的模块结构和命名
3. **可维护性**: 利用现有的 nanors 抽象
4. **线程安全**: 使用 Arc 和适当的同步原语（domain-web 约束）
5. **错误处理**: 使用 anyhow::Result 和 thiserror

## 用户会话隔离策略

- 使用 `tg:{chat_id}` 格式作为 `user_scope`
- 每个 Telegram chat 独立的 `session_id` (Uuid)
- 会话状态存储在 MemoryManager 的 sessions 表中

## 技术亮点

1. **Dispatcher 模式**: 使用 teloxide 的 Dispatcher 进行声明式消息处理
2. **依赖注入**: 通过 dptree 注入 provider, memory_manager, config
3. **会话持久化**: 利用现有的 PostgreSQL 存储，无需额外依赖
4. **记忆系统**: 自动保存重要对话到长期记忆
