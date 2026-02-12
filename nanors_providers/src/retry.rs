use std::fmt::Display;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

/// Retry a async operation with exponential backoff.
///
/// # Arguments
/// * `operation` - The async operation to retry
/// * `base_delays` - Initial delays in seconds for exponential backoff
/// * `final_retries` - Number of additional retries at max delay
///
/// # Returns
/// The result of the operation if successful, or the last error if all retries fail
pub async fn retry_with_backoff<F, Fut, T, E>(
    mut operation: F,
    base_delays: &[u64],
    final_retries: usize,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: Display,
{
    let mut last_error = None;

    // Try initial attempt + exponential backoff retries
    for (i, delay_secs) in base_delays.iter().enumerate() {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let attempt = i + 1;
                if attempt < base_delays.len() + final_retries {
                    warn!(
                        "Request failed (attempt {}/{}): {e}. Retrying after {}s...",
                        attempt,
                        base_delays.len() + final_retries,
                        delay_secs
                    );
                    sleep(Duration::from_secs(*delay_secs)).await;
                }
                last_error = Some(e);
            }
        }
    }

    // Final retries at 10 second intervals
    for i in 0..final_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let attempt = base_delays.len() + i + 1;
                if i < final_retries - 1 {
                    warn!(
                        "Request failed (attempt {}/{}): {e}. Retrying after 10s...",
                        attempt,
                        base_delays.len() + final_retries
                    );
                    sleep(Duration::from_secs(10)).await;
                }
                last_error = Some(e);
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| panic!("All retry attempts exhausted but no error was stored")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = retry_with_backoff(
            || {
                let attempts = attempts.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), String>(())
                }
            },
            &[1, 2],
            2,
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_succeeds_after_failures() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result: std::result::Result<(), String> = retry_with_backoff(
            || {
                let attempts = attempts.clone();
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    if count < 3 {
                        Err(String::from("fail"))
                    } else {
                        Ok(())
                    }
                }
            },
            &[1, 2],
            2,
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_fails_after_all_attempts() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result: std::result::Result<(), String> = retry_with_backoff(
            || {
                let attempts = attempts.clone();
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err(String::from("fail"))
                }
            },
            &[1, 2],
            2,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 4); // 2 base + 2 final
    }
}
