# MicroClaw → Nanors 工具调用能力移植分析报告

## 1. 概述

本报告详细分析了将 microclaw 项目的工具调用能力移植到 nanors 项目的完整方案。microclaw 拥有完善的工具系统，包括 21+ 内置工具、工具注册表、权限控制和 Agent 循环集成。

## 2. 架构对比

### 2.1 MicroClaw 架构

```
microclaw/
├── src/
│   ├── tools/
│   │   ├── mod.rs              # Tool trait, ToolRegistry, ToolResult, 安全机制
│   │   ├── bash.rs             # Shell 命令执行
│   │   ├── read_file.rs         # 文件读取
│   │   ├── write_file.rs        # 文件写入
│   │   ├── edit_file.rs         # 文件编辑
│   │   ├── glob.rs              # 文件模式匹配
│   │   ├── grep.rs              # 正则搜索
│   │   ├── memory/              # 记忆工具 (read_memory, write_memory)
│   │   ├── web_fetch.rs         # Web 抓取
│   │   ├── web_search.rs        # Web 搜索
│   │   ├── web_html.rs          # HTML 处理
│   │   ├── send_message.rs      # 消息发送
│   │   ├── schedule/            # 定时任务工具
│   │   ├── export_chat.rs        # 聊天导出
│   │   ├── sub_agent.rs          # 子代理
│   │   ├── activate_skill.rs     # 激活技能
│   │   ├── sync_skills.rs        # 同步技能
│   │   ├── todo/                # 待办事项
│   │   ├── browser.rs           # 浏览器自动化
│   │   ├── command_runner.rs     # 命令运行辅助
│   │   ├── path_guard.rs        # 路径安全保护
│   │   └── mcp.rs               # MCP 服务器集成
│   ├── agent_engine.rs           # Agent 循环 + 工具调用
│   ├── claude.rs                # Claude API 消息格式
│   ├── runtime.rs               # AppState 包含 ToolRegistry
│   └── config.rs               # WorkingDirIsolation 配置
```

**核心组件：**

| 组件 | 位置 | 职责 |
|------|------|------|
| `Tool` trait | tools/mod.rs:215-220 | 工具接口定义 |
| `ToolRegistry` | tools/mod.rs:222-462 | 工具注册表 |
| `ToolResult` | tools/mod.rs:35-78 | 工具执行结果 |
| `ToolAuthContext` | tools/mod.rs:148-163 | 权限上下文 |
| `process_with_agent_impl` | agent_engine.rs:129-435 | Agent 主循环 |

### 2.2 Nanors 当前架构

```
nanors/
├── nanors_core/
│   ├── lib.rs                 # LLMProvider, SessionStorage traits
│   ├── agent/
│   │   ├── mod.rs
│   │   └── agent_loop.rs       # AgentLoop (无工具调用)
│   ├── memory/
│   ├── retrieval/
│   └── util.rs
├── nanors_providers/
│   ├── zhipu.rs              # ZhipuAI provider (无工具调用)
│   └── lib.rs
├── nanors_config/
├── nanors_entities/
├── nanors_memory/
├── nanors_conversation/
└── nanors_telegram/
```

**核心组件：**

| 组件 | 位置 | 职责 |
|------|------|------|
| `LLMProvider` | nanors_core/src/lib.rs:66-70 | LLM 提供者接口 |
| `AgentLoop` | agent/agent_loop.rs:54-324 | Agent 循环 (无工具) |
| `ChatMessage` | nanors_core/src/lib.rs:46-50 | 消息格式 |
| `ZhipuProvider` | nanors_providers/src/zhipu.rs:88-140 | 智谱 AI 实现 |

## 3. 差异分析

### 3.1 消息格式

**MicroClaw (Claude API 格式):**
```rust
// src/claude.rs
pub enum ContentBlock {
    Text { text: String },
    Image { source: ImageSource },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, is_error: Option<bool> },
}

pub struct Message {
    pub role: String,  // "user" | "assistant" | "system"
    pub content: MessageContent,  // Text(String) | Blocks(Vec<ContentBlock>)
}
```

**Nanors (简单格式):**
```rust
// nanors_core/src/lib.rs
pub enum Role { User, Assistant, System, Tool }

pub struct ChatMessage {
    pub role: Role,
    pub content: String,  // 纯文本，无 blocks
}
```

