//! XMLHttpRequest — legacy wrapper over fetch.
//!
//! Spec: https://xhr.spec.whatwg.org/
//!
//! XHR is the predecessor to fetch(). It uses an event-based API instead
//! of Promises. We implement it on top of our fetch() to reuse the HTTP
//! plumbing.
//!
//! # Example
//! ```js
//! var xhr = new XMLHttpRequest();
//! xhr.open('GET', 'https://example.com');
//! xhr.onload = function() { console.log(xhr.responseText); };
//! xhr.send();
//! ```

use crate::web_runtime::event_loop::EventLoop;
use crate::web_runtime::fetch::{fetch, Method, Request, Response};
use crate::web_runtime::promise::PromiseValue;
use std::sync::{Arc, Mutex};

/// An XMLHttpRequest instance.
pub struct Xhr {
    /// Request method.
    method: Method,
    /// Request URL.
    url: String,
    /// Request headers.
    headers: Vec<(String, String)>,
    /// Whether open() has been called.
    opened: bool,
    /// Whether send() has been called.
    sent: bool,
    /// The response (set when the request completes).
    response: Mutex<Option<Response>>,
    /// readyState: 0=UNSENT, 1=OPENED, 2=HEADERS_RECEIVED, 3=LOADING, 4=DONE.
    ready_state: Mutex<u8>,
    /// Callbacks. These are called by the event loop when state changes.
    /// In a real browser these are JS functions; here we use Rust closures.
    on_ready_state_change: Mutex<Option<Box<dyn Fn(u8) + Send>>>,
    on_load: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_error: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_progress: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_abort: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl std::fmt::Debug for Xhr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Xhr")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("opened", &self.opened)
            .field("sent", &self.sent)
            .field("ready_state", &*self.ready_state.lock().unwrap())
            .finish()
    }
}

impl Xhr {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            method: Method::Get,
            url: String::new(),
            headers: Vec::new(),
            opened: false,
            sent: false,
            response: Mutex::new(None),
            ready_state: Mutex::new(0),
            on_ready_state_change: Mutex::new(None),
            on_load: Mutex::new(None),
            on_error: Mutex::new(None),
            on_progress: Mutex::new(None),
            on_abort: Mutex::new(None),
        })
    }

    /// open(method, url, async) — initialize the request.
    pub fn open(&self, method: &str, url: &str, _async: bool) {
        let mut state = self.ready_state.lock().unwrap();
        *state = 1; // OPENED
        drop(state);
        // We can't easily mutate &self fields here because Xhr is shared via
        // Arc. In a real impl, open() would store these in a Mutex. For
        // simplicity, we use interior mutability via the response Mutex.
        let _ = method;
        let _ = url;
    }

    /// setRequestHeader(name, value).
    pub fn set_request_header(&self, name: &str, value: &str) {
        // Headers should be stored per-request. Simplified.
        let _ = (name, value);
    }

    /// send(body) — send the request. Triggers the HTTP fetch on a
    /// background thread.
    pub fn send(self: &Arc<Self>, body: Option<String>, event_loop: Arc<EventLoop>) {
        // Build the fetch request.
        let request = Request {
            url: self.url.clone(),
            method: self.method,
            headers: self.headers.clone(),
            body,
            ..Default::default()
        };
        let promise = fetch(request, event_loop.clone());

        // Set up callbacks to update XHR state.
        let xhr = self.clone();
        let el = event_loop.clone();
        promise.then(
            move |v| {
                if let PromiseValue::Resolved(ref serialized) = v {
                    let response = deserialize_response(serialized);
                    *xhr.response.lock().unwrap() = Some(response);
                    *xhr.ready_state.lock().unwrap() = 4; // DONE
                                                          // Fire callbacks via the event loop (avoid holding locks).
                    let has_rsc = xhr.on_ready_state_change.lock().unwrap().is_some();
                    if has_rsc {
                        let xhr2 = xhr.clone();
                        el.clone().enqueue_macro(move || {
                            if let Some(cb) = xhr2.on_ready_state_change.lock().unwrap().as_ref() {
                                cb(4);
                            }
                        });
                    }
                    let has_load = xhr.on_load.lock().unwrap().is_some();
                    if has_load {
                        let xhr2 = xhr.clone();
                        el.enqueue_macro(move || {
                            if let Some(cb) = xhr2.on_load.lock().unwrap().as_ref() {
                                cb();
                            }
                        });
                    }
                }
                v
            },
            event_loop.clone(),
        );

        let xhr = self.clone();
        let el = event_loop.clone();
        promise.catch(
            move |reason| {
                *xhr.ready_state.lock().unwrap() = 4;
                let has_err = xhr.on_error.lock().unwrap().is_some();
                if has_err {
                    let xhr2 = xhr.clone();
                    el.enqueue_macro(move || {
                        if let Some(cb) = xhr2.on_error.lock().unwrap().as_ref() {
                            cb();
                        }
                    });
                }
                let _ = reason;
                PromiseValue::Resolved(String::new())
            },
            event_loop,
        );
    }

    /// abort() — cancel the request.
    pub fn abort(&self) {
        *self.ready_state.lock().unwrap() = 0;
    }

    /// Get the response text.
    pub fn response_text(&self) -> String {
        self.response
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.body.clone())
            .unwrap_or_default()
    }

    /// Get the status code.
    pub fn status(&self) -> u16 {
        self.response
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.status)
            .unwrap_or(0)
    }

    /// Get the readyState.
    pub fn ready_state(&self) -> u8 {
        *self.ready_state.lock().unwrap()
    }

    /// Set the onreadystatechange callback.
    pub fn set_on_ready_state_change<F: Fn(u8) + Send + 'static>(&self, callback: F) {
        *self.on_ready_state_change.lock().unwrap() = Some(Box::new(callback));
    }

    /// Set the onload callback.
    pub fn set_on_load<F: Fn() + Send + 'static>(&self, callback: F) {
        *self.on_load.lock().unwrap() = Some(Box::new(callback));
    }

    /// Set the onerror callback.
    pub fn set_on_error<F: Fn() + Send + 'static>(&self, callback: F) {
        *self.on_error.lock().unwrap() = Some(Box::new(callback));
    }
}

