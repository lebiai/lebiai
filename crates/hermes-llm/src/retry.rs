//! Shared transient-failure retry policy for LLM HTTP providers.
//!
//! Both Anthropic and OpenAI-compatible endpoints can fail with 429 / 5xx /
//! network errors; the policy is identical so the retry behavior is one
//! implementation, not two copies.

use std::time::Duration;

/// Maximum HTTP retry attempts for transient failures (5xx / 429 / network).
/// Total attempts = 1 + RETRY_ATTEMPTS.
pub const RETRY_ATTEMPTS: usize = 3;

/// Returns true for transient HTTP statuses that warrant a retry.
pub fn is_retriable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Backoff for attempt `n` (0-based): 500ms, 1500ms, 4500ms — plus small jitter.
pub fn backoff_delay(attempt: usize) -> Duration {
    let base_ms = 500_u64 * 3u64.pow(attempt as u32);
    // jitter: 0..=base_ms/4, derived deterministically from current nanos.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let jitter = nanos % (base_ms / 4).max(1);
    Duration::from_millis(base_ms + jitter)
}

/// Parse `Retry-After` header if present (whole seconds).
pub fn parse_retry_after(resp: &reqwest::Response) -> Option<Duration> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retriable_statuses() {
        use reqwest::StatusCode;
        assert!(is_retriable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retriable_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retriable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retriable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retriable_status(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn backoff_grows_with_attempt() {
        let a0 = backoff_delay(0);
        let a1 = backoff_delay(1);
        let a2 = backoff_delay(2);
        assert!(a1 > a0);
        assert!(a2 > a1);
    }
}
