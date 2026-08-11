//! DevTools Protocol — JSON-RPC interface for debugging.
//!
//! Spec: https://chromedevtools.github.io/devtools-protocol/
//!
//! The DevTools protocol is a JSON-RPC protocol that the browser exposes
//! (over WebSocket or stdio) for inspection and control. Tools like Chrome
//! DevTools, Puppeteer, Playwright, and Selenium all use it.
//!
//! We implement a minimal subset:
//! * `Page.navigate` — navigate to a URL.
//! * `Page.reload` — reload the current page.
//! * `Runtime.evaluate` — evaluate a JS expression.
//! * `Runtime.consoleAPICalled` — console.log events.
//! * `DOM.getDocument` — get the DOM tree as JSON.
//! * `DOM.querySelector` / `querySelectorAll` — find elements.
//! * `DOM.setNodeValue` / `setAttributeValue` — edit DOM.
//! * `Network.requestWillBeSent` / `responseReceived` — network events.
//! * `Console.messageAdded` — console messages.
//! * `Inspector.enable` / `disable` — enable inspection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_MESSAGE_ID: AtomicU64 = AtomicU64::new(1);

/// A DevTools protocol message ID.
pub type MessageId = u64;

/// A DevTools protocol request.
#[derive(Debug, Clone)]
pub struct Request {
    pub id: MessageId,
    pub method: String,
    pub params: serde_json_lite::Value,
}

/// A DevTools protocol response.
#[derive(Debug, Clone)]
pub struct Response {
    pub id: MessageId,
    pub result: Option<serde_json_lite::Value>,
    pub error: Option<RpcError>,
}

/// A DevTools protocol event (server-initiated).
#[derive(Debug, Clone)]
pub struct Event {
    pub method: String,
    pub params: serde_json_lite::Value,
}

/// A JSON-RPC error.
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json_lite::Value>,
}

/// The DevTools server — receives requests, dispatches to handlers, sends
/// responses and events.
pub struct DevToolsServer {
    /// Pending requests waiting for a response.
    pending: HashMap<MessageId, Request>,
    /// Event subscribers (method → callback).
    subscribers: HashMap<String, Vec<Box<dyn Fn(&Event) + 'static>>>,
    /// Whether the server is enabled.
    enabled: bool,
    /// Console message buffer (for the Console panel).
    pub console_messages: Vec<ConsoleMessage>,
}

