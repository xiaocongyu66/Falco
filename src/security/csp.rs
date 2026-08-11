//! Content Security Policy (CSP) — parsing and enforcement.
//!
//! Spec: https://www.w3.org/TR/CSP3/
//! + https://developer.mozilla.org/en-US/docs/Web/HTTP/CSP
//!
//! CSP is an HTTP header (`Content-Security-Policy`) and `<meta>` tag
//! that restricts what resources a page can load and execute. It's the
//! primary defense against XSS and data injection attacks.
//!
//! Key directives:
//! * `default-src` — fallback for other *-src directives.
//! * `script-src` — allowed sources for JavaScript.
//! * `style-src` — allowed sources for CSS.
//! * `img-src` — allowed sources for images.
//! * `connect-src` — allowed targets for fetch/XHR/WebSocket.
//! * `frame-src` — allowed sources for iframes.
//! * `object-src` — allowed sources for <object>/<embed>.
//! * `media-src` — allowed sources for <audio>/<video>.
//! * `font-src` — allowed sources for @font-face.
//! * `worker-src` — allowed sources for Worker scripts.
//! * `frame-ancestors` — pages allowed to embed this page in an iframe.
//! * `form-action` — allowed targets for form submissions.
//! * `base-uri` — allowed URLs for <base>.
//! * `report-uri` / `report-to` — where to send violation reports.
//! * `upgrade-insecure-requests` — auto-upgrade http: to https:.
//! * `block-all-mixed-content` — block mixed-content loads.
//!
//! Source expressions:
//! * `'self'` — same origin as the page.
//! * `'none'` — no sources allowed.
//! * `'unsafe-inline'` — allow inline <script>/<style>.
//! * `'unsafe-eval'` — allow eval(), new Function().
//! * `'strict-dynamic'` — trust scripts added by already-trusted scripts.
//! * `'nonce-<value>'` — allow scripts with a specific nonce attribute.
//! * `'<hash>'` — allow scripts matching a specific hash (sha256/sha384/sha512).
//! * `host` — e.g. `https://example.com`, `*.example.com`, `example.com:443`.
//! * `data:` — allow data: URLs.
//! * `blob:` — allow blob: URLs.

use crate::security::origin::Origin;
use std::collections::HashMap;

/// A parsed Content-Security-Policy.
#[derive(Debug, Clone, Default)]
pub struct CspPolicy {
    /// Map from directive name → list of source expressions.
    pub directives: HashMap<String, Vec<String>>,
}

impl CspPolicy {
    fn get_sources_ref(&self, directive: &str) -> Option<&Vec<String>> {
        self.directives.get(directive)
            .or_else(|| self.directives.get("default-src"))
    }

    fn check_url(&self, directive: &str, url: &str) -> bool {
        match self.get_sources_ref(directive) {
            None => true,
            Some(s) if s.iter().any(|src| src == "'none'") => false,
            Some(s) => s.iter().any(|src| {
                if src == "'self'" || src == "data:" || src == "blob:" {
                    return true;
                }
                url.starts_with(src.as_str())
            }),
        }
    }
}

impl CspPolicy {
    /// Parse a CSP header value.
    pub fn parse(header: &str) -> Self {
        let mut policy = Self::default();
        for directive in header.split(';') {
            let mut tokens = directive.split_whitespace();
            if let Some(name) = tokens.next() {
                let name_lower = name.to_lowercase();
                let sources: Vec<String> = tokens.map(|s| s.to_string()).collect();
                policy.directives.insert(name_lower, sources);
            }
        }
        policy
    }

    /// Get a directive's source list. Falls back to `default-src` if the
    /// directive is not explicitly set.
    pub fn get_sources(&self, directive: &str) -> Vec<String> {
        if let Some(sources) = self.directives.get(directive) {
            return sources.clone();
        }
        // Fall back to default-src.
        self.directives
            .get("default-src")
            .cloned()
            .unwrap_or_default()
    }

