//! HTTP cache — caches responses by URL with ETag and Last-Modified.
//!
//! Supports:
//! - Simple in-memory cache (keyed by URL)
//! - Conditional requests (If-None-Match, If-Modified-Since)
//! - Cache-Control: max-age (simplified — just stores for the process lifetime)
//! - Manual cache clearing

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A cached response.
#[derive(Debug, Clone)]
struct CachedResponse {
    body: Vec<u8>,
    etag: Option<String>,
    last_modified: Option<String>,
    cached_at: Instant,
    max_age: Option<Duration>,
}

static CACHE: Mutex<Option<HashMap<String, CachedResponse>>> = Mutex::new(None);

/// Get a cached response by URL. Returns None if not cached or expired.
pub fn get(url: &str) -> Option<Vec<u8>> {
    let cache = CACHE.lock().unwrap();
    let cache = cache.as_ref()?;
    let entry = cache.get(url)?;

    // Check max-age expiration.
    if let Some(max_age) = entry.max_age {
        if entry.cached_at.elapsed() > max_age {
            return None;
        }
    }

    Some(entry.body.clone())
}

/// Get the ETag for a cached URL (for conditional requests).
pub fn get_etag(url: &str) -> Option<String> {
    let cache = CACHE.lock().unwrap();
    cache.as_ref()?.get(url)?.etag.clone()
}

/// Get the Last-Modified for a cached URL.
pub fn get_last_modified(url: &str) -> Option<String> {
    let cache = CACHE.lock().unwrap();
    cache.as_ref()?.get(url)?.last_modified.clone()
}

/// Store a response in the cache.
pub fn put(url: &str, body: Vec<u8>) {
    let mut cache = CACHE.lock().unwrap();
    if cache.is_none() {
        *cache = Some(HashMap::new());
    }

    if let Some(ref mut cache) = *cache {
        cache.insert(
            url.to_string(),
            CachedResponse {
                body,
                etag: None,
                last_modified: None,
                cached_at: Instant::now(),
                max_age: None,
            },
        );
    }
}

/// Store a response with metadata (ETag, Last-Modified, Cache-Control).
pub fn put_with_metadata(
    url: &str,
    body: Vec<u8>,
    etag: Option<String>,
    last_modified: Option<String>,
    cache_control: Option<&str>,
) {
    let max_age = cache_control.and_then(|cc| {
        // Parse "max-age=3600" from Cache-Control header.
        if let Some(pos) = cc.find("max-age=") {
            let rest = &cc[pos + 8..];
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            rest[..end].parse::<u64>().ok().map(Duration::from_secs)
        } else {
            None
        }
    });

    let mut cache = CACHE.lock().unwrap();
    if cache.is_none() {
        *cache = Some(HashMap::new());
    }

    if let Some(ref mut cache) = *cache {
        cache.insert(
            url.to_string(),
            CachedResponse {
                body,
                etag,
                last_modified,
                cached_at: Instant::now(),
                max_age,
            },
        );
    }
}

/// Clear the entire cache.
pub fn clear() {
    *CACHE.lock().unwrap() = None;
}

/// Get cache statistics.
pub fn stats() -> (usize, usize) {
    let cache = CACHE.lock().unwrap();
    match cache.as_ref() {
        Some(c) => (c.len(), c.values().map(|e| e.body.len()).sum()),
        None => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_put_get() {
        clear();
        put("https://example.com", b"hello world".to_vec());
        let result = get("https://example.com");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"hello world");
        clear();
    }
}
