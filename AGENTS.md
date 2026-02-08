# AGENTS.md

## 构建和测试命令

### 构建
```bash
cargo build                           # 开发构建
cargo build --release                 # 发布构建
```

### 代码检查和格式化
```bash
./debug.sh                            # 格式化 + clippy 检查（推荐）
./fix.sh                              # 自动修复 clippy 警告
cargo fmt                             # 仅格式化 Rust 代码
taplo fmt Cargo.toml */*.toml         # 格式化 TOML 文件
cargo clippy --all-targets --all-features  # 手动 clippy 检查
cargo clippy --fix --allow-dirty      # 自动修复 clippy 警告
```

### 测试
```bash
cargo test                            # 运行所有测试
cargo test <test_name>                # 运行单个测试
cargo test --package <crate>          # 运行指定 crate 的测试
cargo nextest run                     # 使用 nextest 运行测试（如果已安装）
```

## 工作流程要求

### 代码修改后必须执行
1. **运行 `./debug.sh`** - 格式化 + clippy 检查（**唯一允许的构建/检查命令**）
2. **简单功能测试** - 验证修改的基本功能正常
3. **所有 clippy 警告必须修复** - 不得使用 `#[allow(...)]` 屏蔽警告
4. **保持文件头部 linting 配置完整** - 不得修改或移除 `main.rs` 和 `lib.rs` 中的 `#![deny(...)]` 和 `#![allow(...)]` 配置

### 禁止行为
- ❌ 使用 `#[allow(...)]` 屏蔽 clippy 警告
- ❌ 修改 `main.rs` 或 `lib.rs` 的 linting 配置
- ❌ 移除或注释掉文件头部的 `#![deny(...)]` 或 `#![allow(...)]`
- ❌ 跳过 `./debug.sh` 执行
- ❌ 提交未通过 clippy 检查的代码
- ❌ 使用 `cargo build` 检测代码能否编译（只允许使用 `./debug.sh`）
- ❌ 以 release 模式编译（除非主动要求）

### 提交前检查清单
- [ ] 执行 `./debug.sh` 无警告
- [ ] 简单功能测试通过
- [ ] 无新增 `#[allow(...)]` 属性
- [ ] 文件头部 linting 配置未修改

## 代码风格指南

### 文件头部
每个 `.rs` 文件必须包含严格的 linting 配置：
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
#![allow(
    clippy::similar_names,
    clippy::missing_safety_doc,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]
```

### 错误处理
- 使用 `anyhow::Result<T>` 作为应用代码的默认错误类型
- 使用 `thiserror` 创建库的自定义错误类型
- 避免 `unwrap()` - 使用 `?` 运算符传播错误
- 仅在值保证存在时使用 `expect()`，而非 `unwrap()`
- 使用 `assert!` 在函数入口验证不变量
- 在可用时使用 anyhow 的 `.context()` 提供上下文

### 异步代码
- 使用 `tokio` 作为异步运行时
- 使用 `#[tokio::main]` 或 `#[tokio::test]` 标记异步函数
- 使用 `#[async_trait]` 宏实现异步 trait
- 存储为 trait 对象时，所有异步 trait 必须包含 `Send + Sync` 约束

### 模块组织
- 使用 `pub mod` 声明子模块
- 使用 `pub use` 在 crate 根部重新导出常用类型
- 保持模块结构扁平且逻辑清晰
- 按照工作空间结构将关注点分离到不同的 crate

### 命名规范
- 结构体：`PascalCase`（如 `AgentConfig`, `ChatMessage`）
- 枚举：`PascalCase`（如 `Role`, `Commands`）
- 函数：`snake_case`（如 `process_message`, `run_interactive`），不要使用 `get_` 前缀
- 常量：`SCREAMING_SNAKE_CASE`
- Trait：`PascalCase`（如 `LLMProvider`, `Tool`）
- 转换方法：`as_`（廉价引用）、`to_`（昂贵转换）、`into_`（所有权转移）
- 迭代器：`iter()`（引用）、`iter_mut()`（可变引用）、`into_iter()`（消费）
- 使用能清晰表达意图的描述性名称

### 导入
- 外部导入（std/crates）放在内部导入（workspace）之前
- 在组内按字母顺序组织导入
- 优先使用 `use crate::` 进行内部导入，而非相对路径
- 保持导入简洁干净

### 属性
- 在修改 `self` 的 builder 方法上使用 `#[must_use]`
- 在数据承载的结构体上使用 `#[derive(Debug, Clone)]`
- 使用 `#[derive(Serialize, Deserialize)]` 为需要序列化的类型添加 serde
- 使用 `#[async_trait]` 实现异步 trait