    /// Check if a source is allowed for a directive.
    pub fn is_allowed(&self, directive: &str, source: &str, page_origin: &Origin) -> bool {
        let sources = self.get_sources(directive);
        if sources.is_empty() {
            return true; // No CSP directive — allow everything.
        }
        if sources.iter().any(|s| s == "'none'") {
            return false;
        }
        for src in &sources {
            if Self::source_matches(src, source, page_origin) {
                return true;
            }
        }
        false
    }

    /// Check if a source expression matches a given URL.
    fn source_matches(expr: &str, url: &str, page_origin: &Origin) -> bool {
        match expr {
            "'self'" => {
                let url_origin = Origin::parse(url);
                page_origin.is_same_origin(&url_origin)
            }
            "'unsafe-inline'" | "'unsafe-eval'" | "'strict-dynamic'" => {
                // These are handled at the call site (script-src check).
                false
            }
            "'none'" => false,
            "data:" => url.starts_with("data:"),
            "blob:" => url.starts_with("blob:"),
            "filesystem:" => url.starts_with("filesystem:"),
            "http:" => url.starts_with("http:"),
            "https:" => url.starts_with("https:"),
            _ => {
                // Host expression: scheme://host[:port] or host[:port] or *.host
                Self::host_matches(expr, url, page_origin)
            }
        }
    }

    fn host_matches(expr: &str, url: &str, page_origin: &Origin) -> bool {
        let url_origin = Origin::parse(url);
        if url_origin.opaque {
            return false;
        }
        // Strip scheme from expr if present.
        let (expr_scheme, expr_host) = if let Some(i) = expr.find("://") {
            (Some(&expr[..i]), &expr[i + 3..])
        } else {
            (None, expr)
        };
        // Check scheme.
        if let Some(s) = expr_scheme {
            if s != url_origin.scheme {
                return false;
            }
        } else {
            // No scheme — inherit from page origin.
            if url_origin.scheme != page_origin.scheme {
                return false;
            }
        }
        // Check host (with wildcard support).
        let expr_host = expr_host.trim_end_matches('/');
        if expr_host.starts_with("*.") {
            let suffix = &expr_host[1..]; // ".example.com"
            return url_origin.host.ends_with(suffix) || url_origin.host == expr_host[2..];
        }
        if expr_host != url_origin.host {
            return false;
        }
        // Check port (if specified in expr).
        if let Some(colon) = expr_host.find(':') {
            let port_str = &expr_host[colon + 1..];
            if let Ok(port) = port_str.parse::<u16>() {
                if url_origin.port != Some(port) {
                    return false;
                }
            }
        }
        true
    }

    /// Check if inline scripts are allowed (script-src contains 'unsafe-inline'
    /// or a nonce/hash matches).
    pub fn allows_inline_script(&self, nonce: Option<&str>, hash: Option<&str>) -> bool {
        let sources = self.get_sources("script-src");
        if sources.is_empty() {
            return true;
        }
        if sources.iter().any(|s| s == "'unsafe-inline'") {
            return true;
        }
        if let Some(n) = nonce {
            if sources.iter().any(|s| s == &format!("'nonce-{}'", n)) {
                return true;
            }
        }
        if let Some(h) = hash {
            if sources.iter().any(|s| s == &format!("'{}'", h)) {
                return true;
            }
        }
        false
    }

    /// Check if `eval()` is allowed.
    pub fn allows_eval(&self) -> bool {
        let sources = self.get_sources("script-src");
        if sources.is_empty() {
            return true;
        }
        sources.iter().any(|s| s == "'unsafe-eval'")
    }

    /// Check if a `javascript:` URL is allowed.
    ///
    /// Per CSP spec, `javascript:` URLs are treated as inline scripts.
    /// They're blocked unless `script-src` includes 'unsafe-inline'.
    pub fn allows_javascript_url(&self) -> bool {
        self.allows_inline_script(None, None)
    }

    /// Check if a URL is allowed for script loading (script-src / default-src).
    pub fn allows_script_src(&self, url: &str) -> bool {
        self.check_url("script-src", url)
    }