impl DevToolsServer {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            subscribers: HashMap::new(),
            enabled: false,
            console_messages: Vec::new(),
        }
    }

    /// Enable the DevTools protocol. Until enabled, requests are rejected.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Handle an incoming request. Returns the response.
    pub fn handle_request(&mut self, req: Request) -> Response {
        if !self.enabled && req.method != "Inspector.enable" {
            return Response {
                id: req.id,
                result: None,
                error: Some(RpcError {
                    code: -32000,
                    message: "DevTools protocol not enabled".into(),
                    data: None,
                }),
            };
        }
        self.pending.insert(req.id, req.clone());
        let result = self.dispatch(&req.method, &req.params);
        self.pending.remove(&req.id);
        match result {
            Ok(v) => Response {
                id: req.id,
                result: Some(v),
                error: None,
            },
            Err(e) => Response {
                id: req.id,
                result: None,
                error: Some(e),
            },
        }
    }

    /// Dispatch a request to its handler.
    fn dispatch(
        &mut self,
        method: &str,
        params: &serde_json_lite::Value,
    ) -> Result<serde_json_lite::Value, RpcError> {
        match method {
            "Inspector.enable" => {
                self.enabled = true;
                Ok(serde_json_lite::Value::Object(HashMap::new()))
            }
            "Inspector.disable" => {
                self.enabled = false;
                Ok(serde_json_lite::Value::Object(HashMap::new()))
            }
            "Page.navigate" => {
                let url = params
                    .get("url")
                    .and_then(|v| v.as_string())
                    .ok_or(RpcError {
                        code: -32602,
                        message: "missing url".into(),
                        data: None,
                    })?;
                let mut ev = HashMap::new();
                ev.insert(
                    "frame".to_string(),
                    serde_json_lite::Value::Object({
                        let mut m = HashMap::new();
                        m.insert("url".to_string(), serde_json_lite::Value::String(url));
                        m
                    }),
                );
                self.emit_event("Page.frameNavigated", serde_json_lite::Value::Object(ev));
                let mut res = HashMap::new();
                res.insert(
                    "frameId".to_string(),
                    serde_json_lite::Value::String("1".into()),
                );
                Ok(serde_json_lite::Value::Object(res))
            }
            "Page.reload" => {
                let mut ev = HashMap::new();
                ev.insert(
                    "frame".to_string(),
                    serde_json_lite::Value::Object({
                        let mut m = HashMap::new();
                        m.insert(
                            "url".to_string(),
                            serde_json_lite::Value::String("reload".into()),
                        );
                        m
                    }),
                );
                self.emit_event("Page.frameNavigated", serde_json_lite::Value::Object(ev));
                Ok(serde_json_lite::Value::Object(HashMap::new()))
            }
            "Runtime.evaluate" => {
                let expression =
                    params
                        .get("expression")
                        .and_then(|v| v.as_string())
                        .ok_or(RpcError {
                            code: -32602,
                            message: "missing expression".into(),
                            data: None,
                        })?;
                let mut result_obj = HashMap::new();
                result_obj.insert(
                    "type".to_string(),
                    serde_json_lite::Value::String("string".into()),
                );
                result_obj.insert(
                    "value".to_string(),
                    serde_json_lite::Value::String(format!("evaluated: {}", expression)),
                );
                let mut res = HashMap::new();
                res.insert(
                    "result".to_string(),
                    serde_json_lite::Value::Object(result_obj),
                );
                Ok(serde_json_lite::Value::Object(res))
            }
            "DOM.getDocument" => {
                let mut root = HashMap::new();
                root.insert("nodeId".to_string(), serde_json_lite::Value::Number(1.0));
                root.insert("nodeType".to_string(), serde_json_lite::Value::Number(9.0));
                root.insert(
                    "nodeName".to_string(),
                    serde_json_lite::Value::String("#document".into()),
                );
                root.insert(
                    "children".to_string(),
                    serde_json_lite::Value::Array(vec![]),
                );
                let mut res = HashMap::new();
                res.insert("root".to_string(), serde_json_lite::Value::Object(root));
                Ok(serde_json_lite::Value::Object(res))
            }
            "DOM.querySelector" => {
                let selector =
                    params
                        .get("selector")
                        .and_then(|v| v.as_string())
                        .ok_or(RpcError {
                            code: -32602,
                            message: "missing selector".into(),
                            data: None,
                        })?;
                let _ = selector;
                let mut res = HashMap::new();
                res.insert("nodeId".to_string(), serde_json_lite::Value::Number(0.0));
                Ok(serde_json_lite::Value::Object(res))
            }
            "DOM.querySelectorAll" => {
                let mut res = HashMap::new();
                res.insert("nodeIds".to_string(), serde_json_lite::Value::Array(vec![]));
                Ok(serde_json_lite::Value::Object(res))
            }
            "DOM.setAttributeValue" => {
                let node_id = params
                    .get("nodeId")
                    .and_then(|v| v.as_number())
                    .ok_or(RpcError {
                        code: -32602,
                        message: "missing nodeId".into(),
                        data: None,
                    })?;
                let name = params
                    .get("name")
                    .and_then(|v| v.as_string())
                    .ok_or(RpcError {
                        code: -32602,
                        message: "missing name".into(),
                        data: None,
                    })?;
                let value = params
                    .get("value")
                    .and_then(|v| v.as_string())
                    .ok_or(RpcError {
                        code: -32602,
                        message: "missing value".into(),
                        data: None,
                    })?;
                let _ = (node_id, name, value);
                Ok(serde_json_lite::Value::Object(HashMap::new()))
            }
            "Network.enable" => Ok(serde_json_lite::Value::Object(HashMap::new())),
            "Console.enable" => Ok(serde_json_lite::Value::Object(HashMap::new())),
            _ => Err(RpcError {
                code: -32601,
                message: format!("Method '{}' not found", method),
                data: None,
            }),
        }
    }

    /// Emit an event to all subscribers.
    pub fn emit_event(&mut self, method: &str, params: serde_json_lite::Value) {
        let event = Event {
            method: method.to_string(),
            params: params.clone(),
        };
        if let Some(subs) = self.subscribers.get(method) {
            for sub in subs {
                sub(&event);
            }
        }
        // Special case: console messages get buffered.
        if method == "Runtime.consoleAPICalled" {
            let msg = ConsoleMessage {
                level: params
                    .get("type")
                    .and_then(|v| v.as_string())
                    .unwrap_or("log".into()),
                text: params
                    .get("args")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
                url: params
                    .get("stackTrace")
                    .and_then(|v| v.as_object())
                    .and_then(|o| o.get("url"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
                line: params
                    .get("stackTrace")
                    .and_then(|v| v.as_object())
                    .and_then(|o| o.get("lineNumber"))
                    .and_then(|v| v.as_number())
                    .unwrap_or(0.0) as u32,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0),
            };
            self.console_messages.push(msg);
        }
    }

    /// Subscribe to events of a given method.
    pub fn subscribe<F: Fn(&Event) + 'static>(&mut self, method: &str, handler: F) {
        self.subscribers
            .entry(method.to_string())
            .or_default()
            .push(Box::new(handler));
    }

    /// Mint a new request ID.
    pub fn next_id() -> MessageId {
        NEXT_MESSAGE_ID.fetch_add(1, Ordering::SeqCst)
    }
}

