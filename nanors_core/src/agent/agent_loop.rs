use sha2::Digest;
use std::io::Write;
use std::sync::{Arc, atomic::AtomicBool};
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    ChatMessage, LLMProvider, MemoryCategoryRepo, MemoryItem, MemoryItemRepo, MemoryType,
    ResourceRepo, Role, SessionStorage,
};

/// Tiered retrieval configuration based on memU's approach
#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    pub categories_enabled: bool,
    pub categories_top_k: usize,
    pub items_top_k: usize,
    pub resources_enabled: bool,
    pub resources_top_k: usize,
    pub context_target_length: usize,
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
    category_manager: Option<Arc<dyn MemoryCategoryRepo>>,
    resource_manager: Option<Arc<dyn ResourceRepo>>,
    user_scope: String,
    retrieval_config: RetrievalConfig,
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
            if msg.role == Role::System {
                debug!("System prompt: {}", msg.content);
            }
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

        let Ok(categories) = category_manager
            .search_by_embedding(
                &self.user_scope,
                query_embedding,
                self.retrieval_config.categories_top_k,
            )
            .await
        else {
            return;
        };

        info!("Tier 1: Retrieved {} categories", categories.len());
        for cat_score in categories {
            let text = if let Some(summary) = &cat_score.category.summary {
                format!("Category: {} - {}", cat_score.category.name, summary)
            } else {
                format!("Category: {}", cat_score.category.name)
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

        let Ok(items) = memory_manager
            .search_by_embedding(
                &self.user_scope,
                query_embedding,
                self.retrieval_config.items_top_k,
            )
            .await
        else {
            return;
        };

        info!("Tier 2: Retrieved {} items", items.len());
        for item_score in items {
            let text = format!("- {}", item_score.item.summary);
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

        let Ok(resources) = resource_manager
            .search_by_embedding(
                &self.user_scope,
                query_embedding,
                self.retrieval_config.resources_top_k,
            )
            .await
        else {
            return;
        };

        info!("Tier 3: Retrieved {} resources", resources.len());
        for res_score in resources {
            let caption = res_score
                .resource
                .caption
                .as_deref()
                .unwrap_or("Untitled resource");
            let text = format!("[Resource: {caption}]");
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
            "Using tiered retrieval with config: categories_top_k={}, items_top_k={}, context_target_length={}",
            self.retrieval_config.categories_top_k,
            self.retrieval_config.items_top_k,
            self.retrieval_config.context_target_length
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

        self.retrieve_categories_tier(&query_embedding, &mut context_parts, &mut total_length)
            .await;
        self.retrieve_items_tier(&query_embedding, &mut context_parts, &mut total_length)
            .await;
        self.retrieve_resources_tier(&query_embedding, &mut context_parts, &mut total_length)
            .await;

        if context_parts.is_empty() {
            return "You are a helpful AI assistant.".to_string();
        }

        let memory_context = context_parts.join("\n");
        info!(
            "Built context with {} chars (target: {})",
            total_length, self.retrieval_config.context_target_length
        );

        format!(
            "You are a helpful AI assistant with memory of past conversations.\n\nHere are relevant memories from previous conversations:\n{memory_context}\n\nUse this context to provide better, more personalized responses."
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
    async fn save_to_memory_with_embeddings(&self, content: &str, response: &crate::LLMResponse) {
        if let Some(memory) = &self.memory_manager {
            let now = chrono::Utc::now();

            // Generate embeddings for both user and assistant messages
            let (user_embedding, assistant_embedding) = match tokio::join!(
                self.provider.embed(content),
                self.provider.embed(&response.content)
            ) {
                (Ok(u), Ok(a)) => (Some(u), Some(a)),
                (Err(e), _) | (_, Err(e)) => {
                    debug!("Failed to generate embeddings: {e}");
                    (None, None)
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
            };

            if let Err(e) = memory.insert(&user_memory).await {
                debug!("Failed to store user memory: {e}");
            }

            let response_summary = format!("Assistant: {}", response.content);
            let assistant_memory = MemoryItem {
                id: Uuid::now_v7(),
                user_scope: self.user_scope.clone(),
                resource_id: None,
                memory_type: MemoryType::Episodic,
                summary: response_summary.clone(),
                embedding: assistant_embedding,
                happened_at: now,
                extra: None,
                content_hash: format!(
                    "{:x}",
                    sha2::Sha256::digest(format!("episodic:{response_summary}"))
                ),
                reinforcement_count: 0,
                created_at: now,
                updated_at: now,
            };

            if let Err(e) = memory.insert(&assistant_memory).await {
                debug!("Failed to store assistant memory: {e}");
            }

            debug!("Stored interaction as memories with embeddings");
        }
    }

    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
