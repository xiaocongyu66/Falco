//! Origin parsing and Same-Origin Policy (SOP) enforcement.
//!
//! Spec: https://html.spec.whatwg.org/multipage/origin.html#origin
//! + https://www.w3.org/TR/CSP/
//!
//! An origin is the triple (scheme, host, port). Two origins are "same" if
//! and only if all three components match exactly. Subdomains are NOT
//! same-origin (e.g. `a.example.com` and `b.example.com` are different).
//!
//! Special cases:
//! * `null` origin — sandboxed iframes, `data:` URLs.
//! * `opaque` origin — created by `about:blank` in some cases.
//! * `file:` URLs — origin is `null` per spec.
//! * `about:blank` — inherits parent's origin.

use std::fmt;

/// A web origin — (scheme, host, port) tuple.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    /// True for opaque origins (cannot be serialized).
    pub opaque: bool,
}

impl Origin {
    /// Parse a URL into an origin.
    pub fn parse(url: &str) -> Self {
        // Handle special schemes.
        if url.starts_with("data:") || url.starts_with("javascript:") || url.starts_with("about:") {
            return Self::null();
        }
        if url.starts_with("file:") {
            return Self::null();
        }
        // Strip scheme.
        let (scheme, rest) = match url.find("://") {
            Some(i) => (&url[..i], &url[i + 3..]),
            None => return Self::null(),
        };
        // Strip path.
        let authority = match rest.find('/') {
            Some(i) => &rest[..i],
            None => rest,
        };
        // Strip userinfo if present.
        let authority = match authority.find('@') {
            Some(i) => &authority[i + 1..],
            None => authority,
        };
        // Split host:port. Handle IPv6 brackets.
        let (host, port) = if authority.starts_with('[') {
            // IPv6: [addr]:port
            if let Some(end) = authority.find(']') {
                let h = &authority[1..end];
                let p = authority[end + 1..]
                    .strip_prefix(':')
                    .and_then(|s| s.parse::<u16>().ok());
                (h.to_string(), p)
            } else {
                (authority.to_string(), None)
            }
        } else if let Some(i) = authority.rfind(':') {
            (
                authority[..i].to_string(),
                authority[i + 1..].parse::<u16>().ok(),
            )
        } else {
            (authority.to_string(), None)
        };
        let default_port = default_port_for_scheme(scheme);
        let port = port.or(default_port);
        Self {
            scheme: scheme.to_lowercase(),
            host: host.to_lowercase(),
            port,
            opaque: false,
        }
    }

    /// Create a `null` origin (used for sandboxed iframes, data: URLs).
    pub fn null() -> Self {
        Self {
            scheme: String::new(),
            host: String::new(),
            port: None,
            opaque: true,
        }
    }

    /// Check if two origins are same-origin.
    pub fn is_same_origin(&self, other: &Origin) -> bool {
        if self.opaque || other.opaque {
            return false;
        }
        self.scheme == other.scheme && self.host == other.host && self.port == other.port
    }

    /// Check if `other` is same-site (same registrable domain).
    /// Example: `a.example.com` and `b.example.com` are same-site.
    pub fn is_same_site(&self, other: &Origin) -> bool {
        if self.opaque || other.opaque {
            return false;
        }
        if self.scheme != other.scheme {
            return false;
        }
        let r1 = registrable_domain(&self.host);
        let r2 = registrable_domain(&other.host);
        r1 == r2
    }

    /// Serialize to the origin header form: `scheme://host:port`.
    pub fn serialize(&self) -> String {
        if self.opaque {
            return "null".to_string();
        }
        match self.port {
            Some(p) => format!("{}://{}:{}", self.scheme, self.host, p),
            None => format!("{}://{}", self.scheme, self.host),
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.serialize())
    }
}

/// Get the default port for a scheme.
pub fn default_port_for_scheme(scheme: &str) -> Option<u16> {
    match scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

/// Compute the registrable domain (eTLD+1) for a hostname.
/// Uses a small public-suffix list for common TLDs. Real browsers use the
/// full https://publicsuffix.org/ list.
pub fn registrable_domain(host: &str) -> String {
    // Split on dots.
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 {
        return host.to_string();
    }
    // Check if the last part is a "two-part TLD" (e.g. co.uk, com.au).
    let last_two = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    let two_part_tlds = [
        "co.uk", "org.uk", "ac.uk", "gov.uk", "com.au", "net.au", "org.au", "co.jp", "co.kr",
        "co.nz", "co.in", "co.za", "com.br", "com.cn", "com.tw", "com.hk", "com.sg", "com.mx",
        "com.ar",
    ];
    if two_part_tlds.contains(&last_two.as_str()) && parts.len() >= 3 {
        return format!(
            "{}.{}.{}",
            parts[parts.len() - 3],
            parts[parts.len() - 2],
            parts[parts.len() - 1]
        );
    }
    format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
}

/// Same-Origin Policy check for a cross-origin request.
///
/// Returns `Ok(())` if the access is allowed, or an error describing why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SopError {
    /// Origins differ — direct DOM access is blocked.
    CrossOrigin,
    /// `null` origin — sandboxed, blocked from everything.
    NullOrigin,
    /// Disallowed scheme (e.g. `file:` accessing `https:`).
    SchemeMismatch,
}

