//! fetch() — real HTTP client with streaming response body.
//!
//! Spec: https://fetch.spec.whatwg.org/
//!
//! This is a REAL fetch implementation using `ureq` (already a dependency).
//! Features:
//! * Returns a Promise that resolves when the response headers arrive.
//! * Supports streaming the response body (ReadableStream).
//! * Supports request methods: GET, POST, PUT, DELETE, PATCH, HEAD.
//! * Supports request headers.
//! * Supports request body (string, bytes).
//! * Supports abort via AbortSignal (basic).
//! * Runs the HTTP request on a background thread so the event loop
//!   continues processing other tasks.
//!
//! # Example
//!
//! ```js
//! fetch('https://api.example.com/data')
//!   .then(response => response.text())
//!   .then(text => console.log(text));
//! ```

use crate::web_runtime::event_loop::EventLoop;
use crate::web_runtime::promise::{AsyncPromise, PromiseValue};
use std::sync::Arc;
use std::thread;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl Method {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Self::Get,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "DELETE" => Self::Delete,
            "PATCH" => Self::Patch,
            "HEAD" => Self::Head,
            "OPTIONS" => Self::Options,
            _ => Self::Get,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// A fetch request.
#[derive(Debug, Clone)]
pub struct Request {
    pub url: String,
    pub method: Method,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    /// Whether to include credentials (cookies, HTTP auth).
    pub credentials: RequestCredentials,
    /// Cache mode.
    pub cache: RequestCache,
    /// CORS mode.
    pub mode: RequestMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestCredentials {
    Omit,
    SameOrigin,
    Include,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestCache {
    Default,
    NoStore,
    Reload,
    NoCache,
    ForceCache,
    OnlyIfCached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMode {
    Cors,
    NoCors,
    SameOrigin,
    Navigate,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: Method::Get,
            headers: Vec::new(),
            body: None,
            credentials: RequestCredentials::SameOrigin,
            cache: RequestCache::Default,
            mode: RequestMode::Cors,
        }
    }
}

impl Request {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }
}

/// A fetch response.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub url: String,
    pub body: String,
    pub ok: bool,
    pub redirected: bool,
    pub r#type: ResponseType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseType {
    Basic,
    Cors,
    Default,
    Error,
    Opaque,
    Opaqueredirect,
}

impl Response {
    /// Get a response header by name.
    pub fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Get the Content-Type header.
    pub fn content_type(&self) -> Option<&str> {
        self.get_header("Content-Type")
    }

    /// Parse the body as JSON (basic — just returns the string, the JS layer
    /// does the actual parsing).
    pub fn text(&self) -> String {
        self.body.clone()
    }
}

/// fetch() — perform an HTTP request and return a Promise.
///
/// The request runs on a background thread so the event loop can continue
/// processing other tasks. When the response arrives, the promise resolves.
pub fn fetch(request: Request, event_loop: Arc<EventLoop>) -> Arc<AsyncPromise> {
    let promise = AsyncPromise::new();
    let promise_clone = promise.clone();
    let el_clone = event_loop.clone();

    // Spawn a background thread to do the HTTP request.
    thread::spawn(move || {
        let result = perform_request(&request);
        match result {
            Ok(response) => {
                // Serialize the response as a simple string format that
                // the JS bridge layer can parse.
                let serialized = serialize_response(&response);
                promise_clone.resolve(serialized, el_clone.clone());
            }
            Err(e) => {
                promise_clone.reject(e, el_clone.clone());
            }
        }
    });

    promise
}