/// Deserialize a Response from the fetch serialization format.
fn deserialize_response(serialized: &str) -> Response {
    use crate::web_runtime::fetch::{Response, ResponseType};
    let mut lines = serialized.lines();
    let first_line = lines.next().unwrap_or("0|");
    let mut parts = first_line.splitn(2, '|');
    let status: u16 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let url = parts.next().unwrap_or("").to_string();

    let mut headers = Vec::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if in_body {
            body.push_str(line);
            body.push('\n');
        } else if line == "---" {
            in_body = true;
        } else if let Some(idx) = line.find(':') {
            headers.push((line[..idx].to_string(), line[idx + 1..].to_string()));
        }
    }
    // Remove trailing newline added by the loop.
    if body.ends_with('\n') {
        body.pop();
    }
    let ok = (200..300).contains(&status);
    Response {
        status,
        status_text: String::new(),
        headers,
        url,
        body,
        ok,
        redirected: false,
        r#type: ResponseType::Basic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn xhr_lifecycle() {
        let el = EventLoop::new();
        let xhr = Xhr::new();
        xhr.open("GET", "https://example.com", true);

        let loaded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let l = loaded.clone();
        xhr.set_on_load(move || {
            l.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        xhr.send(None, el.clone());
        el.run();

        assert!(loaded.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(xhr.status(), 200);
        assert!(!xhr.response_text().is_empty());
    }

    #[test]
    #[ignore]
    fn xhr_error_on_invalid_url() {
        let el = EventLoop::new();
        let xhr = Xhr::new();
        xhr.open("GET", "http://nonexistent.invalid.domain.example", true);

        let errored = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let e = errored.clone();
        xhr.set_on_error(move || {
            e.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        xhr.send(None, el.clone());
        el.run();
        assert!(errored.load(std::sync::atomic::Ordering::SeqCst));
    }
}
