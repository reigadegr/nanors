//! Agent loop for processing messages with memory retrieval.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::{Arc, atomic::AtomicBool};
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    ChatMessage, ContentBlock, DEFAULT_SYSTEM_PROMPT, LLMProvider, MemoryItem, MemoryItemRepo,
    MessageContent, Role, SessionStorage,
};

use crate::retrieval::adaptive::{AdaptiveConfig, find_adaptive_cutoff};

/// Format a timestamp as a human-readable "time ago" string
fn time_ago_since(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(timestamp);

    if duration.num_days() > 0 {
        format!("{}天前", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}小时前", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}分钟前", duration.num_minutes())
    } else {
        "刚刚".to_string()
    }
}

/// Memory retrieval configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    /// Maximum number of items to retrieve
    pub items_top_k: usize,
    /// Maximum context length in characters
    pub context_target_length: usize,
    /// Enable adaptive retrieval to dynamically determine result count
    #[serde(default)]
    pub adaptive: AdaptiveConfig,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            items_top_k: 5,
            context_target_length: 2000,
            adaptive: AdaptiveConfig::default(),
        }
    }
}

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
    tools: Option<nanors_tools::StaticToolRegistry>,
    max_tool_iterations: usize,
    /// Maximum number of messages to keep in context history
    history_limit: usize,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub model: String,
    pub max_tokens: usize,
    pub temperature: f32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "glm-4-flash".to_string(),
            max_tokens: 8192,
            temperature: 0.7,
        }
    }
}