### 日志
- 使用 `tracing` 进行结构化日志
- 日志级别：`trace!`, `debug!`, `info!`, `warn!`, `error!`
- 在日志消息中提供上下文（如模型名称、会话密钥）
- 在 `main()` 中使用适当的过滤器初始化 tracing subscriber

### 文档
- 为公共 API 添加文档注释（`///`）
- 在文档注释中包含使用示例
- 记录相关的错误条件
- 保持文档简洁清晰

### 配置和数据
- 使用 `serde` 进行序列化/反序列化
- 使用 `chrono` 处理日期时间
- 使用 `uuid` 的 `v7` 特性生成唯一标识符
- 使用 `sea-orm` 进行数据库操作

### 数据类型
- 优先使用 `&str` 而非 `String` 作为函数参数（除非需要所有权）
- 使用 `bytes::Bytes` 处理二进制数据
- 使用 `Cow<str>` 当可能需要修改借用数据时
- 避免嵌套迭代（字符串 `contains()` 是 O(n*m)）

### 性能优化
- 预分配：`Vec::with_capacity()`, `String::with_capacity()`
- ASCII 处理使用 `s.bytes()` 而非 `s.chars()`
- 使用 `format!` 代替字符串连接 `+`
- 使用 `assert!` 和 `debug_assert!` 进行运行时检查
- 使用 `#[inline]` 对小型函数进行内联优化

## 工作空间结构

项目组织为 5 个 crate：
- **app**：CLI 入口点（`app/src/main.rs`）
- **nanors_core**：核心抽象（agent, tools, LLM traits）
- **nanors_providers**：LLM provider 实现（如 ZhipuProvider）
- **nanors_session**：使用 Sea-ORM 的会话管理
- **nanors_config**：配置管理

所有工作空间依赖项都定义在根目录的 `Cargo.toml` 中的 `[workspace.dependencies]` 下。

## 运行时配置

Rust 版本：2024
最低 rust-version：1.85
异步运行时：tokio 1.49.0

使用优化构建时，设置：
```bash
export RUSTFLAGS="-Z function-sections=yes -C link-arg=-fuse-ld=/usr/bin/mold -C link-args=-Wl,--gc-sections,--as-needed"
```

## 已弃用模式 → 推荐替代

| 已弃用 | 推荐替代 | 原因 |
|--------|----------|------|
| `lazy_static!` | `std::sync::OnceLock` (1.70) | 标准库支持 |
| `once_cell::Lazy` | `std::sync::LazyLock` (1.80) | 标准库支持 |
| `try!()` | `?` 运算符 | 更简洁 |
| `failure` / `error-chain` | `thiserror` / `anyhow` | 现代 Rust 标准 |
| `mem::uninitialized()` | `MaybeUninit<T>` | 更安全 |

## 最佳实践

1. **优先使用组合而非继承** - 使用 trait 实现多态
2. **使用 builder 模式** 构建复杂对象
3. **实现 Default** 为配置结构体提供合理的默认值
4. **使用 Arc<dyn Trait>** 共享异步 trait 对象
5. **保持异步函数可取消** - 避免跨 await 点持有锁
6. **使用 tracing instrument** 为复杂的异步函数添加 `#[tracing::instrument]`
7. **预分配容器** - 使用 `Vec::with_capacity()` 和 `String::with_capacity()`
8. **使用 newtypes** - 用 `struct Email(String)` 表达领域语义
9. **原子操作用于简单类型** - 使用 `AtomicUsize` 而非 `Mutex<usize>`
10. **异步用于 I/O，同步用于 CPU** - CPU 密集型任务使用 `spawn_blocking`

## Unsafe 代码规范

仅在必要时使用 unsafe：

### 合法的 unsafe 用途
- FFI（调用 C 函数）
- 实现底层抽象（如 `Vec`, `Arc`）
- 性能优化（在有安全的替代方案且确实太慢时）

### 必需的文档
```rust
// SAFETY: <为什么这是安全的>
unsafe { ... }

/// # Safety
/// <调用者需要满足的要求>
pub unsafe fn dangerous() { ... }
```

### 快速参考
| 操作 | 安全要求 |
|------|----------|
| `*ptr` 解引用 | 有效、对齐、已初始化 |
| `&*ptr` | + 无别名违规 |
| `transmute` | 相同大小、有效位模式 |
| `extern "C"` | 正确的签名、ABI |
| `static mut` | 保证同步 |
| `impl Send/Sync` | 实际线程安全 |
