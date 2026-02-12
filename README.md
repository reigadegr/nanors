# nanors - AI Assistant Rust Implementation

nanors 的 Rust 实现，采用渐进式开发策略和 workspace 架构。

## 架构

本项目采用 workspace 架构，按功能拆分为多个 crate：

- **app**: CLI 入口
- **nanors_core**: 核心抽象（agent, tools, 配置）
- **nanors_providers**: LLM Provider 实现（智谱 GLM）
- **nanors_conversation**: 多轮对话管理（持久化会话）
- **nanors_memory**: 长期记忆管理（向量检索 + Rerank）
- **nanors_entities**: 数据库实体（Sea-ORM 生成）
- **nanors_config**: 配置管理
- **nanors_telegram**: Telegram Bot 集成

```
nanors/
├── Cargo.toml              # workspace 配置
├── debug.sh               # 格式化 + clippy 检查
├── fix.sh                 # 自动修复 clippy 警告
├── app/                  # CLI 入口
│   └── src/
│       ├── main.rs
│       └── command/      # 命令实现
│           ├── mod.rs
│           ├── agent.rs  # 单轮对话命令
│           └── chat.rs   # 多轮对话命令（新）
├── nanors_core/          # 核心抽象
│   ├── src/lib.rs
│   ├── src/agent/
│   └── src/tools/
├── nanors_providers/      # LLM Provider (智谱 GLM)
│   └── src/
├── nanors_conversation/   # 多轮对话管理（新）
│   └── src/
│       ├── manager.rs    # ConversationManager
│       ├── history.rs    # HistoryManager
│       └── session.rs    # 会话状态
├── nanors_memory/         # 长期记忆管理
│   └── src/
│       ├── mod.rs
│       ├── rerank/       # 检索重排序
│       └── query/        # 问题类型检测
├── nanors_entities/       # 数据库实体
│   └── src/              # Sea-ORM 生成
└── nanors_config/        # 配置管理
    └── src/
```

## 配置和数据文件

所有配置和数据文件统一放在 `~/.nanors` 目录：

```
~/.nanors/
├── config.json          # 配置文件（必需，init 时创建）
└── sessions.db         # 数据库文件（自动创建）
```

## 技术栈

基于 `pmi-rust_backend` 经过生产验证的技术栈：

| 依赖 | 版本 | 说明 |
|------|------|------|
| tokio | 1.49.0 | 异步运行时 |
| serde | 1.0.228 | 序列化/反序列化 |
| sea-orm | 2.0.0-rc.30 | ORM 框架 |
| sqlx | 0.8 | 数据库驱动（PostgreSQL, MySQL, SQLite） |
| async-trait | 0.1.89 | 异步 trait |
| anyhow | 1.0.100 | 错误处理 |
| tracing | 0.1.44 | 结构化日志 |
| reqwest | 0.12 | HTTP 客户端 |
| clap | 4.5 | CLI 解析 |

### Sea-ORM 配置

```toml
sea-orm = { version = "2.0.0-rc.30", features = [
  "sqlx-postgres",
  "sqlx-mysql",
  "sqlx-sqlite",
  "runtime-tokio-rustls",
  "with-chrono",
  "debug-print",
  "macros",
  "with-uuid",
  "with-json",
] }
```

## 快速开始

### 1. 初始化配置

```bash
nanors init
```

这将直接创建 `~/.nanors/config.json` 配置文件。

### 2. 编辑配置

```bash
# 编辑 ~/.nanors/config.json，填入你的智谱 API Key
```

配置文件格式：

```json
{
  "agents": {
    "defaults": {
      "model": "glm-4-flash",
      "max_tokens": 8192,
      "temperature": 0.7,
      "system_prompt": "You are a helpful AI assistant with memory of past conversations. Provide clear, concise responses.",
      "history_limit": 20
    }
  },
  "providers": {
    "zhipu": {
      "api_key": "your-zhipu-api-key-here"
    }
  },
  "database": {
    "url": "postgresql://reigadegr:1234@localhost:5432/nanors"
  },
  "memory": {
    "default_user_scope": "default",
    "retrieval": {
      "items_top_k": 10,
      "context_target_length": 2000,
      "adaptive": {
        "min_results": 5,
        "max_results": 100000
      }
    }
  },
  "telegram": {
    "token": "your-telegram-bot-token-here",
    "allow_from": []
  }
}
```

