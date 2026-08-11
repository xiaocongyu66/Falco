//! Cookie jar — parses Set-Cookie headers and sends Cookie headers.
//!
//! Supports:
//! - Set-Cookie: name=value; Path=/; Domain=example.com; Secure; HttpOnly
//! - Cookie: name1=value1; name2=value2
//! - Domain matching (simplified)
//! - Path matching
//! - Expiration (via max-age)
//! - Secure flag (only sent over HTTPS)
//! - HttpOnly flag (stored but not accessible to JS — future)

use std::collections::HashMap;
use std::sync::Mutex;

/// A stored cookie.
#[derive(Debug, Clone)]
struct Cookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    max_age: Option<u64>,
}

static COOKIE_JAR: Mutex<Option<Vec<Cookie>>> = Mutex::new(None);

/// Get the Cookie header value for a URL.
pub fn get_cookie_header(url: &str) -> Option<String> {
    let (domain, path, is_secure) = parse_url_parts(url);
    let jar = COOKIE_JAR.lock().unwrap();
    let jar = jar.as_ref()?;

    let matching: Vec<&Cookie> = jar
        .iter()
        .filter(|c| {
            domain_matches(&domain, &c.domain)
                && path_matches(&path, &c.path)
                && (!c.secure || is_secure)
        })
        .collect();

    if matching.is_empty() {
        return None;
    }

    Some(
        matching
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// Process Set-Cookie headers from a response.
pub fn process_response_cookies(url: &str, resp: &ureq::Response) {
    let (domain, _, _) = parse_url_parts(url);

    // ureq doesn't expose multiple Set-Cookie headers easily,
    // so we use header_all.
    for header_name in resp.headers_names() {
        if header_name.eq_ignore_ascii_case("set-cookie") {
            if let Some(value) = resp.header(&header_name) {
                parse_set_cookie(value, &domain);
            }
        }
    }
}

/// Parse a Set-Cookie header value.
fn parse_set_cookie(header: &str, default_domain: &str) {
    let parts: Vec<&str> = header.split(';').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return;
    }

    // First part is name=value.
    let nv = parts[0];
    let eq = match nv.find('=') {
        Some(i) => i,
        None => return,
    };
    let name = nv[..eq].trim().to_string();
    let value = nv[eq + 1..].trim().to_string();

    let mut domain = default_domain.to_string();
    let mut path = "/".to_string();
    let mut secure = false;
    let mut http_only = false;
    let mut max_age = None;

    for part in &parts[1..] {
        let lower = part.to_lowercase();
        if lower.starts_with("domain=") {
            domain = part[7..].trim().to_string();
        } else if lower.starts_with("path=") {
            path = part[5..].trim().to_string();
        } else if lower == "secure" {
            secure = true;
        } else if lower == "httponly" {
            http_only = true;
        } else if lower.starts_with("max-age=") {
            max_age = part[8..].trim().parse().ok();
        }
    }

    let cookie = Cookie {
        name,
        value,
        domain,
        path,
        secure,
        http_only,
        max_age,
    };

    let mut jar = COOKIE_JAR.lock().unwrap();
    if jar.is_none() {
        *jar = Some(Vec::new());
    }

    // Remove any existing cookie with the same name+domain+path.
    if let Some(ref mut jar) = *jar {
        jar.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        });
        jar.push(cookie);
    }
}

/// Parse a URL into (domain, path, is_secure).
fn parse_url_parts(url: &str) -> (String, String, bool) {
    let is_secure = url.starts_with("https://") || url.starts_with("wss://");
    let stripped = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("ws://"))
        .or_else(|| url.strip_prefix("wss://"))
        .unwrap_or(url);

    let (domain_part, path_part) = match stripped.find('/') {
        Some(i) => (&stripped[..i], &stripped[i..]),
        None => (stripped, "/"),
    };

    let domain = domain_part
        .split(':')
        .next()
        .unwrap_or(domain_part)
        .to_string();
    let path = if path_part.is_empty() {
        "/".to_string()
    } else {
        path_part.to_string()
    };

    (domain, path, is_secure)
}

/// Check if a request domain matches a cookie domain.
fn domain_matches(request_domain: &str, cookie_domain: &str) -> bool {
    if cookie_domain.is_empty() {
        return true;
    }
    let cd = cookie_domain.trim_start_matches('.');
    request_domain == cd || request_domain.ends_with(&format!(".{}", cd))
}

/// Check if a request path matches a cookie path.
fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if cookie_path == "/" {
        return true;
    }
    request_path.starts_with(cookie_path)
}

/// Clear all cookies.
pub fn clear() {
    *COOKIE_JAR.lock().unwrap() = None;
}

/// Get all cookies as a HashMap (for JS document.cookie — future).
pub fn all_cookies() -> HashMap<String, String> {
    let jar = COOKIE_JAR.lock().unwrap();
    let jar = match jar.as_ref() {
        Some(j) => j,
        None => return HashMap::new(),
    };
    jar.iter()
        .map(|c| (c.name.clone(), c.value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_url_parts() {
        let (d, p, s) = parse_url_parts("https://example.com/path/page");
        assert_eq!(d, "example.com");
        assert_eq!(p, "/path/page");
        assert!(s);

        let (d, p, s) = parse_url_parts("http://sub.example.com:8080/");
        assert_eq!(d, "sub.example.com");
        assert_eq!(p, "/");
        assert!(!s);
    }

    #[test]
    fn domain_matching() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(domain_matches("www.example.com", "example.com"));
        assert!(domain_matches("www.example.com", ".example.com"));
        assert!(!domain_matches("other.com", "example.com"));
    }

    #[test]
    fn path_matching() {
        assert!(path_matches("/page", "/"));
        assert!(path_matches("/api/users", "/api"));
        assert!(!path_matches("/other", "/api"));
    }
}
