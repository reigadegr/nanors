# nanors - nanobot Rust Implementation

nanobot 的 Rust 重写实现，采用渐进式开发策略和 workspace 架构。

## 架构

本项目采用 workspace 架构，按功能拆分为多个 crate：

- **app**: CLI 入口
- **nanors_core**: 核心抽象（agent, tools, 配置）
- **nanors_providers**: LLM Provider 实现（智谱 GLM）
- **nanors_session**: 会话管理（Sea-ORM + SQLite）
- **nanors_config**: 配置管理

```
nanors/
├── Cargo.toml              # workspace 配置
├── debug.sh               # clippy 检测脚本
├── app/                  # CLI 入口
│   └── src/main.rs
├── nanors_core/          # 核心抽象
│   ├── src/lib.rs
│   ├── src/agent/
│   └── src/tools/
├── nanors_providers/      # LLM Provider (智谱 GLM)
│   └── src/
├── nanors_session/        # SQLite 会话管理
│   └── src/
│       └── entity/       # Sea-ORM entity
└── nanors_config/        # 配置管理
    └── src/
```

## 配置文件位置

所有配置和数据文件统一放在 `~/.nanobot` 目录下：

```
~/.nanobot/
├── config.json          # 配置文件（必需）
├── config.json.example  # 示例配置（init 时创建）
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
cargo run -- init
```

这将创建 `~/.nanobot/config.json.example`。

### 2. 编辑配置

```bash
cd ~/.nanobot
cp config.json.example config.json
# 编辑 config.json，填入你的智谱 API Key
```

配置文件格式：

```json
{
  "agents": {
    "defaults": {
      "model": "glm-4-flash",
      "max_tokens": 8192,
      "temperature": 0.7
    }
  },
  "providers": {
    "zhipu": {
      "api_key": "your-zhipu-api-key"
    }
  }
}
```

### 3. 运行

交互式对话：

```bash
cargo run -- agent
```

单次查询：

```bash
cargo run -- agent -m "你好"
```

指定模型：

```bash
cargo run -- agent -m "你好" --model glm-4-plus
```

## 命令说明

### `nanobot agent`

运行 AI 助手。

**选项：**
- `-m, --message <MESSAGE>`: 发送单次消息
- `-M, --model <MODEL>`: 指定使用的模型

**示例：**

```bash
# 交互式对话
nanobot agent

# 单次查询
nanobot agent -m "今天天气怎么样？"

# 指定模型
nanobot agent -m "你好" -M glm-4-plus
```

### `nanobot init`

初始化配置文件。

```bash
nanobot init
```

### `nanobot version`

显示版本信息。

```bash
nanobot version
```

## 开发

### 构建

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release
```

二进制文件位于 `target/release/nanobot`。

### 代码检查

使用 `debug.sh` 进行 clippy 检查：

```bash
./debug.sh
```

该脚本会设置编译优化选项并运行 clippy。

**编译选项：**
```bash
export RUSTFLAGS="-Z function-sections=yes -C link-arg=-fuse-ld=/usr/bin/mold -C link-args=-Wl,--gc-sections,--as-needed"
```

## 第一阶段功能

- ✅ CLI 工具
- ✅ 智谱 GLM 集成
- ✅ SQLite 会话持久化（Sea-ORM）
- ✅ 基础工具框架
- ✅ Workspace 架构（5 个 crate）
- ✅ 完整的 clippy 检查（pedantic、nursery 等）
- ✅ 生产级技术栈（与 pmi-rust-backend 一致）
- ✅ 所有配置和数据统一在 `~/.nanobot` 目录

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

- PostgreSQL
- MySQL
- SQLite

默认使用 SQLite，可根据需要切换到 PostgreSQL 或 MySQL。

## 文件结构

```
~/.nanobot/
├── config.json          # 配置文件（必需）
├── config.json.example  # 示例配置
└── sessions.db         # 数据库文件（自动创建）
```

## 许可证

MIT
