use sha2::Digest;
use std::io::Write;
use std::sync::{Arc, atomic::AtomicBool};
use tracing::{debug, info};
use uuid::Uuid;

use crate::{ChatMessage, LLMProvider, MemoryItemRepo, MemoryType, Role, SessionStorage};

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
    user_scope: String,
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
            user_scope: String::new(),
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

        // Build system prompt with memory context
        let system_prompt = if let Some(memory) = &self.memory_manager {
            info!(
                "Memory manager available, user_scope: '{}'",
                self.user_scope
            );
            debug!("Searching for relevant memories");

            // Get recent memories and build context
            match memory.list_by_scope(&self.user_scope).await {
                Ok(memories) => {
                    eprintln!("[DEBUG] Found {} memories in database", memories.len());
                    if !memories.is_empty() {
                        // Take most recent memories (in reverse, newest first)
                        let user_memories: Vec<_> = memories
                            .iter()
                            .rev()
                            .take(10) // Limit to top 10 most recent memories
                            .filter(|m| {
                                let starts_with_user = m.summary.starts_with("User:");
                                if !starts_with_user {
                                    debug!(
                                        "Skipping non-user memory: {}",
                                        m.summary.chars().take(20).collect::<String>()
                                    );
                                }
                                starts_with_user
                            })
                            .collect();

                        info!("Filtered to {} user memories", user_memories.len());

                        if !user_memories.is_empty() {
                            let memory_context: String = user_memories
                                .iter()
                                .map(|m| format!("- {}", m.summary))
                                .collect::<Vec<_>>()
                                .join("\n");

                            format!(
                                "You are a helpful AI assistant with memory of past conversations.\n\nHere are relevant memories from previous conversations:\n{memory_context}\n\nUse this context to provide better, more personalized responses."
                            )
                        } else {
                            "You are a helpful AI assistant.".to_string()
                        }
                    } else {
                        "You are a helpful AI assistant.".to_string()
                    }
                }
                Err(e) => {
                    info!("Failed to retrieve memories: {}", e);
                    "You are a helpful AI assistant.".to_string()
                }
            }
        } else {
            info!("No memory manager available");
            "You are a helpful AI assistant.".to_string()
        };

        let mut messages = Vec::new();
        messages.push(ChatMessage {
            role: Role::System,
            content: system_prompt,
        });

        // Add user message
        messages.push(ChatMessage {
            role: Role::User,
            content: content.to_string(),
        });

        // Debug: log the messages being sent
        for (i, msg) in messages.iter().enumerate() {
            info!(
                "Message {}: role={:?}, content_len={}",
                i,
                msg.role,
                msg.content.len()
            );
            if msg.role == Role::System {
                info!("System prompt: {}", msg.content);
            }
        }

        let response = self.provider.chat(&messages, &self.config.model).await?;

        self.session_manager
            .add_message(session_id, Role::User, content)
            .await?;
        self.session_manager
            .add_message(session_id, Role::Assistant, &response.content)
            .await?;

        // Store the interaction as a memory if memory manager is available
        if let Some(memory) = &self.memory_manager {
            let now = chrono::Utc::now();

            // Store user message as episodic memory
            let user_memory = crate::MemoryItem {
                id: Uuid::now_v7(),
                user_scope: self.user_scope.clone(),
                resource_id: None,
                memory_type: MemoryType::Episodic,
                summary: format!("User: {content}"),
                embedding: None, // Would be populated by embedding service
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

            // Store assistant response as episodic memory
            let response_summary = format!("Assistant: {}", response.content);
            let assistant_memory = crate::MemoryItem {
                id: Uuid::now_v7(),
                user_scope: self.user_scope.clone(),
                resource_id: None,
                memory_type: MemoryType::Episodic,
                summary: response_summary.clone(),
                embedding: None,
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

            debug!("Stored interaction as memories");
        }

        Ok(response.content)
    }

    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