impl<P, S> AgentLoop<P, S>
where
    P: LLMProvider + Send + Sync,
    S: SessionStorage + Send + Sync,
{
    pub fn new(provider: P, session_manager: S, config: AgentConfig) -> Self {
        Self {
            provider,
            session_manager,
            config,
            running: Arc::new(AtomicBool::new(true)),
            memory_manager: None,
            retrieval_config: RetrievalConfig::default(),
            tools: None,
            max_tool_iterations: 10,
            history_limit: 20,
        }
    }

    /// Set the tools registry for tool calling.
    #[must_use]
    pub fn with_tools(mut self, tools: nanors_tools::StaticToolRegistry) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the maximum number of tool iterations.
    #[must_use]
    pub const fn with_max_tool_iterations(mut self, max: usize) -> Self {
        self.max_tool_iterations = max;
        self
    }

    /// Set the memory manager for persistent memory storage.
    #[must_use]
    pub fn with_memory(mut self, memory_manager: Arc<dyn MemoryItemRepo>) -> Self {
        self.memory_manager = Some(memory_manager);
        self
    }

    /// Set the retrieval configuration.
    #[must_use]
    pub const fn with_retrieval_config(mut self, retrieval_config: RetrievalConfig) -> Self {
        self.retrieval_config = retrieval_config;
        self
    }

    /// Set history limit for context.
    #[must_use]
    pub const fn with_history_limit(mut self, limit: usize) -> Self {
        self.history_limit = limit;
        self
    }

    pub async fn run_interactive(&self) -> anyhow::Result<()> {
        println!("nanors agent started. Type 'exit' to quit.\n");

        while self.running.load(std::sync::atomic::Ordering::Relaxed) {
            print!("> ");
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input == "exit" {
                break;
            }

            if input.is_empty() {
                continue;
            }

            let session_id = Uuid::now_v7();
            match self.process_message(&session_id, input).await {
                Ok(response) => println!("\n{response}\n"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }

        Ok(())
    }

    pub async fn process_message(
        &self,
        session_id: &Uuid,
        content: &str,
    ) -> anyhow::Result<String> {
        info!("Processing message from session: {}", session_id);

        // Check if tools are available
        if self.tools.is_some() {
            return self.process_message_with_tools(session_id, content).await;
        }

        // Load session history
        let session = self.session_manager.get_or_create(session_id).await?;
        let history = &session.messages;

        // Get last N messages for context
        let history_start = history.len().saturating_sub(self.history_limit);
        let history_messages: Vec<ChatMessage> = history[history_start..].to_vec();

        let system_prompt = self.build_system_prompt(content).await;

        // Build messages: system prompt + history + current message
        let mut messages = vec![ChatMessage {
            role: Role::System,
            content: MessageContent::Text(system_prompt),
        }];
        messages.extend(history_messages);
        messages.push(ChatMessage {
            role: Role::User,
            content: MessageContent::Text(content.to_string()),
        });

        // Log messages being sent to the LLM
        for (i, msg) in messages.iter().enumerate() {
            let content_len = match &msg.content {
                MessageContent::Text(text) => text.len(),
                MessageContent::Blocks(blocks) => blocks.len(),
            };
            info!(
                "Message {}: role={:?}, content_len={}",
                i, msg.role, content_len
            );
        }

        let response = self.provider.chat(&messages, &self.config.model).await?;

        self.save_to_session(session_id, content, &response).await?;
        self.save_to_memory_with_embeddings(content, &response.content)
            .await;

        Ok(response.content)
    }

    /// Process message with tool calling support.
    async fn process_message_with_tools(
        &self,
        session_id: &Uuid,
        content: &str,
    ) -> anyhow::Result<String> {
        // Load session history
        let session = self.session_manager.get_or_create(session_id).await?;
        let history = &session.messages;

        // Get last N messages for context
        let history_start = history.len().saturating_sub(self.history_limit);
        let history_messages: Vec<ChatMessage> = history[history_start..].to_vec();

        let system_prompt = self.build_system_prompt(content).await;
        let Some(tools) = self.tools.as_ref() else {
            anyhow::bail!("Tool calling requested but no tools available")
        };
        let tool_definitions = tools.definitions();

        // Build conversation: system prompt + history + current message
        let mut messages = vec![ChatMessage {
            role: Role::System,
            content: MessageContent::Text(system_prompt),
        }];
        messages.extend(history_messages);
        messages.push(ChatMessage {
            role: Role::User,
            content: MessageContent::Text(content.to_string()),
        });

        // Tool calling loop
        for iteration in 0..self.max_tool_iterations {
            info!("Tool iteration {}", iteration + 1);

            let tool_defs = if tool_definitions.is_empty() {
                None
            } else {
                Some(tool_definitions.clone())
            };

            let response = self
                .provider
                .chat_with_tools(&messages, &self.config.model, tool_defs)
                .await?;

            // Check stop reason
            match response.stop_reason.as_deref() {
                Some("end_turn" | "stop") | None => {
                    // Extract text content from response
                    let text_content: Vec<String> = response
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect();

                    let final_text = text_content.join("\n");

                    // Save to session and memory
                    self.save_to_session_with_blocks(session_id, content, &response.content)
                        .await?;
                    self.save_to_memory_with_embeddings(content, &final_text)
                        .await;

                    return Ok(final_text);
                }
                Some("tool_use" | "tool_calls") => {
                    // Process tool calls
                    let mut tool_results = Vec::new();

                    for block in &response.content {
                        if let ContentBlock::ToolUse { id, name, input } = block {
                            info!("Tool call: {} with id {}", name, id);

                            let result = tools.execute(name, input.clone()).await;

                            let tool_result_block = ContentBlock::ToolResult {
                                tool_use_id: id.clone(),
                                content: result.content.clone(),
                                is_error: Some(result.is_error),
                            };
                            tool_results.push(tool_result_block);
                        }
                    }

                    // Add assistant message with tool calls
                    messages.push(ChatMessage {
                        role: Role::Assistant,
                        content: MessageContent::Blocks(response.content),
                    });

                    // Add tool results - Zhipu API requires Role::Tool with tool_call_id
                    // Each tool result should be a separate message
                    if !tool_results.is_empty() {
                        for result_block in tool_results {
                            messages.push(ChatMessage {
                                role: Role::Tool,
                                content: MessageContent::Blocks(vec![result_block]),
                            });
                        }
                    }
                }
                Some(other) => {
                    return Err(anyhow::anyhow!("Unexpected stop reason: {other}"));
                }
            }
        }

        Err(anyhow::anyhow!(
            "Max tool iterations ({}) reached",
            self.max_tool_iterations
        ))
    }

    /// Build the system prompt with memory retrieval.
    pub async fn build_system_prompt(&self, query: &str) -> String {
        let Some(memory_manager) = &self.memory_manager else {
            return DEFAULT_SYSTEM_PROMPT.to_string();
        };

        let query_embedding = match self.provider.embed(query).await {
            Ok(embedding) => embedding,
            Err(e) => {
                info!("Failed to generate query embedding: {e}, falling back to default");
                return DEFAULT_SYSTEM_PROMPT.to_string();
            }
        };

        // Fetch more items for adaptive retrieval
        let fetch_count = self.retrieval_config.adaptive.max_results;

        // Try to use enhanced search if available, fall back to standard search
        let Ok(mut items) = memory_manager
            .search_enhanced(&query_embedding, query, fetch_count)
            .await
        else {
            return DEFAULT_SYSTEM_PROMPT.to_string();
        };

        if items.is_empty() {
            return DEFAULT_SYSTEM_PROMPT.to_string();
        }

        // Apply adaptive retrieval cutoff
        let scores: Vec<f64> = items.iter().map(|s| s.score).collect();
        let cutoff = find_adaptive_cutoff(&scores, &self.retrieval_config.adaptive);
        let effective_count = cutoff.min(self.retrieval_config.items_top_k);

        info!(
            "Adaptive retrieval: using {} of {} items",
            effective_count,
            items.len()
        );

        // Truncate to effective count
        items.truncate(effective_count);

        // Debug: Log similarity scores for top items
        info!("=== Top {} memories by similarity ===", items.len().min(20));
        for (i, item_score) in items.iter().take(20).enumerate() {
            info!(
                "  [{}] sim={:.4} score={:.4} - [{}] {}",
                i + 1,
                item_score.similarity,
                item_score.score,
                time_ago_since(item_score.item.happened_at),
                item_score.item.summary
            );
        }
        info!("=== End similarity ranking ===");

        // Note: items are already sorted by similarity first (in search_by_embedding)
        // No need to re-sort by recency here - similarity is the primary ranking factor

        let mut context_parts = Vec::new();
        let mut total_length = 0_usize;

        for item_score in &items {
            let time_ago = time_ago_since(item_score.item.happened_at);
            let text = format!("- [{}] {}", time_ago, item_score.item.summary);
            let text_len = text.len();
            if total_length + text_len > self.retrieval_config.context_target_length {
                break;
            }
            context_parts.push(text);
            total_length += text_len;
        }

        let memory_context = context_parts.join("\n");

        info!(
            "Built context with {} chars (target: {})",
            total_length, self.retrieval_config.context_target_length
        );

        // Log the actual memory context being sent to LLM (for debugging)
        info!("=== Memory Context ({} items) ===", context_parts.len());
        for (i, part) in context_parts.iter().enumerate() {
            info!("  [{}] {}", i + 1, part);
        }
        info!("=== End Memory Context ===");

        format!(
            "You are a helpful AI assistant with memory of past conversations.\n\n# Relevant Memories\n\nMemories below are sorted by RELEVANCE (similarity), NOT by time. Each memory shows when it was recorded.\n\n{memory_context}\n\n# CRITICAL: Resolve Conflicts by RECENCY\n\n**When memories conflict, ALWAYS pick the one with the SMALLEST time value.**\n\nTime comparison (smaller = more recent):\n- 1小时前 < 1天前 (1 hour ago is MORE recent than 1 day ago)\n- 14小时前 < 1天前 (14 hours ago is MORE recent than 1 day ago)\n- 2天前 < 1周前 (2 days ago is MORE recent than 1 week ago)\n\n**DO NOT** pick based on position in the list. **ALWAYS** compare the timestamps.\n\nExample: If you see \"[14小时前] 我住丰台\" and \"[1天前] 我搬家到了东城\", answer \"丰台\" because 14小时前 < 1天前.\n\nMake a decisive answer. Do NOT ask for confirmation."
        )
    }

    /// Save messages to session storage.
    async fn save_to_session(
        &self,
        session_id: &Uuid,
        content: &str,
        response: &crate::LLMResponse,
    ) -> anyhow::Result<()> {
        self.session_manager
            .add_message(session_id, Role::User, content)
            .await?;
        self.session_manager
            .add_message(session_id, Role::Assistant, &response.content)
            .await?;
        Ok(())
    }

    /// Save messages with blocks to session storage.
    async fn save_to_session_with_blocks(
        &self,
        session_id: &Uuid,
        content: &str,
        response_blocks: &[ContentBlock],
    ) -> anyhow::Result<()> {
        self.session_manager
            .add_message(session_id, Role::User, content)
            .await?;

        // Extract text from blocks for storage
        let response_text: Vec<String> = response_blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect();

        self.session_manager
            .add_message(session_id, Role::Assistant, &response_text.join("\n"))
            .await?;
        Ok(())
    }

    /// Save interaction to memory storage with embeddings.
    async fn save_to_memory_with_embeddings(&self, content: &str, _response_text: &str) {
        let Some(memory) = &self.memory_manager else {
            return;
        };

        let now = chrono::Utc::now();

        // Only store user messages as memories - assistant responses are just outputs,
        // not facts. Storing assistant responses can cause confusion when they contain
        // incorrect information that gets retrieved later.
        let user_embedding = match self.provider.embed(content).await {
            Ok(embedding) => Some(embedding),
            Err(e) => {
                debug!("Failed to generate user embedding: {e}");
                None
            }
        };

        let user_memory = MemoryItem::create_episodic(content, user_embedding, now);

        // Use semantic upsert to handle fact updates (e.g., location changes)
        match memory.semantic_upsert(&user_memory, 0.85).await {
            Ok(id) => {
                debug!("Stored user memory: {}", id);
            }
            Err(e) => {
                debug!("Failed to store user memory: {e}");
            }
        }

        debug!("Stored user message as memory");
    }
}
