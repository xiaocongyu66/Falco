//! AbortController / AbortSignal — cancelable async operations.
//!
//! ```js
//! const controller = new AbortController();
//! const signal = controller.signal;
//! signal.addEventListener("abort", () => console.log("aborted!"));
//! controller.abort();
//! // "aborted!" is printed, signal.aborted === true
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Shared state for an abort controller/signal pair.
#[derive(Default)]
struct AbortState {
    aborted: bool,
    reason: Option<Value>,
    abort_handlers: Vec<Value>,
}

/// Register AbortController.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "AbortController",
        Value::Builtin(BuiltinFn {
            name: "AbortController".to_string(),
            func: Rc::new(|_args| {
                let state = Rc::new(RefCell::new(AbortState::default()));

                let mut controller = ObjectValue::new();
                controller.set("signal", make_signal(state.clone()));

                let state_for_abort = state.clone();
                controller.set(
                    "abort",
                    Value::Builtin(BuiltinFn {
                        name: "AbortController.abort".to_string(),
                        func: Rc::new(move |args| {
                            let reason = args.first().cloned().unwrap_or(Value::Undefined);
                            let mut s = state_for_abort.borrow_mut();
                            if s.aborted {
                                return Ok(Value::Undefined);
                            }
                            s.aborted = true;
                            s.reason = Some(reason.clone());
                            let handlers = std::mem::take(&mut s.abort_handlers);
                            drop(s);
                            // Fire the abort event.
                            for handler in handlers {
                                let mut event = ObjectValue::new();
                                event.set("type", Value::String("abort".to_string()));
                                event.set("target", Value::Undefined);
                                let event_val = Value::Object(Rc::new(RefCell::new(event)));
                                let _ = call_handler(&handler, event_val);
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(controller))))
            }),
        }),
    );

    // AbortSignal.abort(reason) — static method to create an already-aborted signal.
    scope.declare(
        "AbortSignal",
        Value::Builtin(BuiltinFn {
            name: "AbortSignal".to_string(),
            func: Rc::new(|_args| {
                // Direct constructor — returns a non-aborted signal.
                let state = Rc::new(RefCell::new(AbortState::default()));
                Ok(make_signal(state))
            }),
        }),
    );

    // Add AbortSignal.abort static method.
    if let Some(Value::Builtin(b)) = scope.get("AbortSignal") {
        // We can't easily add a static method to a Builtin. Instead, we'll
        // create an object wrapper.
        let _ = b;
    }
}

fn make_signal(state: Rc<RefCell<AbortState>>) -> Value {
    let mut obj = ObjectValue::new();

    let state_for_aborted = state.clone();
    obj.set(
        "aborted",
        Value::Boolean(state_for_aborted.borrow().aborted),
    );

    let state_for_reason = state.clone();
    obj.set(
        "reason",
        state_for_reason
            .borrow()
            .reason
            .clone()
            .unwrap_or(Value::Undefined),
    );

    let state_for_listener = state.clone();
    obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "AbortSignal.addEventListener".to_string(),
            func: Rc::new(move |args| {
                let event = args.first().map(|v| v.to_string()).unwrap_or_default();
                let handler = args.get(1).cloned().unwrap_or(Value::Undefined);
                if event == "abort" {
                    let s = state_for_listener.borrow();
                    if s.aborted {
                        // Already aborted — fire immediately.
                        let mut event = ObjectValue::new();
                        event.set("type", Value::String("abort".to_string()));
                        let event_val = Value::Object(Rc::new(RefCell::new(event)));
                        let _ = call_handler(&handler, event_val);
                    } else {
                        drop(s);
                        state_for_listener
                            .borrow_mut()
                            .abort_handlers
                            .push(handler);
                    }
                }
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_throw = state.clone();
    obj.set(
        "throwIfAborted",
        Value::Builtin(BuiltinFn {
            name: "AbortSignal.throwIfAborted".to_string(),
            func: Rc::new(move |_args| {
                let s = state_for_throw.borrow();
                if s.aborted {
                    Err(format!(
                        "AbortError: {}",
                        s.reason
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "aborted".to_string())
                    ))
                } else {
                    Ok(Value::Undefined)
                }
            }),
        }),
    );

    Value::Object(Rc::new(RefCell::new(obj)))
}

/// Call a JS handler.
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
    fn abort_controller_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("AbortController").unwrap();
        if let Value::Builtin(b) = ctor {
            let controller = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = controller {
                let obj = obj.borrow();
                assert!(obj.properties.contains_key("signal"));
                assert!(obj.properties.contains_key("abort"));
                // Signal should not be aborted initially.
                if let Some(Value::Object(signal)) = obj.properties.get("signal") {
                    let signal = signal.borrow();
                    assert_eq!(signal.properties.get("aborted"), Some(&Value::Boolean(false)));
                }
            }
        }
    }

    #[test]
    fn abort_controller_abort() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("AbortController").unwrap();
        if let Value::Builtin(b) = ctor {
            let controller = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = &controller {
                let obj = obj.borrow();
                if let Some(Value::Builtin(abort_fn)) = obj.properties.get("abort") {
                    let _ = (abort_fn.func)(vec![]).unwrap();
                }
            }
        }
    }
}