**影响：** 需要扩展 `ChatMessage` 支持复杂内容块。

### 3.2 Provider 接口

**MicroClaw:**
```rust
pub trait LlmProvider: Send + Sync {
    async fn send_message(
        &self,
        system: &str,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> anyhow::Result<MessagesResponse>;

    async fn send_message_stream(
        &self,
        system: &str,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        tx: Option<&UnboundedSender<String>>,
    ) -> anyhow::Result<MessagesResponse>;
}
```

**Nanors:**
```rust
pub trait LLMProvider: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        model: &str,
    ) -> anyhow::Result<LLMResponse>;

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
}
```

**影响：** 需要扩展 `LLMProvider` 支持工具调用。

### 3.3 Agent 循环流程

**MicroClaw 流程：**
```rust
for iteration in 0..max_iterations {
    let response = llm.send_message(system, messages, Some(tool_defs)).await?;

    match response.stop_reason {
        "end_turn" | "max_tokens" => {
            // 返回最终文本响应
            return Ok(text);
        }
        "tool_use" => {
            // 执行每个工具
            for tool_use in response.content {
                let result = tools.execute_with_auth(name, input, &auth).await;
                messages.push(tool_result);
            }
            // 继续循环
        }
    }
}
```

**Nanors 流程：**
```rust
// 当前只有单次请求，无循环
let response = provider.chat(&messages, &config.model).await?;
// 保存并返回
```

**影响：** 需要实现完整的工具调用循环。

## 4. 移植方案

### 4.1 目录结构

在 nanors 项目中创建新的 crate 和目录：

```
nanors/
├── nanors_tools/              # 新建工具 crate
│   ├── Cargo.toml
│   └── src/
│       ├── mod.rs              # Tool trait, ToolRegistry
│       ├── bash.rs
│       ├── read_file.rs
│       ├── write_file.rs
│       ├── edit_file.rs
│       ├── glob.rs
│       ├── grep.rs
│       ├── path_guard.rs
│       ├── command_runner.rs
│       └── ... (其他工具)
│
├── nanors_core/
│   └── src/
│       ├── lib.rs              # 扩展 LLMProvider, ChatMessage
│       ├── agent/
│       │   ├── mod.rs
│       │   └── agent_loop.rs   # 集成工具调用
│       └── tooling/            # 新建工具支持模块
│           ├── mod.rs
│           ├── context.rs       # ToolAuthContext
│           └── message.rs       # 扩展消息格式
│
└── Cargo.toml                 # 添加 nanors_tools
```

### 4.2 新增依赖

```toml
[workspace.dependencies]
# 新增依赖用于工具系统
glob = "0.3"
regex = "1"
uuid = { version = "1", features = ["v4", "v7", "serde", "fast-rng"] }

# nanors_tools 依赖
nanors_tools = { path = "nanors_tools" }
```

### 4.3 代码迁移映射

| MicroClaw 文件 | Nanors 目标位置 | 迁移方式 |
|----------------|-----------------|----------|
| src/tools/mod.rs | nanors_tools/src/lib.rs | 复制 + 改名 |
| src/tools/bash.rs | nanors_tools/src/bash.rs | 复制 |
| src/tools/read_file.rs | nanors_tools/src/read_file.rs | 复制 |
| src/tools/write_file.rs | nanors_tools/src/write_file.rs | 复制 |
| src/tools/edit_file.rs | nanors_tools/src/edit_file.rs | 复制 |
| src/tools/glob.rs | nanors_tools/src/glob.rs | 复制 |
| src/tools/grep.rs | nanors_tools/src/grep.rs | 复制 |
| src/tools/path_guard.rs | nanors_tools/src/path_guard.rs | 复制 |
| src/tools/command_runner.rs | nanors_tools/src/command_runner.rs | 复制 |
| src/claude.rs | nanors_core/src/tooling/message.rs | 适配 |
| src/agent_engine.rs | nanors_core/src/agent/agent_loop.rs | 集成 |
| src/config.rs (WorkingDirIsolation) | nanors_config/src/lib.rs | 合并 |

## 5. 核心代码适配

