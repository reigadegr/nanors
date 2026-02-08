/// Strategy for displaying version information.
///
/// This strategy outputs the current version of the nanors application.
///
/// # Design
/// - Zero-allocation: No heap allocation
/// - Static dispatch: All method calls are monomorphized
/// - Stateless: No internal state
#[derive(Debug, Clone, Copy)]
pub struct VersionStrategy;

impl super::CommandStrategy for VersionStrategy {
    type Input = ();

    async fn execute(&self, _input: Self::Input) -> anyhow::Result<()> {
        println!("nanors {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    }
}
