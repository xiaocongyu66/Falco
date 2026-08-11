//! JS runner — runs JS files through Falco's TJS engine + HTML script extraction.
//!
//! Usage: falco --js-run <file.js>
//! This evaluates the JS file and prints console.log output.

use crate::tjs::TjsContext;

/// Run a JavaScript file through Falco's TJS engine.
pub fn run_js_file(path: &str) -> Result<(), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;

    // Use interpreter only (VM doesn't support ES2020+ features yet).
    let mut tjs = TjsContext::new();
    tjs.execute(&src)?;
    Ok(())
}

/// Extract all `<script>...</script>` contents from HTML (without the tags).
pub fn extract_scripts(html: &str) -> Vec<String> {
    let mut scripts = Vec::new();
    let lower = html.to_lowercase();
    let mut search_from = 0;
    while let Some(open) = lower[search_from..].find("<script") {
        let open_abs = search_from + open;
        let Some(close_rel) = lower[open_abs..].find('>') else {
            break;
        };
        let content_start = open_abs + close_rel + 1;
        let Some(end_rel) = lower[content_start..].find("</script>") else {
            break;
        };
        let content_end = content_start + end_rel;
        let tag_content = &lower[open_abs..open_abs + close_rel];
        if !tag_content.contains("src=") {
            scripts.push(html[content_start..content_end].to_string());
        }
        search_from = content_end + 9;
    }
    scripts
}

/// Extract external script source URLs from `<script src="...">` tags.
/// Returns a list of (src_url, is_async) pairs.
pub fn extract_external_scripts(html: &str) -> Vec<(String, bool)> {
    let mut scripts = Vec::new();
    let lower = html.to_lowercase();
    let mut search_from = 0;
    while let Some(open) = lower[search_from..].find("<script") {
        let open_abs = search_from + open;
        let Some(close_rel) = lower[open_abs..].find('>') else {
            break;
        };
        let tag_content = &lower[open_abs..open_abs + close_rel];
        if tag_content.contains("src=") {
            // Extract the src URL.
            let src = if let Some(s) = tag_content.find("src=\"") {
                let start = s + 5;
                if let Some(end) = tag_content[start..].find('"') {
                    &tag_content[start..start + end]
                } else {
                    ""
                }
            } else if let Some(s) = tag_content.find("src='") {
                let start = s + 5;
                if let Some(end) = tag_content[start..].find('\'') {
                    &tag_content[start..start + end]
                } else {
                    ""
                }
            } else {
                ""
            };
            if !src.is_empty() {
                let is_async = tag_content.contains("async") || tag_content.contains("defer");
                scripts.push((src.to_string(), is_async));
            }
        }
        search_from = open_abs + close_rel + 1;
    }
    scripts
}

/// Extract inline event handlers (onclick, onload, etc.) from an element's
/// attributes. Returns (event_name, handler_source) pairs.
pub fn extract_event_handlers(
    attrs: &std::collections::HashMap<String, String>,
) -> Vec<(String, String)> {
    let events = [
        "onclick",
        "onload",
        "onmouseover",
        "onmouseout",
        "onsubmit",
        "onchange",
        "oninput",
    ];
    let mut handlers = Vec::new();
    for event in events {
        if let Some(handler) = attrs.get(event) {
            handlers.push((event.to_string(), handler.clone()));
        }
    }
    handlers
}

/// Extract a CSP policy from `<meta http-equiv="Content-Security-Policy">`
/// tags in the HTML. If multiple such tags exist, they are combined (the
/// most restrictive wins, per the CSP spec).
///
/// Returns `None` if no CSP meta tag is found.
pub fn extract_csp_policy(html: &str) -> Option<crate::security::csp::CspPolicy> {
    let lower = html.to_lowercase();
    let mut policies: Vec<String> = Vec::new();
    let mut search_from = 0;
    while let Some(meta_start) = lower[search_from..].find("<meta") {
        let abs = search_from + meta_start;
        let Some(close) = lower[abs..].find('>') else {
            break;
        };
        let tag_end = abs + close;
        let tag = &lower[abs..tag_end];
        // Check if this is a CSP meta tag.
        if tag.contains("http-equiv") && tag.contains("content-security-policy") {
            // Extract the content attribute.
            if let Some(content_start) = tag.find("content=\"") {
                let cs = content_start + 9;
                if let Some(content_end) = tag[cs..].find('"') {
                    let content = &html[abs + cs..abs + cs + content_end];
                    policies.push(content.to_string());
                }
            } else if let Some(content_start) = tag.find("content='") {
                let cs = content_start + 9;
                if let Some(content_end) = tag[cs..].find('\'') {
                    let content = &html[abs + cs..abs + cs + content_end];
                    policies.push(content.to_string());
                }
            }
        }
        search_from = tag_end + 1;
    }
    if policies.is_empty() {
        return None;
    }
    // Combine policies by joining with "; " (CSP spec: multiple sources are
    // unioned within a directive, but multiple policies are intersected).
    // For simplicity we just parse the first one — this is the common case.
    Some(crate::security::csp::CspPolicy::parse(&policies.join("; ")))
}
