//! HTTP redirect handling — follows 301, 302, 303, 307, 308 redirects.

/// Check if a status code is a redirect.
pub fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Resolve a redirect Location header against the original URL.
pub fn resolve_redirect(original_url: &str, location: &str) -> String {
    // Absolute URL.
    if location.starts_with("http://") || location.starts_with("https://") {
        return location.to_string();
    }
    // Protocol-relative URL (//example.com/path).
    if location.starts_with("//") {
        let scheme = if original_url.starts_with("https://") {
            "https:"
        } else {
            "http:"
        };
        return format!("{}{}", scheme, location);
    }
    // Absolute path (/path).
    if location.starts_with("/") {
        if let Some(scheme_end) = original_url.find("://") {
            let after_scheme = &original_url[scheme_end + 3..];
            let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
            return format!(
                "{}{}",
                &original_url[..scheme_end + 3 + path_start],
                location
            );
        }
    }
    // Relative path — append to the original URL's directory.
    let last_slash = original_url.rfind('/').unwrap_or(original_url.len());
    format!("{}{}", &original_url[..last_slash + 1], location)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_redirects() {
        assert!(is_redirect(301));
        assert!(is_redirect(302));
        assert!(is_redirect(307));
        assert!(!is_redirect(200));
        assert!(!is_redirect(404));
    }

    #[test]
    fn resolves_absolute_redirect() {
        assert_eq!(
            resolve_redirect("https://example.com/old", "https://example.com/new"),
            "https://example.com/new"
        );
    }

    #[test]
    fn resolves_relative_redirect() {
        assert_eq!(
            resolve_redirect("https://example.com/page.html", "/new"),
            "https://example.com/new"
        );
    }

    #[test]
    fn resolves_path_redirect() {
        assert_eq!(
            resolve_redirect("https://example.com/dir/page.html", "other.html"),
            "https://example.com/dir/other.html"
        );
    }
}
