//! HSTS (HTTP Strict-Transport-Security) — force HTTPS connections.
//!
//! # How HSTS Works
//!
//! When a browser receives an HTTPS response with the header:
//! ```text
//! Strict-Transport-Security: max-age=31536000; includeSubDomains; preload
//! ```
//!
//! It remembers that this host should only be accessed via HTTPS for the
//! next `max-age` seconds. Future HTTP requests to this host are
//! automatically upgraded to HTTPS.
//!
//! # Implementation
//!
//! - `HstsStore` — in-memory store of known HSTS hosts
//! - `should_upgrade(host)` — returns true if the host should be upgraded
//! - `process_response(host, headers)` — records HSTS policy from response headers
//!
//! The store is in-memory only (not persisted to disk). A future version
//! could write to a JSON file for persistence across sessions.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// An HSTS policy for a host.
#[derive(Debug, Clone)]
struct HstsPolicy {
    /// The host this policy applies to.
    host: String,
    /// When the policy expires (Unix timestamp in seconds).
    expires: u64,
    /// Whether to include subdomains.
    include_subdomains: bool,
    /// Whether this host is on the HSTS preload list.
    preload: bool,
}

impl HstsPolicy {
    /// Check if the policy has expired.
    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now >= self.expires
    }
}

/// Global HSTS store.
static HSTS_STORE: Mutex<Option<HashMap<String, HstsPolicy>>> = Mutex::new(None);

/// Initialize the store if needed.
fn ensure_store() {
    let mut store = HSTS_STORE.lock().unwrap();
    if store.is_none() {
        *store = Some(HashMap::new());
    }
}

/// Check if a host (or its parent) has an HSTS policy that requires HTTPS.
pub fn should_upgrade(host: &str) -> bool {
    ensure_store();
    let store = HSTS_STORE.lock().unwrap();
    let store = store.as_ref().unwrap();

    let host = host.to_lowercase();

    // Check the exact host.
    if let Some(policy) = store.get(&host) {
        if !policy.is_expired() {
            return true;
        }
    }

    // Check parent domains (for includeSubDomains).
    let parts: Vec<&str> = host.split('.').collect();
    for i in 1..parts.len() {
        let parent = parts[i..].join(".");
        if let Some(policy) = store.get(&parent) {
            if policy.include_subdomains && !policy.is_expired() {
                return true;
            }
        }
    }

    false
}

/// Process an HTTP response, recording any HSTS policy from the
/// `Strict-Transport-Security` header.
pub fn process_response(host: &str, headers: &[(String, String)]) {
    ensure_store();
    let host = host.to_lowercase();

    // Find the HSTS header.
    let hsts_header = headers.iter().find_map(|(k, v)| {
        if k.eq_ignore_ascii_case("strict-transport-security") {
            Some(v.as_str())
        } else {
            None
        }
    });

    if let Some(header_value) = hsts_header {
        if let Some(policy) = parse_hsts_header(host.as_str(), header_value) {
            let mut store = HSTS_STORE.lock().unwrap();
            store.as_mut().unwrap().insert(host, policy);
        }
    }
}

/// Parse a `Strict-Transport-Security` header value.
fn parse_hsts_header(host: &str, value: &str) -> Option<HstsPolicy> {
    let mut max_age: Option<u64> = None;
    let mut include_subdomains = false;
    let mut preload = false;

    for directive in value.split(';') {
        let directive = directive.trim();
        if directive.eq_ignore_ascii_case("includeSubDomains") {
            include_subdomains = true;
        } else if directive.eq_ignore_ascii_case("preload") {
            preload = true;
        } else if let Some(age_str) = directive.strip_prefix("max-age=") {
            if let Ok(age) = age_str.trim().parse::<u64>() {
                max_age = Some(age);
            }
        } else if let Some(age_str) = directive.strip_prefix("max-age =") {
            if let Ok(age) = age_str.trim().parse::<u64>() {
                max_age = Some(age);
            }
        }
    }

    let max_age = max_age?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Some(HstsPolicy {
        host: host.to_string(),
        expires: now + max_age,
        include_subdomains,
        preload,
    })
}

/// Clear all HSTS policies (for testing).
pub fn clear() {
    ensure_store();
    let mut store = HSTS_STORE.lock().unwrap();
    store.as_mut().unwrap().clear();
}

/// Get the list of known HSTS hosts (for debugging).
pub fn known_hosts() -> Vec<String> {
    ensure_store();
    let store = HSTS_STORE.lock().unwrap();
    store.as_ref().unwrap().keys().cloned().collect()
}

