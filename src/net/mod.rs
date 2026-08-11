//! Network layer — HTTP, HTTPS, WebSocket, cookies, cache, redirects.
//!
//! This module provides:
//! - HTTP/HTTPS fetch with redirect following
//! - Cookie jar (Set-Cookie parsing, Cookie header sending)
//! - Response cache (ETag, Last-Modified conditional requests)
//! - WebSocket client (ws://, wss://) — basic text frame support
//! - Content-Type sniffing + charset detection
//! - URL resolution

pub mod cache;
pub mod cookies;
pub mod redirect;
pub mod websocket;

use std::path::Path;

/// Fetch a URL with cookies, cache, and redirect following.
/// Returns the final response body as a string.
pub fn fetch(url: &str) -> anyhow::Result<String> {
    fetch_with_options(url, FetchOptions::default())
}

/// Fetch a URL with raw bytes.
pub fn fetch_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    fetch_bytes_with_options(url, FetchOptions::default())
}

/// Options for a fetch request.
#[derive(Debug, Clone)]
pub struct FetchOptions {
    pub follow_redirects: bool,
    pub max_redirects: usize,
    pub send_cookies: bool,
    pub receive_cookies: bool,
    pub use_cache: bool,
    pub user_agent: String,
    pub referer: Option<String>,
}

/// Default User-Agent that looks like a real browser (avoids 403 blocks).
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

impl FetchOptions {
    pub fn default_with_redirects() -> Self {
        Self {
            follow_redirects: true,
            max_redirects: 10,
            send_cookies: true,
            receive_cookies: true,
            use_cache: true,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            referer: None,
        }
    }
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            max_redirects: 10,
            send_cookies: false,
            receive_cookies: false,
            use_cache: false,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            referer: None,
        }
    }
}

/// Fetch a URL string with full options.
pub fn fetch_with_options(url: &str, opts: FetchOptions) -> anyhow::Result<String> {
    let bytes = fetch_bytes_with_options(url, opts)?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Fetch raw bytes with full options.
pub fn fetch_bytes_with_options(url: &str, opts: FetchOptions) -> anyhow::Result<Vec<u8>> {
    let mut current_url = url.to_string();
    let mut redirects = 0;
    let max = if opts.follow_redirects {
        opts.max_redirects
    } else {
        0
    };

    loop {
        // Check cache.
        if opts.use_cache {
            if let Some(cached) = cache::get(&current_url) {
                return Ok(cached);
            }
        }

        // Build request with browser-like headers.
        let mut req = ureq::get(&current_url)
            .timeout(std::time::Duration::from_secs(15))
            .set("User-Agent", &opts.user_agent)
            .set(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .set("Accept-Language", "en-US,en;q=0.9")
            .set("Accept-Encoding", "identity")
            .set("Connection", "keep-alive")
            .set("Upgrade-Insecure-Requests", "1")
            .set("Sec-Fetch-Dest", "document")
            .set("Sec-Fetch-Mode", "navigate")
            .set("Sec-Fetch-Site", "none")
            .set("Sec-Fetch-User", "?1");

        if let Some(ref referer) = opts.referer {
            req = req.set("Referer", referer);
        }

        // Send cookies.
        if opts.send_cookies {
            if let Some(cookie_header) = cookies::get_cookie_header(&current_url) {
                req = req.set("Cookie", &cookie_header);
            }
        }

        // Check TLS.
        if current_url.starts_with("https://") && !has_tls() {
            anyhow::bail!("HTTPS not supported in this build (compiled without TLS).");
        }

        // Execute.
        let resp = req.call()?;
        let status = resp.status();

        // Process Set-Cookie headers.
        if opts.receive_cookies {
            cookies::process_response_cookies(&current_url, &resp);
        }

        // Handle redirects.
        if redirect::is_redirect(status) && redirects < max {
            if let Some(location) = resp.header("Location") {
                let resolved = redirect::resolve_redirect(&current_url, location);
                eprintln!("[falco:net] redirect {} → {}", current_url, resolved);
                current_url = resolved;
                redirects += 1;
                continue;
            }
        }

        // Read body.
        let mut bytes = Vec::new();
        resp.into_reader().read_to_end(&mut bytes)?;

        // Detect and decompress gzip-compressed content (.svgz files, etc.).
        // Some servers serve .svgz files as raw gzip bytes without
        // Content-Encoding: gzip header, so ureq's built-in decompression
        // doesn't catch them. We check for the gzip magic bytes (0x1f 0x8b)
        // and decompress manually.
        if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
            eprintln!(
                "[falco:net] detected gzip-compressed content, decompressing {} bytes",
                bytes.len()
            );
            match decompress_gzip(&bytes) {
                Ok(decompressed) => {
                    eprintln!("[falco:net] decompressed to {} bytes", decompressed.len());
                    bytes = decompressed;
                }
                Err(e) => {
                    eprintln!("[falco:net] gzip decompression failed: {}", e);
                }
            }
        }

        // Cache the response.
        if opts.use_cache {
            cache::put(&current_url, bytes.clone());
        }

        return Ok(bytes);
    }
}

/// Read a local file as a string.
pub fn read_file(path: &str) -> anyhow::Result<String> {
    let src = std::fs::read_to_string(Path::new(path))?;
    Ok(src)
}

/// Decompress gzip-compressed bytes using flate2.
/// Used for .svgz files (gzip-compressed SVG) that servers serve as raw
/// bytes without Content-Encoding: gzip header.
fn decompress_gzip(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(bytes);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

/// Check if TLS support is compiled in.
#[cfg(feature = "tls")]
fn has_tls() -> bool {
    true
}

#[cfg(not(feature = "tls"))]
fn has_tls() -> bool {
    false
}

/// Decide whether `s` looks like a URL.
pub fn is_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("ws://")
        || s.starts_with("wss://")
}

/// Sniff the content type from raw bytes.
pub fn sniff_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        return "image/png";
    }
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return "image/jpeg";
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return "image/gif";
    }
    if bytes.starts_with(b"RIFF") && bytes.len() > 11 && &bytes[8..12] == b"WEBP" {
        return "image/webp";
    }
    if bytes.starts_with(b"BM") {
        return "image/bmp";
    }
    if bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg") {
        return "image/svg+xml";
    }
    if bytes.starts_with(b"<!DOCTYPE") || bytes.starts_with(b"<html") || bytes.starts_with(b"<HTML")
    {
        return "text/html";
    }
    if bytes.starts_with(b"{") || bytes.starts_with(b"[") {
        return "application/json";
    }
    "application/octet-stream"
}

use std::io::Read;
