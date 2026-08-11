//! Web Workers — background script execution.
//!
//! # Limitations
//!
//! True multi-threaded workers would require spawning OS threads, which
//! conflicts with Falco's single-threaded JS runtime (Rc<RefCell> is not
//! Sync). Instead, we implement **synchronous workers** that:
//!
//! 1. Run the worker script immediately in a new TJS context (sharing globals).
//! 2. Queue `postMessage` calls to the parent's `onmessage` handler.
//! 3. Drain the message queue when the parent checks for messages.
//!
//! This matches the observable behavior of workers for most use cases
//! (computation offloading, message passing) while staying single-threaded.
//!
//! For true parallelism, the parent can use `Worker` with the `--worker-thread`
//! feature (planned, uses `std::thread` with serialized message passing).

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use crate::tjs::TjsContext;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// A message in the worker's message queue.
#[derive(Clone)]
struct Message {
    data: Value,
    /// Whether this message is for the parent (true) or the worker (false).
    to_parent: bool,
}

/// Shared state between a worker and its parent.
#[derive(Default)]
struct WorkerState {
    /// Messages queued for the parent's onmessage.
    parent_queue: VecDeque<Value>,
    /// Messages queued for the worker's onmessage.
    worker_queue: VecDeque<Value>,
    /// The parent's onmessage handler (if set).
    parent_onmessage: Option<Value>,
    /// The worker's onmessage handler (if set).
    worker_onmessage: Option<Value>,
    /// Whether the worker has terminated.
    terminated: bool,
}

