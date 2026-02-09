use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::io::Write;
use std::sync::{Arc, atomic::AtomicBool};
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    ChatMessage, LLMProvider, MemoryCategoryRepo, MemoryItem, MemoryItemRepo, MemoryType,
    ResourceRepo, Role, SessionStorage,
    retrieval::{AdaptiveConfig, CategoryCompressor, SufficiencyChecker, find_adaptive_cutoff},
};

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

/// Tiered retrieval configuration based on memU's approach
#[expect(
    clippy::struct_excessive_bools,
    reason = "These are independent feature flags, not a state machine"
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub categories_enabled: bool,
    pub categories_top_k: usize,
    pub items_top_k: usize,
    pub resources_enabled: bool,
    pub resources_top_k: usize,
    pub context_target_length: usize,
    /// Enable sufficiency check to stop retrieval early
    #[serde(default)]
    pub sufficiency_check_enabled: bool,
    /// Enable category auto-compression
    #[serde(default)]
    pub enable_category_compression: bool,
    /// Target length for category summaries in tokens
    #[serde(default = "RetrievalConfig::default_category_summary_target_length")]
    pub category_summary_target_length: usize,
    /// Adaptive retrieval configuration for memory items
    #[serde(default)]
    pub adaptive_items: AdaptiveConfig,
    /// Adaptive retrieval configuration for categories
    #[serde(default)]
    pub adaptive_categories: AdaptiveConfig,
    /// Adaptive retrieval configuration for resources
    #[serde(default)]
    pub adaptive_resources: AdaptiveConfig,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            categories_enabled: true,
            categories_top_k: 3,
            items_top_k: 5,
            resources_enabled: true,
            resources_top_k: 2,
            context_target_length: 2000,
            sufficiency_check_enabled: false,
            enable_category_compression: false,
            category_summary_target_length: Self::default_category_summary_target_length(),
            adaptive_items: AdaptiveConfig::default(),
            adaptive_categories: AdaptiveConfig::default(),
            adaptive_resources: AdaptiveConfig::default(),
        }
    }
}