### 5.1 Tool Trait (保持不变)

```rust
// nanors_tools/src/lib.rs
use async_trait::async_trait;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: serde_json::Value) -> ToolResult;
}
```

### 5.2 ChatMessage 扩展

```rust
// nanors_core/src/lib.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}
```

### 5.3 LLMProvider 扩展

```rust
// nanors_core/src/lib.rs
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn chat(&self, messages: &[ChatMessage], model: &str) -> anyhow::Result<LLMResponse>;
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    fn get_default_model(&self) -> &str;

    // 新增：工具调用支持
    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        model: &str,
        tools: Option<Vec<ToolDefinition>>,
    ) -> anyhow::Result<LLMToolResponse>;
}

#[derive(Debug, Clone)]
pub struct LLMToolResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}
```

### 5.4 AgentLoop 集成

```rust
// nanors_core/src/agent/agent_loop.rs
pub struct AgentLoop<P = Arc<dyn LLMProvider>, S = Arc<dyn SessionStorage>>
where
    P: Send + Sync,
    S: Send + Sync,
{
    provider: P,
    session_manager: S,
    config: AgentConfig,
    running: Arc<AtomicBool>,
    memory_manager: Option<Arc<dyn MemoryItemRepo>>,
    retrieval_config: RetrievalConfig,
    // 新增：工具注册表
    tools: Option<nanors_tools::ToolRegistry>,
}

impl<P, S> AgentLoop<P, S>
where
    P: LLMProvider + Send + Sync,
    S: SessionStorage + Send + Sync,
{
    // 新增方法
    #[must_use]
    pub fn with_tools(mut self, tools: nanors_tools::ToolRegistry) -> Self {
        self.tools = Some(tools);
        self
    }

    // 修改 process_message 支持工具调用
    pub async fn process_message(
        &self,
        session_id: &Uuid,
        content: &str,
    ) -> anyhow::Result<String> {
        let system_prompt = self.build_system_prompt(content).await;

        let mut messages = vec![
            ChatMessage {
                role: Role::System,
                content: MessageContent::Text(system_prompt),
            },
            ChatMessage {
                role: Role::User,
                content: MessageContent::Text(content.to_string()),
            },
        ];

        // 工具调用循环
        let max_iterations = self.tools.as_ref()
            .map(|_| 100)
            .unwrap_or(1);

        for iteration in 0..max_iterations {
            let tool_defs = self.tools.as_ref()
                .map(|t| t.definitions());

            let response = self.provider.chat_with_tools(
                &messages,
                &self.config.model,
                tool_defs,
            ).await?;

            match response.stop_reason.as_deref() {
                Some("end_turn") | Some("max_tokens") | None => {
                    // 提取文本响应
                    let text: String = response.content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect();

                    messages.push(ChatMessage {
                        role: Role::Assistant,
                        content: MessageContent::Text(text.clone()),
                    });

                    self.save_to_session(session_id, &messages).await?;
                    return Ok(text);
                }
                Some("tool_use") => {
                    // 添加 assistant 消息（包含 tool_use blocks）
                    messages.push(ChatMessage {
                        role: Role::Assistant,
                        content: MessageContent::Blocks(response.content.clone()),
                    });

                    // 执行工具
                    let mut tool_results = Vec::new();
                    for block in &response.content {
                        if let ContentBlock::ToolUse { id, name, input } = block {
                            let result = self.tools.as_ref()
                                .unwrap()
                                .execute(name, input.clone())
                                .await;

                            tool_results.push(ContentBlock::ToolResult {
                                tool_use_id: id.clone(),
                                content: result.content,
                                is_error: if result.is_error { Some(true) } else { None },
                            });
                        }
                    }

                    // 添加 tool 结果消息
                    messages.push(ChatMessage {
                        role: Role::User,
                        content: MessageContent::Blocks(tool_results),
                    });
                }
                _ => break,
            }
        }

        Err(anyhow::anyhow!("Max iterations exceeded"))
    }
}
```

## 6. 工具清单

### 6.1 核心工具（必需）