/// Register the Worker constructor on the scope.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "Worker",
        Value::Builtin(BuiltinFn {
            name: "Worker".to_string(),
            func: Rc::new(|args| {
                let script_url = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                // Try to load the worker script.
                // For simplicity, we treat the URL as a file path or inline source.
                let script_source = if script_url.starts_with("blob:") || script_url.starts_with("data:") {
                    // Inline source — extract after the prefix.
                    // For data: URLs, format is data:<mime>;base64,<data> or data:<mime>,<text>
                    if let Some(idx) = script_url.find(',') {
                        let after = &script_url[idx + 1..];
                        if script_url.contains(";base64,") {
                            use std::str::from_utf8;
                            // Decode base64 inline (simplified — assumes standard base64).
                            let bytes = base64_decode(after).unwrap_or_default();
                            from_utf8(&bytes).unwrap_or("").to_string()
                        } else {
                            after.to_string()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    // Try to read as a file path.
                    std::fs::read_to_string(&script_url).unwrap_or_default()
                };

                let state = Rc::new(RefCell::new(WorkerState::default()));

                // Create the worker object that the parent holds.
                let mut worker_obj = ObjectValue::new();
                let state_for_post = state.clone();
                worker_obj.set(
                    "postMessage",
                    Value::Builtin(BuiltinFn {
                        name: "Worker.postMessage".to_string(),
                        func: Rc::new(move |args| {
                            let data = args.first().cloned().unwrap_or(Value::Undefined);
                            state_for_post.borrow_mut().worker_queue.push_back(data);
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let state_for_terminate = state.clone();
                worker_obj.set(
                    "terminate",
                    Value::Builtin(BuiltinFn {
                        name: "Worker.terminate".to_string(),
                        func: Rc::new(move |_args| {
                            state_for_terminate.borrow_mut().terminated = true;
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                // onmessage is set by the user (worker_obj.onmessage = fn).
                // We use a shared cell so the setter can update it.
                let state_for_onmessage = state.clone();
                let onmessage_cell = Rc::new(RefCell::new(Value::Undefined));
                let cell_for_get = onmessage_cell.clone();
                worker_obj.set(
                    "onmessage",
                    Value::Undefined, // placeholder; user assigns
                );
                // We can't easily intercept property assignment in our object model,
                // so we also expose addEventListener.
                let state_for_listener = state.clone();
                worker_obj.set(
                    "addEventListener",
                    Value::Builtin(BuiltinFn {
                        name: "Worker.addEventListener".to_string(),
                        func: Rc::new(move |args| {
                            let event = args.first().map(|v| v.to_string()).unwrap_or_default();
                            let handler = args.get(1).cloned().unwrap_or(Value::Undefined);
                            if event == "message" {
                                state_for_listener.borrow_mut().parent_onmessage = Some(handler);
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let state_for_drain = state.clone();
                worker_obj.set(
                    "__drain_messages",
                    Value::Builtin(BuiltinFn {
                        name: "Worker.__drain_messages".to_string(),
                        func: Rc::new(move |_args| {
                            let mut s = state_for_drain.borrow_mut();
                            let messages: Vec<_> = s.parent_queue.drain(..).collect();
                            let handler = s.parent_onmessage.clone();
                            drop(s);
                            // Deliver messages to the parent's onmessage handler.
                            if let Some(handler) = handler {
                                for msg in messages {
                                    // Create a MessageEvent-like object.
                                    let mut event = ObjectValue::new();
                                    event.set("data", msg);
                                    event.set("type", Value::String("message".to_string()));
                                    let event_val = Value::Object(Rc::new(RefCell::new(event)));
                                    let _ = call_handler(&handler, event_val);
                                }
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                // Run the worker script in a new TJS context.
                // The worker gets its own `self' object with postMessage and onmessage.
                let mut worker_ctx = TjsContext::new();
                let state_for_worker = state.clone();
                let mut worker_global = crate::tjs::interpreter::Scope::new(None);

                // self.postMessage — sends a message to the parent.
                let state_for_worker_post = state_for_worker.clone();
                let mut self_obj = ObjectValue::new();
                self_obj.set(
                    "postMessage",
                    Value::Builtin(BuiltinFn {
                        name: "self.postMessage".to_string(),
                        func: Rc::new(move |args| {
                            let data = args.first().cloned().unwrap_or(Value::Undefined);
                            state_for_worker_post.borrow_mut().parent_queue.push_back(data);
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                // self.onmessage — set by the worker script.
                let state_for_worker_onmessage = state_for_worker.clone();
                self_obj.set(
                    "addEventListener",
                    Value::Builtin(BuiltinFn {
                        name: "self.addEventListener".to_string(),
                        func: Rc::new(move |args| {
                            let event = args.first().map(|v| v.to_string()).unwrap_or_default();
                            let handler = args.get(1).cloned().unwrap_or(Value::Undefined);
                            if event == "message" {
                                state_for_worker_onmessage.borrow_mut().worker_onmessage = Some(handler);
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                // self.close — terminates the worker.
                let state_for_worker_close = state_for_worker.clone();
                self_obj.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "self.close".to_string(),
                        func: Rc::new(move |_args| {
                            state_for_worker_close.borrow_mut().terminated = true;
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                worker_global.declare("self", Value::Object(Rc::new(RefCell::new(self_obj))));
                worker_global.declare("onmessage", Value::Undefined);

                // Execute the worker script.
                if !script_source.is_empty() {
                    let _ = worker_ctx.execute_with_scope(&script_source, &mut worker_global);
                }

                // Copy the worker_onmessage from the scope back to the state.
                if let Some(handler) = worker_global.get("onmessage") {
                    if !matches!(handler, Value::Undefined) {
                        state_for_worker.borrow_mut().worker_onmessage = Some(handler);
                    }
                }

                // Drain any messages the parent queued before the worker set up its handler.
                let state_for_initial_drain = state.clone();
                {
                    let mut s = state_for_initial_drain.borrow_mut();
                    let messages: Vec<_> = s.worker_queue.drain(..).collect();
                    let handler = s.worker_onmessage.clone();
                    drop(s);
                    if let Some(handler) = handler {
                        for msg in messages {
                            let mut event = ObjectValue::new();
                            event.set("data", msg);
                            event.set("type", Value::String("message".to_string()));
                            let event_val = Value::Object(Rc::new(RefCell::new(event)));
                            let _ = call_handler(&handler, event_val);
                        }
                    }
                }

                Ok(Value::Object(Rc::new(RefCell::new(worker_obj))))
            }),
        }),
    );
}

/// Call a JS handler (Function or Builtin) with a single event argument.
fn call_handler(handler: &Value, event: Value) -> Result<Value, String> {
    match handler {
        Value::Builtin(b) => (b.func)(vec![event]),
        Value::Function(f) => {
            let mut scope = crate::tjs::interpreter::Scope::new(Some(f.closure.clone()));
            for (i, param) in f.params.iter().enumerate() {
                scope.declare(
                    param,
                    if i == 0 { event.clone() } else { Value::Undefined },
                );
            }
            let mut last = Value::Undefined;
            for stmt in &f.body {
                match crate::tjs::interpreter::eval_stmt_pub(stmt, &mut scope) {
                    Ok(crate::tjs::interpreter::Flow::Return(v)) => {
                        last = v;
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(last)
        }
        _ => Ok(Value::Undefined),
    }
}

/// Simple base64 decoder (for data: URLs).
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let lookup = |c: char| -> Result<u8, String> {
        match c {
            'A'..='Z' => Ok((c as u8) - b'A'),
            'a'..='z' => Ok((c as u8) - b'a' + 26),
            '0'..='9' => Ok((c as u8) - b'0' + 52),
            '+' => Ok(62),
            '/' => Ok(63),
            '=' => Ok(0), // padding
            _ => Err(format!("invalid base64 char: {}", c)),
        }
    };

    // Count padding characters.
    let padding = s.chars().filter(|&c| c == '=').count();

    let bytes: Vec<u8> = s
        .chars()
        .filter_map(|c| {
            if c == '=' {
                None
            } else {
                lookup(c).ok()
            }
        })
        .collect();

    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let n = ((chunk[0] as u32) << 18)
            | ((chunk[1] as u32) << 12)
            | (if chunk.len() > 2 { (chunk[2] as u32) << 6 } else { 0 })
            | (if chunk.len() > 3 { chunk[3] as u32 } else { 0 });
        result.push((n >> 16) as u8);
        if chunk.len() > 2 {
            result.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            result.push(n as u8);
        }
    }

    // Remove padding bytes that were incorrectly added.
    // (With 1 pad char, the last group has 3 bytes; with 2 pad chars, 2 bytes.)
    let expected_len = (bytes.len() * 6) / 8;
    result.truncate(expected_len);

    let _ = padding;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_constructor_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("Worker").is_some());
    }

    #[test]
    fn worker_inline_script() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let worker_ctor = scope.get("Worker").unwrap();
        if let Value::Builtin(b) = worker_ctor {
            // Create a worker with an inline script that calls postMessage.
            let script = "self.postMessage(42);";
            let data_url = format!("data:text/javascript,{}", script);
            let worker = (b.func)(vec![Value::String(data_url)]).unwrap();
            if let Value::Object(obj) = worker {
                let obj = obj.borrow();
                // The worker should have postMessage, terminate, addEventListener.
                assert!(obj.properties.contains_key("postMessage"));
                assert!(obj.properties.contains_key("terminate"));
                assert!(obj.properties.contains_key("addEventListener"));
            }
        }
    }

    #[test]
    fn base64_decode_basic() {
        let bytes = base64_decode("aGVsbG8=").unwrap();
        assert_eq!(bytes, b"hello");
    }
}