impl RetrievalConfig {
    const fn default_category_summary_target_length() -> usize {
        400
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
    category_manager: Option<Arc<dyn MemoryCategoryRepo>>,
    resource_manager: Option<Arc<dyn ResourceRepo>>,
    user_scope: String,
    retrieval_config: RetrievalConfig,
    sufficiency_checker: Option<Arc<dyn SufficiencyChecker>>,
    category_compressor: Option<Arc<dyn CategoryCompressor>>,
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
            category_manager: None,
            resource_manager: None,
            user_scope: String::new(),
            retrieval_config: RetrievalConfig::default(),
            sufficiency_checker: None,
            category_compressor: None,
        }
    }

    /// Set the memory manager for persistent memory storage
    #[must_use]
    pub fn with_memory(
        mut self,
        memory_manager: Arc<dyn MemoryItemRepo>,
        user_scope: String,
    ) -> Self {
        self.memory_manager = Some(memory_manager);
        self.user_scope = user_scope;
        self
    }

    /// Set the category and resource managers for tiered retrieval
    #[must_use]
    pub fn with_tiered_retrieval(
        mut self,
        category_manager: Arc<dyn MemoryCategoryRepo>,
        resource_manager: Arc<dyn ResourceRepo>,
        retrieval_config: RetrievalConfig,
    ) -> Self {
        self.category_manager = Some(category_manager);
        self.resource_manager = Some(resource_manager);
        self.retrieval_config = retrieval_config;
        self
    }

    /// Set the sufficiency checker for early termination
    #[must_use]
    pub fn with_sufficiency_checker(mut self, checker: Arc<dyn SufficiencyChecker>) -> Self {
        self.sufficiency_checker = Some(checker);
        self
    }

    /// Set the category compressor for auto-compression
    #[must_use]
    pub fn with_category_compressor(mut self, compressor: Arc<dyn CategoryCompressor>) -> Self {
        self.category_compressor = Some(compressor);
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

        let system_prompt = self
            .build_system_prompt_with_tiered_retrieval(content)
            .await;

        let messages = vec![
            ChatMessage {
                role: Role::System,
                content: system_prompt,
            },
            ChatMessage {
                role: Role::User,
                content: content.to_string(),
            },
        ];

        // Log messages being sent to the LLM
        for (i, msg) in messages.iter().enumerate() {
            info!(
                "Message {}: role={:?}, content_len={}",
                i,
                msg.role,
                msg.content.len()
            );
        }

        let response = self.provider.chat(&messages, &self.config.model).await?;

        self.save_to_session(session_id, content, &response).await?;
        self.save_to_memory_with_embeddings(content, &response)
            .await;

        Ok(response.content)
    }

    /// Add context part if within target length, returns updated total length
    fn add_context_part(
        context_parts: &mut Vec<String>,
        text: String,
        total_length: usize,
        target_length: usize,
    ) -> usize {
        let text_len = text.len();
        if total_length + text_len > target_length {
            total_length
        } else {
            context_parts.push(text);
            total_length + text_len
        }
    }

    /// Retrieve and format tier 1 categories
    async fn retrieve_categories_tier(
        &self,
        query_embedding: &[f32],
        context_parts: &mut Vec<String>,
        total_length: &mut usize,
    ) {
        if !self.retrieval_config.categories_enabled {
            return;
        }
        let Some(category_manager) = &self.category_manager else {
            return;
        };

        let adaptive_config = &self.retrieval_config.adaptive_categories;

        // Use adaptive retrieval: fetch more results and dynamically determine cutoff
        let over_retrieve_k = if adaptive_config.enabled {
            adaptive_config.max_results
        } else {
            self.retrieval_config.categories_top_k
        };

        let Ok(all_categories) = category_manager
            .search_by_embedding(&self.user_scope, query_embedding, over_retrieve_k)
            .await
        else {
            return;
        };

        // Apply adaptive cutoff if enabled
        let categories = if adaptive_config.enabled && !all_categories.is_empty() {
            let scores: Vec<f64> = all_categories.iter().map(|s| s.score).collect();
            let cutoff = find_adaptive_cutoff(&scores, adaptive_config);
            info!(
                "Adaptive retrieval (categories): {} -> {} items",
                all_categories.len(),
                cutoff
            );
            all_categories.into_iter().take(cutoff).collect::<Vec<_>>()
        } else {
            all_categories
        };

        info!("Tier 1: Retrieved {} categories", categories.len());

        // Sort categories by recency
        let mut sorted_categories = categories;
        sorted_categories.sort_by_key(|b| std::cmp::Reverse(b.item.updated_at));

        for cat_score in sorted_categories {
            let time_ago = time_ago_since(cat_score.item.updated_at);
            let text = if let Some(summary) = &cat_score.item.summary {
                format!(
                    "Category: {} - {} [{}]",
                    cat_score.item.name, summary, time_ago
                )
            } else {
                format!("Category: {} [{}]", cat_score.item.name, time_ago)
            };
            *total_length = Self::add_context_part(
                context_parts,
                text,
                *total_length,
                self.retrieval_config.context_target_length,
            );
        }
    }

    /// Retrieve and format tier 2 memory items
    async fn retrieve_items_tier(
        &self,
        query_embedding: &[f32],
        context_parts: &mut Vec<String>,
        total_length: &mut usize,
    ) {
        let Some(memory_manager) = &self.memory_manager else {
            return;
        };

        let adaptive_config = &self.retrieval_config.adaptive_items;

        // Use adaptive retrieval: fetch more results and dynamically determine cutoff
        let over_retrieve_k = if adaptive_config.enabled {
            adaptive_config.max_results
        } else {
            self.retrieval_config.items_top_k
        };

        let Ok(all_items) = memory_manager
            .search_by_embedding(&self.user_scope, query_embedding, over_retrieve_k)
            .await
        else {
            return;
        };

        // Apply adaptive cutoff if enabled
        let items = if adaptive_config.enabled && !all_items.is_empty() {
            let scores: Vec<f64> = all_items.iter().map(|s| s.score).collect();
            let cutoff = find_adaptive_cutoff(&scores, adaptive_config);
            info!(
                "Adaptive retrieval (items): {} -> {} items",
                all_items.len(),
                cutoff
            );
            all_items.into_iter().take(cutoff).collect::<Vec<_>>()
        } else {
            all_items
        };

        info!("Tier 2: Retrieved {} items", items.len());

        // Sort by recency first (most recent first) to ensure newer memories appear earlier
        let mut items = items;
        items.sort_by_key(|b| std::cmp::Reverse(b.item.happened_at));

        for item_score in &items {
            // Format with timestamp for recency awareness
            let time_ago = time_ago_since(item_score.item.happened_at);
            let text = format!("- [{}] {}", time_ago, item_score.item.summary);
            *total_length = Self::add_context_part(
                context_parts,
                text,
                *total_length,
                self.retrieval_config.context_target_length,
            );
        }
    }

    /// Retrieve and format tier 3 resources
    async fn retrieve_resources_tier(
        &self,
        query_embedding: &[f32],
        context_parts: &mut Vec<String>,
        total_length: &mut usize,
    ) {
        if !self.retrieval_config.resources_enabled {
            return;
        }
        let Some(resource_manager) = &self.resource_manager else {
            return;
        };

        let adaptive_config = &self.retrieval_config.adaptive_resources;

        // Use adaptive retrieval: fetch more results and dynamically determine cutoff
        let over_retrieve_k = if adaptive_config.enabled {
            adaptive_config.max_results
        } else {
            self.retrieval_config.resources_top_k
        };

        let Ok(all_resources) = resource_manager
            .search_by_embedding(&self.user_scope, query_embedding, over_retrieve_k)
            .await
        else {
            return;
        };

        // Apply adaptive cutoff if enabled
        let resources = if adaptive_config.enabled && !all_resources.is_empty() {
            let scores: Vec<f64> = all_resources.iter().map(|s| s.score).collect();
            let cutoff = find_adaptive_cutoff(&scores, adaptive_config);
            info!(
                "Adaptive retrieval (resources): {} -> {} items",
                all_resources.len(),
                cutoff
            );
            all_resources.into_iter().take(cutoff).collect::<Vec<_>>()
        } else {
            all_resources
        };

        info!("Tier 3: Retrieved {} resources", resources.len());

        // Sort resources by recency
        let mut resources = resources;
        resources.sort_by_key(|b| std::cmp::Reverse(b.item.created_at));

        for res_score in &resources {
            let caption = res_score
                .item
                .caption
                .as_deref()
                .unwrap_or("Untitled resource");
            let time_ago = time_ago_since(res_score.item.created_at);
            let text = format!("[Resource: {caption} ({time_ago})]");
            *total_length = Self::add_context_part(
                context_parts,
                text,
                *total_length,
                self.retrieval_config.context_target_length,
            );
        }
    }

    /// Build the system prompt using tiered retrieval (memU-style)
    async fn build_system_prompt_with_tiered_retrieval(&self, query: &str) -> String {
        if self.memory_manager.is_none() {
            return "You are a helpful AI assistant.".to_string();
        }

        info!(
            "Using tiered retrieval with config: categories_top_k={}, items_top_k={}, context_target_length={}, sufficiency_check={}",
            self.retrieval_config.categories_top_k,
            self.retrieval_config.items_top_k,
            self.retrieval_config.context_target_length,
            self.retrieval_config.sufficiency_check_enabled
        );

        let query_embedding = match self.provider.embed(query).await {
            Ok(embedding) => embedding,
            Err(e) => {
                info!("Failed to generate query embedding: {e}, falling back to default");
                return "You are a helpful AI assistant.".to_string();
            }
        };

        let mut context_parts = Vec::new();
        let mut total_length = 0_usize;
        let mut active_query = query.to_string();

        // Tier 1: Categories
        self.retrieve_categories_tier(&query_embedding, &mut context_parts, &mut total_length)
            .await;

        // Sufficiency check after Tier 1
        if self.retrieval_config.sufficiency_check_enabled {
            if let Some(checker) = &self.sufficiency_checker {
                let retrieved = context_parts.join("\n");
                match checker.check(&active_query, &retrieved).await {
                    Ok(check) => {
                        active_query = check.rewritten_query;
                        if !check.needs_more {
                            info!("Sufficient content after categories, skipping items/resources");
                            return self.build_final_prompt(&context_parts);
                        }
                    }
                    Err(e) => {
                        debug!(
                            "Sufficiency check failed after categories: {e}, continuing retrieval"
                        );
                    }
                }
            }
        }

        // Tier 2: Items (only if needed)
        self.retrieve_items_tier(&query_embedding, &mut context_parts, &mut total_length)
            .await;

        // Sufficiency check after Tier 2
        if self.retrieval_config.sufficiency_check_enabled {
            if let Some(checker) = &self.sufficiency_checker {
                let retrieved = context_parts.join("\n");
                match checker.check(&active_query, &retrieved).await {
                    Ok(check) => {
                        if !check.needs_more {
                            info!("Sufficient content after items, skipping resources");
                            return self.build_final_prompt(&context_parts);
                        }
                    }
                    Err(e) => {
                        debug!("Sufficiency check failed after items: {e}, continuing retrieval");
                    }
                }
            }
        }

        // Tier 3: Resources (only if needed)
        self.retrieve_resources_tier(&query_embedding, &mut context_parts, &mut total_length)
            .await;

        self.build_final_prompt(&context_parts)
    }

    /// Build the final prompt from context parts
    fn build_final_prompt(&self, context_parts: &[String]) -> String {
        if context_parts.is_empty() {
            return "You are a helpful AI assistant.".to_string();
        }

        let memory_context = context_parts.join("\n");
        let total_length = context_parts
            .iter()
            .map(std::string::String::len)
            .sum::<usize>();

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
            "You are a helpful AI assistant with memory of past conversations.\n\nHere are relevant memories from previous conversations (in chronological order, most recent first):\n{memory_context}\n\nCRITICAL RULE: Each memory shows when it was recorded (e.g., \"[1天前]\"). When memories contain conflicting information, ALWAYS trust and use the MOST RECENT memory. People's situations change - they move houses, change jobs, update preferences. A memory from \"1小时前\" overrides one from \"7天前\".\n\nExamples of conflict resolution:\n- If memory says \"[1天前] User: 我搬到了石家庄\" and \"[7天前] User: 我住在丰台\", the CORRECT answer is 石家庄\n- If memory says \"[2小时前] User: 我换了工作\" and \"[1个月前] User: 我是工程师\", use the NEW information\n\nUse this context to provide better, more personalized responses."
        )
    }

    /// Save messages to session storage
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

    /// Save interaction to memory storage with embeddings
    async fn save_to_memory_with_embeddings(&self, content: &str, _response: &crate::LLMResponse) {
        if let Some(memory) = &self.memory_manager {
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

            let user_memory = MemoryItem {
                id: Uuid::now_v7(),
                user_scope: self.user_scope.clone(),
                resource_id: None,
                memory_type: MemoryType::Episodic,
                summary: format!("User: {content}"),
                embedding: user_embedding,
                happened_at: now,
                extra: None,
                content_hash: format!("{:x}", sha2::Sha256::digest(format!("episodic:{content}"))),
                reinforcement_count: 0,
                created_at: now,
                updated_at: now,
                version: 1,
                parent_version_id: None,
                version_relation: None,
            };

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

            // Trigger category compression if enabled
            if self.retrieval_config.enable_category_compression {
                self.compress_categories_due_to_new_memories(2);
            }
        }
    }

    /// Compress categories that have new memories
    fn compress_categories_due_to_new_memories(&self, item_count: usize) {
        // This is a placeholder for the category compression logic
        // The actual implementation would:
        // 1. Find categories linked to the new items
        // 2. For each category, fetch the current summary and new items
        // 3. Call the compressor to generate a new summary
        // 4. Update the category with the compressed summary

        if self.retrieval_config.enable_category_compression {
            debug!(
                "Category compression enabled, would compress categories for {} new items",
                item_count
            );
            // TODO: Implement actual compression logic once category-item linking is in place
        }
    }

    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
