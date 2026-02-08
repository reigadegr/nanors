use std::io::Write;
use std::sync::{Arc, atomic::AtomicBool};
use tracing::info;

use crate::{ChatMessage, LLMProvider, Role, SessionStorage};

pub struct AgentLoop<P = Arc<dyn LLMProvider>, S = Arc<dyn SessionStorage>>
where
    P: Send + Sync,
    S: Send + Sync,
{
    provider: P,
    session_manager: S,
    config: AgentConfig,
    running: Arc<AtomicBool>,
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
        }
    }

    pub async fn run_interactive(&self) -> anyhow::Result<()> {
        println!("nanobot agent started. Type 'exit' to quit.\n");

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

            let session_id = uuid::Uuid::now_v7();
            match self.process_message(&session_id, input).await {
                Ok(response) => println!("\n{response}\n"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }

        Ok(())
    }

    pub async fn process_message(
        &self,
        session_id: &uuid::Uuid,
        content: &str,
    ) -> anyhow::Result<String> {
        info!("Processing message from session: {}", session_id);

        let messages = vec![ChatMessage {
            role: Role::User,
            content: content.to_string(),
        }];

        let response = self.provider.chat(&messages, &self.config.model).await?;

        self.session_manager
            .add_message(session_id, Role::User, content)
            .await?;
        self.session_manager
            .add_message(session_id, Role::Assistant, &response.content)
            .await?;

        Ok(response.content)
    }

    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