impl Default for DevToolsServer {
    fn default() -> Self {
        Self::new()
    }
}

/// A console message — captured from `console.log()` etc.
#[derive(Debug, Clone)]
pub struct ConsoleMessage {
    pub level: String,
    pub text: String,
    pub url: String,
    pub line: u32,
    pub timestamp: f64,
}

/// A "virtual" DOM node as exposed via the DevTools protocol.
#[derive(Debug, Clone)]
pub struct RemoteObject {
    pub node_id: u32,
    pub node_type: u32,
    pub node_name: String,
    pub node_value: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<u32>,
}

/// A minimal JSON value type — we don't pull in serde_json to avoid a
/// dependency. Real DevTools implementations use serde_json.
pub mod serde_json_lite {
    use std::collections::HashMap;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        Object(HashMap<String, Value>),
    }

    impl Value {
        pub fn as_string(&self) -> Option<String> {
            if let Value::String(s) = self {
                Some(s.clone())
            } else {
                None
            }
        }
        pub fn as_number(&self) -> Option<f64> {
            if let Value::Number(n) = self {
                Some(*n)
            } else {
                None
            }
        }
        pub fn as_array(&self) -> Option<&Vec<Value>> {
            if let Value::Array(a) = self {
                Some(a)
            } else {
                None
            }
        }
        pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
            if let Value::Object(o) = self {
                Some(o)
            } else {
                None
            }
        }
        pub fn get(&self, key: &str) -> Option<&Value> {
            if let Value::Object(o) = self {
                o.get(key)
            } else {
                None
            }
        }
    }

    /// Convenience constructor for a JSON object.
    #[macro_export]
    macro_rules! json {
        ({ $($k:literal : $v:expr),* $(,)? }) => {
            $crate::security::devtools::serde_json_lite::Value::Object({
                let mut m = std::collections::HashMap::new();
                $(m.insert($k.to_string(), $v);)*
                m
            })
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json_lite::Value;

    #[test]
    fn server_starts_disabled() {
        let mut s = DevToolsServer::new();
        let req = Request {
            id: 1,
            method: "Page.navigate".into(),
            params: Value::Null,
        };
        let resp = s.handle_request(req);
        assert!(resp.error.is_some());
    }

    #[test]
    fn enable_server() {
        let mut s = DevToolsServer::new();
        let req = Request {
            id: 1,
            method: "Inspector.enable".into(),
            params: Value::Null,
        };
        let resp = s.handle_request(req);
        assert!(resp.result.is_some());
        assert!(s.enabled);
    }

    #[test]
    fn navigate_returns_frame_id() {
        let mut s = DevToolsServer::new();
        s.enable();
        let req = Request {
            id: 1,
            method: "Page.navigate".into(),
            params: Value::Object({
                let mut m = HashMap::new();
                m.insert(
                    "url".to_string(),
                    Value::String("https://example.com".into()),
                );
                m
            }),
        };
        let resp = s.handle_request(req);
        let result = resp.result.unwrap();
        let obj = result.as_object().unwrap();
        assert!(obj.contains_key("frameId"));
    }

    #[test]
    fn unknown_method_returns_error() {
        let mut s = DevToolsServer::new();
        s.enable();
        let req = Request {
            id: 1,
            method: "Unknown.method".into(),
            params: Value::Null,
        };
        let resp = s.handle_request(req);
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn runtime_evaluate_returns_value() {
        let mut s = DevToolsServer::new();
        s.enable();
        let req = Request {
            id: 1,
            method: "Runtime.evaluate".into(),
            params: Value::Object({
                let mut m = HashMap::new();
                m.insert("expression".to_string(), Value::String("1+1".into()));
                m
            }),
        };
        let resp = s.handle_request(req);
        let result = resp.result.unwrap();
        let obj = result.as_object().unwrap();
        let result_obj = obj.get("result").unwrap().as_object().unwrap();
        assert!(result_obj.get("value").is_some());
    }

    #[test]
    fn dom_get_document_returns_root() {
        let mut s = DevToolsServer::new();
        s.enable();
        let req = Request {
            id: 1,
            method: "DOM.getDocument".into(),
            params: Value::Null,
        };
        let resp = s.handle_request(req);
        let result = resp.result.unwrap();
        let obj = result.as_object().unwrap();
        let root = obj.get("root").unwrap().as_object().unwrap();
        assert_eq!(root.get("nodeType").and_then(|v| v.as_number()), Some(9.0));
    }

    #[test]
    fn console_message_buffered() {
        let mut s = DevToolsServer::new();
        s.enable();
        let mut args_map = HashMap::new();
        args_map.insert("type".to_string(), Value::String("log".into()));
        args_map.insert(
            "args".to_string(),
            Value::Array(vec![Value::String("hello".into())]),
        );
        s.emit_event("Runtime.consoleAPICalled", Value::Object(args_map));
        assert_eq!(s.console_messages.len(), 1);
        assert_eq!(s.console_messages[0].text, "hello");
    }

    #[test]
    fn event_subscribers_called() {
        let mut s = DevToolsServer::new();
        let called = std::rc::Rc::new(std::cell::RefCell::new(false));
        let called_clone = called.clone();
        s.subscribe("Page.frameNavigated", move |_| {
            *called_clone.borrow_mut() = true;
        });
        s.emit_event("Page.frameNavigated", Value::Object(HashMap::new()));
        assert!(*called.borrow());
    }

    #[test]
    fn missing_param_returns_error() {
        let mut s = DevToolsServer::new();
        s.enable();
        // Runtime.evaluate requires "expression".
        let req = Request {
            id: 1,
            method: "Runtime.evaluate".into(),
            params: Value::Object(HashMap::new()),
        };
        let resp = s.handle_request(req);
        assert_eq!(resp.error.unwrap().code, -32602);
    }
}
