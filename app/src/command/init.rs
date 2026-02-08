use nanors_config::Config;

/// Strategy for initializing the configuration.
///
/// This strategy creates the default configuration file at `~/nanors/config.json`.
///
/// # Design
/// - Zero-allocation: No heap allocation
/// - Static dispatch: All method calls are monomorphized
/// - Stateless: No internal state, simplest form of strategy
#[derive(Debug, Clone, Copy)]
pub struct InitStrategy;

impl super::CommandStrategy for InitStrategy {
    type Input = ();

    async fn execute(&self, _input: Self::Input) -> anyhow::Result<()> {
        Config::create_config()
    }
}
