# nanors - AI Assistant Rust Implementation

nanors 的 Rust 实现，采用渐进式开发策略和 workspace 架构。

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
├── debug.sh               # 格式化 + clippy 检查
├── fix.sh                 # 自动修复 clippy 警告
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

## 配置和数据文件

所有配置和数据文件统一放在 `~/nanors` 目录：

```
~/nanors/
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

这将直接创建 `~/nanors/config.json` 配置文件。

### 2. 编辑配置

```bash
# 编辑 ~/nanors/config.json，填入你的智谱 API Key
```

配置文件格式：

```json
{
  "agents": {
    "defaults": {
      "model": "glm-4.7",
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

## 命令说明

### `nanors agent`

运行 AI 助手。

**选项：**
- `-m, --message <MESSAGE>`: 发送单次消息
- `-M, --model <MODEL>`: 指定使用的模型

**示例：**

```bash
# 交互式对话
nanors agent

# 单次查询
nanors agent -m "今天天气怎么样？"

# 指定模型
nanors agent -m "你好" -M glm-4.7
```

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
- ✅ 基础工具框架
- ✅ Workspace 架构（5 个 crate）
- ✅ 完整的 clippy 检查（pedantic、nursery 等）
- ✅ 生产级技术栈（与 pmi-rust-backend 一致）
- ✅ 所有配置和数据统一在 `~/nanors` 目录

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
~/nanors/
├── config.json          # 配置文件（必需）
└── sessions.db         # 数据库文件（自动创建）
```

## 许可证

MIT