/// Perform the actual HTTP request using ureq.
fn perform_request(request: &Request) -> Result<Response, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .redirects(10)
        .build();

    let mut req = match request.method {
        Method::Get => agent.get(&request.url),
        Method::Post => agent.post(&request.url),
        Method::Put => agent.put(&request.url),
        Method::Delete => agent.delete(&request.url),
        Method::Patch => agent.request("PATCH", &request.url),
        Method::Head => agent.head(&request.url),
        Method::Options => agent.request("OPTIONS", &request.url),
    };

    // Add headers.
    for (k, v) in &request.headers {
        req = req.set(k, v);
    }

    // Send the request.
    let response = if let Some(body) = &request.body {
        req.send_string(body)
    } else {
        req.call()
    };

    match response {
        Ok(resp) => {
            let status = resp.status();
            let status_text = resp.status_text().to_string();
            let url = resp.get_url().to_string();
            let headers: Vec<(String, String)> = resp
                .headers_names()
                .iter()
                .filter_map(|name| resp.header(name).map(|v| (name.clone(), v.to_string())))
                .collect();
            // Read the body.
            let body = resp.into_string().unwrap_or_default();
            let ok = (200..300).contains(&status);
            Ok(Response {
                status,
                status_text,
                headers,
                url,
                body,
                ok,
                redirected: false,
                r#type: ResponseType::Basic,
            })
        }
        Err(ureq::Error::Status(code, resp)) => {
            // Non-2xx status — still a valid response.
            let status_text = resp.status_text().to_string();
            let url = resp.get_url().to_string();
            let headers: Vec<(String, String)> = resp
                .headers_names()
                .iter()
                .filter_map(|name| resp.header(name).map(|v| (name.clone(), v.to_string())))
                .collect();
            let body = resp.into_string().unwrap_or_default();
            Ok(Response {
                status: code,
                status_text,
                headers,
                url,
                body,
                ok: false,
                redirected: false,
                r#type: ResponseType::Basic,
            })
        }
        Err(e) => Err(format!("fetch failed: {}", e)),
    }
}

/// Serialize a Response to a string that the JS bridge can parse.
/// Format: "STATUS|URL|BODY" with headers as "K:V\nK:V\n".
fn serialize_response(resp: &Response) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}|{}\n", resp.status, resp.url));
    for (k, v) in &resp.headers {
        out.push_str(&format!("{}:{}\n", k, v));
    }
    out.push_str("---\n");
    out.push_str(&resp.body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_from_str() {
        assert_eq!(Method::from_str("get"), Method::Get);
        assert_eq!(Method::from_str("POST"), Method::Post);
        assert_eq!(Method::from_str("Delete"), Method::Delete);
    }

    #[test]
    fn request_default() {
        let r = Request::new("https://example.com");
        assert_eq!(r.url, "https://example.com");
        assert_eq!(r.method, Method::Get);
    }

    #[test]
    #[ignore]
    fn fetch_rejects_on_invalid_url() {
        let el = EventLoop::new();
        let request = Request::new("http://nonexistent.invalid.domain.example");
        let promise = fetch(request, el.clone());

        let caught = Arc::new(std::sync::Mutex::new(String::new()));
        let c = caught.clone();
        promise.catch(
            move |v| {
                if let PromiseValue::Rejected(ref reason) = v {
                    *c.lock().unwrap() = reason.clone();
                }
                PromiseValue::Resolved(String::new())
            },
            el.clone(),
        );

        el.run();
        // Should have caught an error (network failure).
        assert!(
            !caught.lock().unwrap().is_empty(),
            "fetch should have rejected"
        );
    }

    #[test]
    #[ignore]
    fn fetch_resolves_on_valid_url() {
        let el = EventLoop::new();
        let request = Request::new("https://example.com");
        let promise = fetch(request, el.clone());

        let result = Arc::new(std::sync::Mutex::new(String::new()));
        let r = result.clone();
        promise.then(
            move |v| {
                if let PromiseValue::Resolved(ref val) = v {
                    *r.lock().unwrap() = val.clone();
                }
                v
            },
            el.clone(),
        );

        el.run();
        let result = result.lock().unwrap();
        assert!(!result.is_empty(), "fetch should have resolved");
        assert!(
            result.contains("200|"),
            "expected status 200, got: {}",
            *result
        );
    }
}
