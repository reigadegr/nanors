use nanors_config::Config;
use nanors_core::retrieval::adaptive::CutoffStrategy;
use nanors_memory::MemoryManager;
use nanors_memory::rerank::RuleBasedReranker;
use tracing::info;

/// Strategy for displaying configuration information.
///
/// This strategy outputs detailed configuration including:
/// - API keys (masked)
/// - Database URL and connection status
/// - Agent defaults (model, tokens, temperature, system prompt, history limit)
/// - Memory retrieval configuration
/// - Telegram configuration
///
/// # Design
/// - Zero-allocation: No heap allocation beyond what business logic requires
/// - Static dispatch: All method calls are monomorphized
/// - Stateless: No internal state
#[derive(Debug, Clone, Copy)]
pub struct InfoStrategy;

impl super::CommandStrategy for InfoStrategy {
    type Input = ();

    async fn execute(&self, _input: Self::Input) -> anyhow::Result<()> {
        let config = Config::load()?;

        println!("=== nanors Configuration ===\n");

        println!("API Key:");
        let api_key = &config.providers.zhipu.api_key;
        if api_key.len() > 8 {
            let masked = format!("{}...{}", &api_key[..4], &api_key[api_key.len() - 4..]);
            println!("  Zhipu: {masked}");
        } else {
            println!("  Zhipu: ***");
        }
        println!();

        println!("Database:");
        let db_url = &config.database.url;
        println!("  URL: {}", mask_database_url(db_url));

        info!("Testing database connection");
        match MemoryManager::<RuleBasedReranker>::new(db_url).await {
            Ok(_) => {
                println!("  Status: Connected");
            }
            Err(e) => {
                println!("  Status: Connection failed");
                println!("  Error: {e}");
            }
        }
        println!();

        println!("Agent Defaults:");
        println!("  Model: {}", config.agents.defaults.model);
        println!("  Max Tokens: {}", config.agents.defaults.max_tokens);
        println!("  Temperature: {}", config.agents.defaults.temperature);
        if let Some(ref prompt) = config.agents.defaults.system_prompt {
            println!("  System Prompt: {}", truncate(prompt, 60));
        }
        if let Some(limit) = config.agents.defaults.history_limit {
            println!("  History Limit: {limit}");
        }
        println!();

        println!("Memory Retrieval:");
        println!("  Items Top K: {}", config.memory.retrieval.items_top_k);
        println!(
            "  Context Target Length: {}",
            config.memory.retrieval.context_target_length
        );
        println!("  Adaptive:");
        println!(
            "    Min Results: {}",
            config.memory.retrieval.adaptive.min_results
        );
        println!(
            "    Max Results: {}",
            config.memory.retrieval.adaptive.max_results
        );
        println!(
            "    Strategy: {}",
            format_strategy(&config.memory.retrieval.adaptive.strategy)
        );
        println!(
            "    Normalize Scores: {}",
            config.memory.retrieval.adaptive.normalize_scores
        );
        println!();

        println!("Telegram:");
        let token = if config.telegram.token.is_empty() {
            "(not set)".to_string()
        } else if config.telegram.token.len() > 8 {
            format!("{}...***", &config.telegram.token[..8])
        } else {
            "***".to_string()
        };
        println!("  Token: {token}");
        if config.telegram.allow_from.is_empty() {
            println!("  Allow From: (empty - all users allowed)");
        } else {
            println!("  Allow From: {}", config.telegram.allow_from.join(", "));
        }

        Ok(())
    }
}

fn mask_database_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };

    let Some((credentials, after_at)) = rest.split_once('@') else {
        return url.to_string();
    };

    let Some((username, _password)) = credentials.split_once(':') else {
        return url.to_string();
    };

    format!("{scheme}://{username}:***{after_at}")
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn format_strategy(strategy: &CutoffStrategy) -> String {
    match strategy {
        CutoffStrategy::AbsoluteThreshold { min_score } => {
            format!("AbsoluteThreshold(min_score={min_score})")
        }
        CutoffStrategy::RelativeThreshold { min_ratio } => {
            format!("RelativeThreshold(min_ratio={min_ratio})")
        }
        CutoffStrategy::ScoreCliff { max_drop_ratio } => {
            format!("ScoreCliff(max_drop_ratio={max_drop_ratio})")
        }
        CutoffStrategy::Elbow { sensitivity } => {
            format!("Elbow(sensitivity={sensitivity})")
        }
        CutoffStrategy::Combined {
            relative_threshold,
            max_drop_ratio,
            absolute_min,
        } => format!(
            "Combined(relative_threshold={relative_threshold}, max_drop_ratio={max_drop_ratio}, absolute_min={absolute_min})"
        ),
    }
}