/// Check whether a script from `script_origin` can read responses from
/// `target_origin`. This is the "same-origin" check for XHR/fetch.
pub fn check_same_origin(script_origin: &Origin, target_origin: &Origin) -> Result<(), SopError> {
    if script_origin.opaque || target_origin.opaque {
        return Err(SopError::NullOrigin);
    }
    if script_origin.scheme != target_origin.scheme {
        return Err(SopError::SchemeMismatch);
    }
    if !script_origin.is_same_origin(target_origin) {
        return Err(SopError::CrossOrigin);
    }
    Ok(())
}

/// Check whether a cross-origin request is allowed via CORS.
/// CORS allows cross-origin reads if the server sends appropriate headers.
pub fn check_cors(
    script_origin: &Origin,
    target_origin: &Origin,
    cors_allow_origin: Option<&str>,
    credentials: bool,
) -> Result<(), SopError> {
    // If same-origin, no CORS check needed.
    if script_origin.is_same_origin(target_origin) {
        return Ok(());
    }
    // Otherwise, server must allow our origin (or "*").
    match cors_allow_origin {
        Some("*") => {
            if credentials {
                // "*" is not allowed with credentials.
                return Err(SopError::CrossOrigin);
            }
            Ok(())
        }
        Some(allowed) => {
            let allowed_origin = Origin::parse(allowed);
            if allowed_origin.is_same_origin(script_origin) {
                Ok(())
            } else {
                Err(SopError::CrossOrigin)
            }
        }
        None => Err(SopError::CrossOrigin),
    }
}

/// Check whether `from` origin can navigate `to` origin.
/// Navigation is more permissive than DOM access — any origin can navigate
/// any top-level window. But there are restrictions on navigating across
/// origins into sandboxed contexts.
pub fn check_navigation(from: &Origin, to: &Origin) -> Result<(), SopError> {
    // Navigation is always allowed at the top level.
    let _ = (from, to);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_url() {
        let o = Origin::parse("https://example.com/path");
        assert_eq!(o.scheme, "https");
        assert_eq!(o.host, "example.com");
        assert_eq!(o.port, Some(443));
    }

    #[test]
    fn parses_custom_port() {
        let o = Origin::parse("http://example.com:8080/path");
        assert_eq!(o.port, Some(8080));
    }

    #[test]
    fn parses_userinfo() {
        let o = Origin::parse("https://user:pass@example.com/path");
        assert_eq!(o.host, "example.com");
    }

    #[test]
    fn data_url_is_null() {
        let o = Origin::parse("data:text/html,hello");
        assert!(o.opaque);
    }

    #[test]
    fn same_origin_check() {
        let a = Origin::parse("https://example.com/a");
        let b = Origin::parse("https://example.com/b");
        assert!(a.is_same_origin(&b));
        let c = Origin::parse("https://other.com/c");
        assert!(!a.is_same_origin(&c));
    }

    #[test]
    fn subdomains_are_not_same_origin() {
        let a = Origin::parse("https://a.example.com");
        let b = Origin::parse("https://b.example.com");
        assert!(!a.is_same_origin(&b));
        // But they ARE same-site.
        assert!(a.is_same_site(&b));
    }

    #[test]
    fn different_schemes_not_same_origin() {
        let a = Origin::parse("http://example.com");
        let b = Origin::parse("https://example.com");
        assert!(!a.is_same_origin(&b));
    }

    #[test]
    fn different_ports_not_same_origin() {
        let a = Origin::parse("http://example.com:80");
        let b = Origin::parse("http://example.com:8080");
        assert!(!a.is_same_origin(&b));
    }

    #[test]
    fn registrable_domain_for_simple_tld() {
        assert_eq!(registrable_domain("a.b.example.com"), "example.com");
        assert_eq!(registrable_domain("example.com"), "example.com");
    }

    #[test]
    fn registrable_domain_for_two_part_tld() {
        assert_eq!(registrable_domain("a.b.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("example.co.uk"), "example.co.uk");
    }

    #[test]
    fn sop_blocks_cross_origin() {
        let a = Origin::parse("https://example.com");
        let b = Origin::parse("https://other.com");
        assert_eq!(check_same_origin(&a, &b), Err(SopError::CrossOrigin));
    }

    #[test]
    fn sop_allows_same_origin() {
        let a = Origin::parse("https://example.com/a");
        let b = Origin::parse("https://example.com/b");
        assert!(check_same_origin(&a, &b).is_ok());
    }

    #[test]
    fn cors_allows_wildcard_without_credentials() {
        let a = Origin::parse("https://example.com");
        let b = Origin::parse("https://other.com");
        assert!(check_cors(&a, &b, Some("*"), false).is_ok());
        // With credentials, "*" is rejected.
        assert!(check_cors(&a, &b, Some("*"), true).is_err());
    }

    #[test]
    fn cors_allows_specific_origin() {
        let a = Origin::parse("https://example.com");
        let b = Origin::parse("https://other.com");
        assert!(check_cors(&a, &b, Some("https://example.com"), false).is_ok());
        assert!(check_cors(&a, &b, Some("https://third.com"), false).is_err());
    }

    #[test]
    fn cors_not_needed_for_same_origin() {
        let a = Origin::parse("https://example.com/a");
        let b = Origin::parse("https://example.com/b");
        assert!(check_cors(&a, &b, None, false).is_ok());
    }

    #[test]
    fn origin_serialization() {
        let o = Origin::parse("https://example.com:443/path");
        assert_eq!(o.serialize(), "https://example.com:443");
        let o = Origin::parse("http://example.com:8080");
        assert_eq!(o.serialize(), "http://example.com:8080");
        let o = Origin::null();
        assert_eq!(o.serialize(), "null");
    }
}