/// Load a preload list of well-known HSTS hosts.
///
/// This is a small built-in list of domains that are known to support HSTS.
/// A full preload list (like Chrome's) would have ~10,000 entries; this
/// is a curated subset for demonstration.
pub fn load_preload_list() {
    let preloaded = [
        "google.com",
        "youtube.com",
        "github.com",
        "twitter.com",
        "facebook.com",
        "instagram.com",
        "linkedin.com",
        "cloudflare.com",
        "mozilla.org",
        "wikipedia.org",
        "reddit.com",
        "stackoverflow.com",
        "paypal.com",
        "stripe.com",
        "amazon.com",
        "microsoft.com",
        "apple.com",
        "netflix.com",
        "spotify.com",
        "github.io",
    ];

    ensure_store();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut store = HSTS_STORE.lock().unwrap();
    let store = store.as_mut().unwrap();
    for host in &preloaded {
        store.insert(
            host.to_string(),
            HstsPolicy {
                host: host.to_string(),
                expires: now + 365 * 24 * 60 * 60, // 1 year
                include_subdomains: true,
                preload: true,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests use a global HSTS store, so they must run serially
    // to avoid interfering with each other.
    use std::sync::Mutex;
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_hsts_basic() {
        let policy = parse_hsts_header("example.com", "max-age=31536000").unwrap();
        assert_eq!(policy.host, "example.com");
        assert!(!policy.include_subdomains);
        assert!(!policy.preload);
    }

    #[test]
    fn parse_hsts_with_subdomains() {
        let policy =
            parse_hsts_header("example.com", "max-age=31536000; includeSubDomains").unwrap();
        assert!(policy.include_subdomains);
    }

    #[test]
    fn parse_hsts_with_preload() {
        let policy =
            parse_hsts_header("example.com", "max-age=31536000; includeSubDomains; preload")
                .unwrap();
        assert!(policy.include_subdomains);
        assert!(policy.preload);
    }

    #[test]
    fn parse_hsts_no_max_age() {
        let policy = parse_hsts_header("example.com", "includeSubDomains");
        assert!(policy.is_none());
    }

    #[test]
    fn process_response_records_policy() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        process_response(
            "test.example.com",
            &[(
                "Strict-Transport-Security".to_string(),
                "max-age=3600".to_string(),
            )],
        );
        assert!(should_upgrade("test.example.com"));
    }

    #[test]
    fn should_upgrade_subdomain() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        process_response(
            "example.com",
            &[(
                "Strict-Transport-Security".to_string(),
                "max-age=3600; includeSubDomains".to_string(),
            )],
        );
        assert!(should_upgrade("sub.example.com"));
        assert!(should_upgrade("deep.sub.example.com"));
    }

    #[test]
    fn should_not_upgrade_without_subdomains() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        process_response(
            "example.com",
            &[(
                "Strict-Transport-Security".to_string(),
                "max-age=3600".to_string(),
            )],
        );
        assert!(should_upgrade("example.com"));
        assert!(!should_upgrade("sub.example.com"));
    }

    #[test]
    fn should_not_upgrade_unknown_host() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        assert!(!should_upgrade("unknown.example.com"));
    }

    #[test]
    fn case_insensitive_host() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        process_response(
            "Example.COM",
            &[(
                "Strict-Transport-Security".to_string(),
                "max-age=3600".to_string(),
            )],
        );
        assert!(should_upgrade("example.com"));
        assert!(should_upgrade("EXAMPLE.COM"));
    }

    #[test]
    fn preload_list() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        load_preload_list();
        assert!(should_upgrade("google.com"));
        assert!(should_upgrade("www.google.com")); // includeSubDomains
        assert!(should_upgrade("github.com"));
        assert!(!should_upgrade("random-unknown-site.com"));
    }

    #[test]
    fn case_insensitive_header_name() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        process_response(
            "example.com",
            &[(
                "strict-transport-security".to_string(),
                "max-age=3600".to_string(),
            )],
        );
        assert!(should_upgrade("example.com"));
    }

    #[test]
    fn known_hosts_returns_list() {
        let _g = TEST_LOCK.lock().unwrap();
        clear();
        process_response(
            "a.example.com",
            &[
                ("Strict-Transport-Security".to_string(), "max-age=3600".to_string()),
            ],
        );
        process_response(
            "b.example.com",
            &[
                ("Strict-Transport-Security".to_string(), "max-age=3600".to_string()),
            ],
        );
        let hosts = known_hosts();
        assert!(hosts.contains(&"a.example.com".to_string()));
        assert!(hosts.contains(&"b.example.com".to_string()));
    }
}