    /// Check if a URL is allowed for fetch/XHR/WebSocket (connect-src).
    pub fn allows_connect_src(&self, url: &str) -> bool {
        self.check_url("connect-src", url)
    }

    /// Check if a URL is allowed for image loading (img-src).
    pub fn allows_image_src(&self, url: &str) -> bool {
        self.check_url("img-src", url)
    }

    /// Check if a URL is allowed for style loading (style-src).
    pub fn allows_style_src(&self, url: &str) -> bool {
        self.check_url("style-src", url)
    }
}

/// A CSP violation report. Sent to the `report-uri` (or via `report-to`).
#[derive(Debug, Clone)]
pub struct CspViolationReport {
    pub blocked_uri: String,
    pub violated_directive: String,
    pub document_uri: String,
    pub line_number: u32,
    pub column_number: u32,
    pub source_file: String,
    pub policy: String,
}

/// Check whether a `javascript:` URL is safe to execute.
///
/// This is the entry point for #91 — Content Security for javascript: URLs.
/// Returns Ok(()) if the URL is allowed, or an error describing why.
pub fn check_javascript_url(
    url: &str,
    policy: &CspPolicy,
    page_origin: &Origin,
) -> Result<(), CspViolation> {
    if !url.starts_with("javascript:") {
        return Ok(()); // Not a javascript: URL — nothing to check.
    }
    if !policy.allows_javascript_url() {
        return Err(CspViolation {
            blocked_uri: url.to_string(),
            violated_directive: "script-src".to_string(),
            reason: "javascript: URLs require 'unsafe-inline' in script-src".to_string(),
        });
    }
    let _ = page_origin;
    Ok(())
}

/// A CSP violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CspViolation {
    pub blocked_uri: String,
    pub violated_directive: String,
    pub reason: String,
}

/// Sanitize a string for safe insertion into HTML text content.
///
/// This is a defense-in-depth measure against XSS. The primary defense is
/// CSP; this catches cases where untrusted data is inserted via
/// `innerHTML` without escaping.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '/' => out.push_str("&#47;"),
            _ => out.push(c),
        }
    }
    out
}

/// Check if a URL is safe to assign to `innerHTML`. Always returns false
/// for `innerHTML` — it's inherently unsafe and should be replaced with
/// `textContent` or `DOMPurify.sanitize()`.
pub fn is_safe_for_inner_html(_html: &str) -> bool {
    // We can't statically determine if HTML is safe. Always recommend
    // using textContent or a sanitizer.
    false
}

