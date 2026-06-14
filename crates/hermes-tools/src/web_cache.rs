//! Shared TTL cache for `web_fetch` / `web_search` results.
//!
//! A process-global, time-bounded cache keyed by an opaque string. Used to
//! avoid redundant network round-trips (and redundant tokens) when the agent
//! fetches the same URL or runs the same search twice within a session — a
//! common pattern in multi-round web research.
//!
//! Design notes:
//! - Zero new dependencies: a `Mutex<HashMap>` behind `once_cell::Lazy`,
//!   mirroring the shared HTTP client in [`crate::http_defaults`].
//! - Entries carry an insertion `Instant`; reads past `ttl` are treated as
//!   misses and lazily evicted.
//! - A hard entry cap bounds memory; when full, the oldest entry is evicted.
//! - `web_fetch` caches the *cleaned markdown* (not the LLM-extracted answer)
//!   so that different `prompt`s against the same page reuse one network fetch.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;

/// Maximum number of cached entries before the oldest is evicted.
const MAX_ENTRIES: usize = 256;

static CACHE: Lazy<Mutex<HashMap<String, (Instant, String)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Look up `key`; returns the cached value if present and younger than `ttl`.
/// Expired entries are evicted on access.
pub fn get(key: &str, ttl: Duration) -> Option<String> {
    let mut map = CACHE.lock().ok()?;
    match map.get(key) {
        Some((inserted, value)) if inserted.elapsed() < ttl => Some(value.clone()),
        Some(_) => {
            // Expired — drop it so the map doesn't accumulate stale entries.
            map.remove(key);
            None
        }
        None => None,
    }
}

/// Insert (or overwrite) `key` → `value`. Evicts the oldest entry when the
/// cache is at capacity.
pub fn put(key: String, value: String) {
    let Ok(mut map) = CACHE.lock() else { return };
    if map.len() >= MAX_ENTRIES && !map.contains_key(&key) {
        if let Some(oldest) = map
            .iter()
            .min_by_key(|(_, (inserted, _))| *inserted)
            .map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        }
    }
    map.insert(key, (Instant::now(), value));
}

/// Cache key for a `web_fetch` of `url`. Keyed by URL only — the cache stores
/// the full cleaned markdown, so different `max_chars` / `prompt` values reuse
/// a single network fetch.
pub fn fetch_key(url: &str) -> String {
    format!("fetch:{url}")
}

/// Cache key for a `web_search` of `query` limited to `limit` results.
pub fn search_key(query: &str, limit: usize) -> String {
    format!("search:{limit}:{query}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_within_ttl_miss_after() {
        let key = "test:hit_within_ttl".to_string();
        put(key.clone(), "value".to_string());
        assert_eq!(get(&key, Duration::from_secs(60)), Some("value".to_string()));
        // Zero TTL → immediately expired.
        assert_eq!(get(&key, Duration::from_millis(0)), None);
        // Expired access evicted it.
        assert_eq!(get(&key, Duration::from_secs(60)), None);
    }

    #[test]
    fn overwrite_refreshes_value() {
        let key = "test:overwrite".to_string();
        put(key.clone(), "old".to_string());
        put(key.clone(), "new".to_string());
        assert_eq!(get(&key, Duration::from_secs(60)), Some("new".to_string()));
    }

    #[test]
    fn key_helpers_are_distinct() {
        assert_ne!(fetch_key("https://a"), fetch_key("https://b"));
        assert_ne!(search_key("q", 5), fetch_key("q"));
    }
}