| 工具 | 文件 | 描述 | 优先级 |
|------|------|------|--------|
| bash | bash.rs | 执行 shell 命令 | P0 |
| read_file | read_file.rs | 读取文件内容 | P0 |
| write_file | write_file.rs | 写入文件 | P0 |
| edit_file | edit_file.rs | 编辑文件（查找替换） | P1 |
| glob | glob.rs | 文件模式匹配 | P1 |
| grep | grep.rs | 正则搜索文件内容 | P1 |

### 6.2 辅助工具

| 工具 | 文件 | 描述 | 优先级 |
|------|------|------|--------|
| path_guard | path_guard.rs | 路径安全保护 | P0（内置） |
| command_runner | command_runner.rs | 命令构建辅助 | P0（内置） |

## 7. 安全考虑

### 7.1 Path Guard

microclaw 的 `path_guard.rs` 提供了路径安全保护，防止访问敏感文件：

**阻止的目录：** `.ssh`, `.aws`, `.gnupg`, `.kube`

**阻止的文件：** `.env`, `credentials`, `id_rsa`, `token.json` 等

**阻止的绝对路径：** `/etc/shadow`, `/etc/gshadow`, `/etc/sudoers`

### 7.2 Working Directory Isolation

支持两种隔离模式：

```rust
pub enum WorkingDirIsolation {
    Shared,   // 所有对话共享同一目录
    Chat,     // 每个对话独立目录
}
```

## 8. 数据库集成

### 8.1 会话存储扩展

需要扩展 `SessionStorage` trait 支持工具消息：

```rust
#[async_trait]
pub trait SessionStorage: Send + Sync {
    async fn get_or_create(&self, id: &Uuid) -> anyhow::Result<Session>;
    async fn add_message(&self, id: &Uuid, role: Role, content: &str) -> anyhow::Result<()>;

    // 新增：保存完整消息（支持 blocks）
    async fn save_messages(&self, id: &Uuid, messages: &[ChatMessage]) -> anyhow::Result<()>;
    async fn load_messages(&self, id: &Uuid) -> anyhow::Result<Vec<ChatMessage>>;
}
```

## 9. 实施步骤

### Phase 1: 基础设施 (P0)
1. 创建 `nanors_tools` crate
2. 移植核心数据结构 (`Tool`, `ToolResult`, `ToolDefinition`)
3. 移植 `path_guard.rs` 和 `command_runner.rs`

### Phase 2: 核心工具 (P0)
4. 移植 `bash.rs`
5. 移植 `read_file.rs`
6. 移植 `write_file.rs`
7. 移植 `glob.rs` 和 `grep.rs`

### Phase 3: 消息格式 (P0)
8. 扩展 `ChatMessage` 支持 `MessageContent::Blocks`
9. 扩展 `LLMProvider` 支持 `chat_with_tools`
10. 适配 `ZhipuProvider` 实现工具调用

### Phase 4: Agent 集成 (P0)
11. 修改 `AgentLoop` 添加工具调用循环
12. 实现 `ToolRegistry` 并集成到 `AgentLoop`

### Phase 5: 测试与验证 (P0)
13. 编写单元测试
14. 运行 `./debug.sh` 确保无警告

### Phase 6: 额外工具 (P1-P2)
15. 移植剩余工具（web, schedule, todo 等）

## 10. 兼容性

| 方面 | 状态 | 说明 |
|------|------|------|
| Rust 版本 | ✅ | nanors 使用 1.85，microclaw 使用 2021，均支持所需特性 |
| 异步运行时 | ✅ | 都使用 tokio |
| 序列化 | ✅ | 都使用 serde |
| 数据库 | ⚠️ | microclaw 用 SQLite，nanors 用 IvorySQL/PostgreSQL，需要适配存储层 |

## 11. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 智谱 API 不支持原生工具调用 | 高 | 需要实现模拟工具调用或使用支持工具调用的模型 |
| 消息格式不兼容 | 中 | 使用适配层转换格式 |
| 路径安全问题 | 中 | 确保 path_guard 正确移植并测试 |

## 12. 总结

移植工作量评估：

- **核心代码行数：** ~3000 行（包含所有工具）
- **新增依赖：** glob, regex
- **预估工时：** 2-3 天（核心功能），1 周完整移植

**推荐策略：** 分阶段实施，优先实现核心工具（bash, read_file, write_file），验证架构后再添加其他工具。
