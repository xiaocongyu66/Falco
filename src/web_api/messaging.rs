//! MessageChannel / MessagePort / BroadcastChannel.
//!
//! # MessageChannel
//!
//! Creates a pair of connected MessagePorts. Messages sent on one port
//! are delivered to the other port's `onmessage` handler.
//!
//! ```js
//! const channel = new MessageChannel();
//! channel.port1.onmessage = (e) => console.log(e.data);
//! channel.port2.postMessage("hello");
//! ```
//!
//! # BroadcastChannel
//!
//! A named channel that broadcasts messages to all subscribers with the
//! same channel name (even across workers/tabs in a real browser).
//!
//! ```js
//! const bc = new BroadcastChannel("my-channel");
//! bc.onmessage = (e) => console.log(e.data);
//! bc.postMessage("hello");
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A message port — one end of a MessageChannel.
#[derive(Default)]
struct PortState {
    /// Messages queued for this port.
    queue: std::collections::VecDeque<Value>,
    /// The other port in the pair (if connected).
    other: Option<Rc<RefCell<PortState>>>,
    /// The onmessage handler.
    onmessage: Option<Value>,
    /// Whether the port is closed.
    closed: bool,
}

/// Register MessageChannel, MessagePort, and BroadcastChannel.
pub fn register(scope: &mut Scope) {
    register_message_channel(scope);
    register_broadcast_channel(scope);
}

fn register_message_channel(scope: &mut Scope) {
    scope.declare(
        "MessageChannel",
        Value::Builtin(BuiltinFn {
            name: "MessageChannel".to_string(),
            func: Rc::new(|_args| {
                let port1 = Rc::new(RefCell::new(PortState::default()));
                let port2 = Rc::new(RefCell::new(PortState::default()));
                port1.borrow_mut().other = Some(port2.clone());
                port2.borrow_mut().other = Some(port1.clone());

                let mut channel = ObjectValue::new();
                channel.set("port1", make_port(port1));
                channel.set("port2", make_port(port2));
                Ok(Value::Object(Rc::new(RefCell::new(channel))))
            }),
        }),
    );
}