**配置说明：**

| 字段 | 说明 | 默认值 |
|------|------|--------|
| `agents.defaults.model` | 使用的模型 | `glm-4-flash` |
| `agents.defaults.max_tokens` | 最大 token 数 | `8192` |
| `agents.defaults.temperature` | 温度参数 | `0.7` |
| `agents.defaults.history_limit` | 多轮对话历史条数 | `20` |
| `database.url` | 数据库连接 URL | PostgreSQL 格式 |

### Telegram Bot 配置

| 字段 | 说明 | 默认值 |
|------|------|--------|
| `telegram.enabled` | 是否启用 Telegram Bot | `false` |
| `telegram.token` | Bot Token（从 @BotFather 获取） | 空 |
| `telegram.allow_from` | 允许的用户/群组 ID 列表（空=全部允许） | `[]` |

### 3. 运行

#### 单轮对话（无状态）

交互式对话（工具调用默认开启）：

```bash
nanors agent
```

单次查询：

```bash
nanors agent -m "你好"
```

指定模型：

```bash
nanors agent -m "你好" --model glm-4.7
```

指定工具工作目录：

```bash
nanors agent -d /path/to/project
```

#### 多轮对话（持久化会话）**新功能**

启动带会话持久化的对话：

```bash
nanors chat
```

指定会话名称：

```bash
nanors chat --name "项目讨论"
```

恢复已有会话：

```bash
nanors chat --session <SESSION_ID>
```

单次消息发送（非交互模式）：

```bash
nanors chat -m "接着刚才的话题继续"
```

自定义历史记录条数：

```bash
nanors chat --history-limit 50
```

## 命令说明

### `nanors agent` - 单轮对话

运行 AI 助手（无状态模式，每次对话独立）。工具调用默认开启。

**选项：**
- `-m, --message <MESSAGE>`: 发送单次消息
- `-M, --model <MODEL>`: 指定使用的模型
- `-d, --working-dir <DIR>`: 指定工具工作目录（默认当前目录）

**工具调用（默认开启）：**
- `bash` - 执行 shell 命令
- `read_file` - 读取文件
- `write_file` - 写入文件
- `edit_file` - 编辑文件
- `glob` - 文件模式匹配
- `grep` - 内容搜索

**示例：**

```bash
# 交互式对话（工具默认开启）
nanors agent

# 单次查询
nanors agent -m "今天天气怎么样？"

# 指定模型
nanors agent -m "你好" -M glm-4.7

# 指定项目目录
nanors agent -d /path/to/project
```

### `nanors chat` - 多轮对话 **新功能**

运行持久化会话的多轮对话（会话状态自动保存）。

**选项：**
- `-m, --message <MESSAGE>`: 发送单次消息（非交互模式）
- `-M, --model <MODEL>`: 指定使用的模型
- `--name <NAME>`: 设置会话名称
- `--session <SESSION_ID>`: 恢复已有会话
- `--history-limit <N>`: 设置历史记录条数

**示例：**

```bash
# 启动新的多轮对话
nanors chat

# 命名会话
nanors chat --name "学习 Rust"

# 恢复已有会话
nanors chat --session 0193abcd-1234-5678-9012-abcdef123456

# 单次消息（会话继续）
nanors chat -m "继续刚才的话题"

# 设置历史记录条数
nanors chat --history-limit 50
```

**特性：**
- 会话自动保存到数据库
- 支持上下文记忆（可配置历史窗口）
- 支持会话恢复
- Token 使用统计

### `nanors telegram` - Telegram Bot **新功能**

启动 Telegram Bot，持续监听并响应 Telegram 消息。工具调用默认开启。

**选项：**
- `-t, --token <TOKEN>`: 覆盖配置文件中的 Bot Token
- `-a, --allow_from <IDS>`: 允许的用户/群组 ID（逗号分隔）

**工具调用（默认开启）：**
- `bash` - 执行 shell 命令
- `read_file` - 读取文件
- `write_file` - 写入文件
- `edit_file` - 编辑文件
- `glob` - 文件模式匹配
- `grep` - 内容搜索

