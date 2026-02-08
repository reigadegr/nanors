use nanors_config::Config;
use nanors_session::SessionManager;
use tracing::info;

/// Strategy for displaying configuration information.
///
/// This strategy outputs detailed configuration including:
/// - API keys (masked)
/// - Database URL and connection status
/// - Agent defaults (model, tokens, temperature)
/// - Memory configuration
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
        match SessionManager::new(db_url).await {
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
        println!();

        println!("Memory:");
        println!(
            "  Enabled: {}",
            if config.memory.enabled { "Yes" } else { "No" }
        );
        println!("  Default User Scope: {}", config.memory.default_user_scope);

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