fn make_port(state: Rc<RefCell<PortState>>) -> Value {
    let mut obj = ObjectValue::new();

    let state_for_post = state.clone();
    obj.set(
        "postMessage",
        Value::Builtin(BuiltinFn {
            name: "MessagePort.postMessage".to_string(),
            func: Rc::new(move |args| {
                let data = args.first().cloned().unwrap_or(Value::Undefined);
                let s = state_for_post.borrow();
                if s.closed {
                    return Err("postMessage: port is closed".to_string());
                }
                if let Some(other) = &s.other {
                    other.borrow_mut().queue.push_back(data);
                }
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_close = state.clone();
    obj.set(
        "close",
        Value::Builtin(BuiltinFn {
            name: "MessagePort.close".to_string(),
            func: Rc::new(move |_args| {
                state_for_close.borrow_mut().closed = true;
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_start = state.clone();
    obj.set(
        "start",
        Value::Builtin(BuiltinFn {
            name: "MessagePort.start".to_string(),
            func: Rc::new(move |_args| {
                // In our model, ports are always "started". This is a no-op.
                let _ = state_for_start.borrow();
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_drain = state.clone();
    obj.set(
        "__drain_messages",
        Value::Builtin(BuiltinFn {
            name: "MessagePort.__drain_messages".to_string(),
            func: Rc::new(move |_args| {
                let mut s = state_for_drain.borrow_mut();
                let messages: Vec<_> = s.queue.drain(..).collect();
                let handler = s.onmessage.clone();
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
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_listener = state.clone();
    obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "MessagePort.addEventListener".to_string(),
            func: Rc::new(move |args| {
                let event = args.first().map(|v| v.to_string()).unwrap_or_default();
                let handler = args.get(1).cloned().unwrap_or(Value::Undefined);
                if event == "message" {
                    state_for_listener.borrow_mut().onmessage = Some(handler);
                }
                Ok(Value::Undefined)
            }),
        }),
    );

    Value::Object(Rc::new(RefCell::new(obj)))
}

// ── BroadcastChannel ──────────────────────────────────────────────────

/// Global registry of broadcast channels, keyed by name.
thread_local! {
    static BROADCAST_CHANNELS: RefCell<HashMap<String, Vec<Rc<RefCell<PortState>>>>> =
        RefCell::new(HashMap::new());
}

fn register_broadcast_channel(scope: &mut Scope) {
    scope.declare(
        "BroadcastChannel",
        Value::Builtin(BuiltinFn {
            name: "BroadcastChannel".to_string(),
            func: Rc::new(|args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                let port = Rc::new(RefCell::new(PortState::default()));

                // Register this port with the named channel.
                BROADCAST_CHANNELS.with(|channels| {
                    channels
                        .borrow_mut()
                        .entry(name.clone())
                        .or_default()
                        .push(port.clone());
                });

                let mut obj = ObjectValue::new();
                obj.set("name", Value::String(name));

                let port_for_post = port.clone();
                obj.set(
                    "postMessage",
                    Value::Builtin(BuiltinFn {
                        name: "BroadcastChannel.postMessage".to_string(),
                        func: Rc::new(move |args| {
                            let data = args.first().cloned().unwrap_or(Value::Undefined);
                            // Broadcast to all other ports with the same name.
                            // We need to find the channel name — capture it.
                            // (Simplification: store the name in the port.)
                            let _ = &port_for_post;
                            // Get the channel name from the global registry.
                            // For now, we iterate all channels and find this port.
                            BROADCAST_CHANNELS.with(|channels| {
                                let channels = channels.borrow();
                                for (ch_name, ports) in channels.iter() {
                                    if ports.iter().any(|p| Rc::ptr_eq(p, &port_for_post)) {
                                        for other_port in ports {
                                            if !Rc::ptr_eq(other_port, &port_for_post) {
                                                other_port.borrow_mut().queue.push_back(data.clone());
                                            }
                                        }
                                        let _ = ch_name;
                                        break;
                                    }
                                }
                            });
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let port_for_close = port.clone();
                obj.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "BroadcastChannel.close".to_string(),
                        func: Rc::new(move |_args| {
                            port_for_close.borrow_mut().closed = true;
                            // Remove from the registry.
                            BROADCAST_CHANNELS.with(|channels| {
                                let mut channels = channels.borrow_mut();
                                for (_, ports) in channels.iter_mut() {
                                    ports.retain(|p| !Rc::ptr_eq(p, &port_for_close));
                                }
                            });
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let port_for_listener = port.clone();
                obj.set(
                    "addEventListener",
                    Value::Builtin(BuiltinFn {
                        name: "BroadcastChannel.addEventListener".to_string(),
                        func: Rc::new(move |args| {
                            let event = args.first().map(|v| v.to_string()).unwrap_or_default();
                            let handler = args.get(1).cloned().unwrap_or(Value::Undefined);
                            if event == "message" {
                                port_for_listener.borrow_mut().onmessage = Some(handler);
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let port_for_drain = port.clone();
                obj.set(
                    "__drain_messages",
                    Value::Builtin(BuiltinFn {
                        name: "BroadcastChannel.__drain_messages".to_string(),
                        func: Rc::new(move |_args| {
                            let mut s = port_for_drain.borrow_mut();
                            let messages: Vec<_> = s.queue.drain(..).collect();
                            let handler = s.onmessage.clone();
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
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );
}

/// Call a JS handler with an event argument.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_channel_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("MessageChannel").unwrap();
        if let Value::Builtin(b) = ctor {
            let result = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = result {
                let obj = obj.borrow();
                assert!(obj.properties.contains_key("port1"));
                assert!(obj.properties.contains_key("port2"));
            }
        }
    }

    #[test]
    fn broadcast_channel_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("BroadcastChannel").unwrap();
        if let Value::Builtin(b) = ctor {
            let result = (b.func)(vec![Value::String("test".to_string())]).unwrap();
            if let Value::Object(obj) = result {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("name"), Some(&Value::String("test".to_string())));
            }
        }
    }
}