注意：工具使用 bot 启动时的当前目录作为工作目录。

**示例：**

```bash
# 使用配置文件中的 token 启动
nanors telegram

# 覆盖 token
nanors telegram -t "1234567890:ABCdefGHIjklMNOpqrsTUVwxyz"

# 只允许特定用户访问
nanors telegram -a "123456789,987654321"
```

**使用步骤：**

1. 在 Telegram 中找到 [@BotFather](https://t.me/BotFather)
2. 发送 `/newbot` 创建新机器人，获取 Token
3. 编辑 `~/.nanors/config.json`，填入 Token
4. 运行 `nanors telegram` 启动机器人
5. 在 Telegram 中找到你的机器人，开始对话

**支持的命令：**
- `/start` - 开始使用机器人
- `/reset` - 重置对话历史
- `/help` - 显示帮助信息

**特性：**
- 持续运行监听消息（无需 webhook）
- 每个用户/群组独立会话
- 支持长期记忆检索
- 工具调用支持（bash、文件操作等）
- Ctrl+C 优雅退出

### `nanors init`

初始化配置文件。

```bash
nanors init
```

- 如果配置文件已存在，会提示用户直接编辑
- 如果配置文件不存在，会创建新的配置文件

### `nanors version`

显示版本信息。

```bash
nanors version
```

## 开发

### 构建

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release
```

二进制文件位于 `target/release/nanors`。

### 代码检查

**格式化 + 检查：**

```bash
./debug.sh
```

该脚本会：
1. 运行 `cargo fmt` 格式化 Rust 代码
2. 运行 `taplo fmt` 格式化所有 TOML 文件
3. 运行 `cargo clippy` 检查代码质量

**自动修复：**

```bash
./fix.sh
```

该脚本会自动修复 clippy 警告。

**编译选项：**

```bash
export RUSTFLAGS="-Z function-sections=yes -C link-arg=-fuse-ld=/usr/bin/mold -C link-args=-Wl,--gc-sections,--as-needed"
```

## 第一阶段功能

- ✅ CLI 工具
- ✅ 智谱 GLM 集成
- ✅ SQLite 会话持久化（Sea-ORM）
- ✅ 工具调用框架（默认开启）
  - bash、read_file、write_file、edit_file、glob、grep
- ✅ Workspace 架构（6 个 crate）
- ✅ 完整的 clippy 检查（pedantic、nursery 等）
- ✅ 生产级技术栈（与 pmi-rust-backend 一致）
- ✅ 所有配置和数据统一在 `~/.nanors` 目录

## 第二阶段功能（新增）

- ✅ 多轮对话管理（`nanors_conversation`）
  - 会话持久化
  - 上下文历史管理
  - 会话恢复支持
  - Token 使用统计
- ✅ 长期记忆管理（`nanors_memory`）
  - 向量检索
  - 问题类型检测
  - 智能重排序（Rerank）
- ✅ IvorySQL/PostgreSQL 数据库支持
- ✅ 0 clippy 警告（pedantic + nursery 标准）
- ✅ Telegram Bot 集成（`nanors_telegram`）
  - 持续监听消息（long polling 模式）
  - 命令支持（/start, /reset, /help）
  - 用户会话隔离
  - 访问控制（allow_from 白名单）

## 代码规范

所有代码遵循严格的 clippy 规则：

```rust
#![deny(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::correctness,
    clippy::suspicious,
    clippy::unwrap_used,
    clippy::expect_used
)]
```

## 性能优势

与 Python 实现相比：

| 指标 | Python | Rust | 提升 |
|------|--------|-------|------|
| 启动时间 | ~500ms | ~50ms | 10x |
| 内存占用 | ~150MB | ~30MB | 5x |
| 二进制大小 | N/A | ~5MB | - |

## 数据库支持

通过 Sea-ORM 支持多种数据库：

- IvorySQL / PostgreSQL（默认）
- MySQL
- SQLite

默认使用 IvorySQL（兼容 PostgreSQL），可根据需要切换到其他数据库。

## 文件结构

```
~/.nanors/
├── config.json          # 配置文件（必需）
└── sessions.db         # 数据库文件（自动创建）
```

## 许可证

MIT