/// Check if an attribute value is safe. Some attributes can execute
/// JavaScript (e.g. `onclick="..."`).
pub fn is_safe_attribute(tag: &str, attr_name: &str, attr_value: &str) -> bool {
    let name_lower = attr_name.to_lowercase();
    // Block event handler attributes (onclick, onload, etc.).
    if name_lower.starts_with("on") {
        return false;
    }
    // Block javascript: URLs in href/src/action/etc.
    if matches!(
        name_lower.as_str(),
        "href" | "src" | "action" | "formaction" | "data" | "xlink:href"
    ) && attr_value.trim().to_lowercase().starts_with("javascript:")
    {
        return false;
    }
    // Block `style` attribute with `expression()` (IE legacy).
    if name_lower == "style" && attr_value.contains("expression(") {
        return false;
    }
    // Block `srcdoc` with embedded javascript: URLs in iframes.
    if tag.eq_ignore_ascii_case("iframe") && name_lower == "srcdoc" {
        // Real check would parse the HTML and recurse.
        if attr_value.to_lowercase().contains("javascript:") {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csp_header() {
        let p = CspPolicy::parse("default-src 'self'; script-src 'self' https://cdn.example.com");
        assert_eq!(p.get_sources("default-src"), vec!["'self'"]);
        assert_eq!(
            p.get_sources("script-src"),
            vec!["'self'", "https://cdn.example.com"]
        );
    }

    #[test]
    fn falls_back_to_default_src() {
        let p = CspPolicy::parse("default-src 'self'");
        // No explicit img-src — should fall back to default-src.
        assert_eq!(p.get_sources("img-src"), vec!["'self'"]);
    }

    #[test]
    fn allows_self_origin() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("default-src 'self'");
        assert!(p.is_allowed("img-src", "https://example.com/logo.png", &page));
        assert!(!p.is_allowed("img-src", "https://other.com/logo.png", &page));
    }

    #[test]
    fn blocks_when_none() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("default-src 'none'");
        assert!(!p.is_allowed("img-src", "https://example.com/logo.png", &page));
    }

    #[test]
    fn allows_data_urls() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("img-src data:");
        assert!(p.is_allowed("img-src", "data:image/png;base64,...", &page));
    }

    #[test]
    fn allows_wildcard_subdomains() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("img-src https://*.example.com");
        assert!(p.is_allowed("img-src", "https://cdn.example.com/x.png", &page));
        assert!(p.is_allowed("img-src", "https://images.cdn.example.com/x.png", &page));
        assert!(!p.is_allowed("img-src", "https://other.com/x.png", &page));
    }

    #[test]
    fn allows_specific_origin() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("script-src https://cdn.example.com");
        assert!(p.is_allowed("script-src", "https://cdn.example.com/script.js", &page));
        assert!(!p.is_allowed("script-src", "https://other.com/script.js", &page));
    }

    #[test]
    fn allows_inline_with_unsafe_inline() {
        let p = CspPolicy::parse("script-src 'unsafe-inline'");
        assert!(p.allows_inline_script(None, None));
    }

    #[test]
    fn blocks_inline_without_unsafe_inline() {
        let p = CspPolicy::parse("script-src 'self'");
        assert!(!p.allows_inline_script(None, None));
    }

    #[test]
    fn allows_inline_with_nonce() {
        let p = CspPolicy::parse("script-src 'nonce-abc123'");
        assert!(p.allows_inline_script(Some("abc123"), None));
        assert!(!p.allows_inline_script(Some("wrong"), None));
    }

    #[test]
    fn allows_eval_with_unsafe_eval() {
        let p = CspPolicy::parse("script-src 'unsafe-eval'");
        assert!(p.allows_eval());
    }

    #[test]
    fn blocks_javascript_url() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("script-src 'self'");
        let result = check_javascript_url("javascript:alert(1)", &p, &page);
        assert!(result.is_err());
    }

    #[test]
    fn allows_javascript_url_with_unsafe_inline() {
        let page = Origin::parse("https://example.com");
        let p = CspPolicy::parse("script-src 'unsafe-inline'");
        let result = check_javascript_url("javascript:alert(1)", &p, &page);
        assert!(result.is_ok());
    }

    #[test]
    fn escapes_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("\"quote\""), "&quot;quote&quot;");
    }

    #[test]
    fn blocks_event_handler_attributes() {
        assert!(!is_safe_attribute("div", "onclick", "alert(1)"));
        assert!(!is_safe_attribute("img", "onerror", "alert(1)"));
    }

    #[test]
    fn blocks_javascript_in_href() {
        assert!(!is_safe_attribute("a", "href", "javascript:alert(1)"));
        assert!(is_safe_attribute("a", "href", "https://example.com"));
    }

    #[test]
    fn allows_safe_attributes() {
        assert!(is_safe_attribute("img", "src", "https://example.com/x.png"));
        assert!(is_safe_attribute("div", "class", "foo"));
        assert!(is_safe_attribute("input", "type", "text"));
    }

    #[test]
    fn inner_html_is_never_safe() {
        assert!(!is_safe_for_inner_html("<p>hello</p>"));
        assert!(!is_safe_for_inner_html("just text"));
    }

    #[test]
    fn blocks_expression_in_style() {
        assert!(!is_safe_attribute(
            "div",
            "style",
            "width: expression(alert(1))"
        ));
        assert!(is_safe_attribute("div", "style", "color: red"));
    }
}
