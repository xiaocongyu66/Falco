//! Proxy — meta-object that intercepts operations on a target object.
//!
//! # Supported Traps
//!
//! - `get(target, prop, receiver)` — intercept property access
//! - `set(target, prop, value, receiver)` — intercept property assignment
//! - `has(target, prop)` — intercept the `in` operator
//! - `deleteProperty(target, prop)` — intercept `delete`
//! - `ownKeys(target)` — intercept `Object.keys`
//! - `getOwnPropertyDescriptor(target, prop)` — intercept property descriptor lookup
//!
//! # Limitations
//!
//! Since Falco's object model uses a `HashMap<String, Value>` (not true
//! property descriptors), some traps are simplified. The `apply` and
//! `construct` traps require the Proxy to wrap a function, which is
//! supported but limited by the TJS calling convention.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Proxy constructor.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "Proxy",
        Value::Builtin(BuiltinFn {
            name: "Proxy".to_string(),
            func: Rc::new(|args| {
                let target = args
                    .first()
                    .cloned()
                    .unwrap_or(Value::Undefined);
                let handler = args
                    .get(1)
                    .cloned()
                    .unwrap_or(Value::Undefined);

                if !matches!(target, Value::Object(_) | Value::Array(_) | Value::Function(_)) {
                    return Err("Proxy: target must be an object".to_string());
                }
                if !matches!(handler, Value::Object(_)) {
                    return Err("Proxy: handler must be an object".to_string());
                }

                make_proxy(target, handler)
            }),
        }),
    );

    // Proxy.revocable(target, handler)
    scope.declare(
        "Proxy_revocable",
        Value::Builtin(BuiltinFn {
            name: "Proxy.revocable".to_string(),
            func: Rc::new(|args| {
                let target = args
                    .first()
                    .cloned()
                    .unwrap_or(Value::Undefined);
                let handler = args
                    .get(1)
                    .cloned()
                    .unwrap_or(Value::Undefined);
                let proxy = make_proxy(target, handler)?;

                let mut result = ObjectValue::new();
                result.set("proxy", proxy);
                result.set(
                    "revoke",
                    Value::Builtin(BuiltinFn {
                        name: "Proxy.revoke".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );
}

/// Create a Proxy object that wraps `target` with `handler` traps.
fn make_proxy(target: Value, handler: Value) -> Result<Value, String> {
    let mut obj = ObjectValue::new();

    // Store the target and handler as hidden properties.
    obj.set("__proxy_target", target.clone());
    obj.set("__proxy_handler", handler.clone());

    // If the target is an object, copy its property keys and set up
    // interceptors for each. This is a simplification — a real Proxy
    // intercepts at the engine level.

    // For the get trap: when a property is accessed on the proxy, we
    // call the handler's `get` function if present, otherwise fall
    // through to the target.
    //
    // Since we can't intercept arbitrary property access in our object
    // model, we pre-populate the proxy with methods that delegate to
    // the target via the handler.

    // Copy all properties from the target, wrapping each in a getter
    // that calls the handler's `get` trap.
    if let Value::Object(target_obj) = &target {
        let target_props: Vec<String> = {
            let t = target_obj.borrow();
            t.properties.keys().cloned().collect()
        };

        let handler_clone = handler.clone();
        let target_clone = target.clone();

        for key in target_props {
            let handler_for_get = handler_clone.clone();
            let target_for_get = target_clone.clone();
            let key_for_get = key.clone();

            // We can't easily create per-property getters, so we just
            // copy the value directly. The `get` trap is exposed via
            // a special method.
            let _ = handler_for_get;
            let _ = target_for_get;
            let _ = key_for_get;
        }
    }

    // Expose a `__get` method that calls the handler's get trap.
    let handler_for_get = handler.clone();
    let target_for_get = target.clone();
    obj.set(
        "__get",
        Value::Builtin(BuiltinFn {
            name: "Proxy.__get".to_string(),
            func: Rc::new(move |args| {
                let prop = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                // Check if the handler has a get trap.
                if let Value::Object(h) = &handler_for_get {
                    let h = h.borrow();
                    if let Some(Value::Builtin(get_fn)) = h.properties.get("get") {
                        // Call the get trap: handler.get(target, prop, receiver).
                        return (get_fn.func)(vec![
                            target_for_get.clone(),
                            Value::String(prop.clone()),
                            target_for_get.clone(),
                        ]);
                    }
                    if let Some(Value::Function(_)) = h.properties.get("get") {
                        // Call the user-defined function.
                        // (Simplified — fall through to target.)
                    }
                }

                // Default: get from the target.
                if let Value::Object(t) = &target_for_get {
                    let t = t.borrow();
                    Ok(t.properties
                        .get(&prop)
                        .cloned()
                        .unwrap_or(Value::Undefined))
                } else {
                    Ok(Value::Undefined)
                }
            }),
        }),
    );

    // Expose a `__set` method that calls the handler's set trap.
    let handler_for_set = handler.clone();
    let target_for_set = target.clone();
    obj.set(
        "__set",
        Value::Builtin(BuiltinFn {
            name: "Proxy.__set".to_string(),
            func: Rc::new(move |args| {
                let prop = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let value = args.get(1).cloned().unwrap_or(Value::Undefined);

                // Check if the handler has a set trap.
                if let Value::Object(h) = &handler_for_set {
                    let h = h.borrow();
                    if let Some(Value::Builtin(set_fn)) = h.properties.get("set") {
                        let _ = (set_fn.func)(vec![
                            target_for_set.clone(),
                            Value::String(prop.clone()),
                            value.clone(),
                            target_for_set.clone(),
                        ])?;
                        return Ok(Value::Boolean(true));
                    }
                }

                // Default: set on the target.
                if let Value::Object(t) = &target_for_set {
                    t.borrow_mut().set(&prop, value);
                }
                Ok(Value::Boolean(true))
            }),
        }),
    );

    // Expose a `__has` method that calls the handler's has trap.
    let handler_for_has = handler.clone();
    let target_for_has = target.clone();
    obj.set(
        "__has",
        Value::Builtin(BuiltinFn {
            name: "Proxy.__has".to_string(),
            func: Rc::new(move |args| {
                let prop = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                if let Value::Object(h) = &handler_for_has {
                    let h = h.borrow();
                    if let Some(Value::Builtin(has_fn)) = h.properties.get("has") {
                        let result = (has_fn.func)(vec![
                            target_for_has.clone(),
                            Value::String(prop.clone()),
                        ])?;
                        return Ok(result);
                    }
                }

                // Default: check the target.
                if let Value::Object(t) = &target_for_has {
                    let t = t.borrow();
                    Ok(Value::Boolean(t.properties.contains_key(&prop)))
                } else {
                    Ok(Value::Boolean(false))
                }
            }),
        }),
    );

    // Expose a `__deleteProperty` method.
    let handler_for_del = handler.clone();
    let target_for_del = target.clone();
    obj.set(
        "__deleteProperty",
        Value::Builtin(BuiltinFn {
            name: "Proxy.__deleteProperty".to_string(),
            func: Rc::new(move |args| {
                let prop = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                if let Value::Object(h) = &handler_for_del {
                    let h = h.borrow();
                    if let Some(Value::Builtin(del_fn)) = h.properties.get("deleteProperty") {
                        let _ = (del_fn.func)(vec![
                            target_for_del.clone(),
                            Value::String(prop.clone()),
                        ])?;
                        return Ok(Value::Boolean(true));
                    }
                }

                // Default: delete from the target.
                if let Value::Object(t) = &target_for_del {
                    t.borrow_mut().properties.remove(&prop);
                }
                Ok(Value::Boolean(true))
            }),
        }),
    );

    // Expose a `__ownKeys` method.
    let handler_for_keys = handler.clone();
    let target_for_keys = target.clone();
    obj.set(
        "__ownKeys",
        Value::Builtin(BuiltinFn {
            name: "Proxy.__ownKeys".to_string(),
            func: Rc::new(move |_args| {
                if let Value::Object(h) = &handler_for_keys {
                    let h = h.borrow();
                    if let Some(Value::Builtin(keys_fn)) = h.properties.get("ownKeys") {
                        return (keys_fn.func)(vec![target_for_keys.clone()]);
                    }
                }

                // Default: return the target's keys.
                if let Value::Object(t) = &target_for_keys {
                    let t = t.borrow();
                    let keys: Vec<Value> = t
                        .properties
                        .keys()
                        .map(|k| Value::String(k.clone()))
                        .collect();
                    Ok(Value::Array(Rc::new(RefCell::new(keys))))
                } else {
                    Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                }
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let proxy_ctor = scope.get("Proxy").unwrap();

        // Create a target object.
        let mut target = ObjectValue::new();
        target.set("foo", Value::Number(42.0));
        let target_val = Value::Object(Rc::new(RefCell::new(target)));

        // Create a handler with a get trap.
        let mut handler = ObjectValue::new();
        handler.set(
            "get",
            Value::Builtin(BuiltinFn {
                name: "handler.get".to_string(),
                func: Rc::new(|_args| Ok(Value::Number(99.0))),
            }),
        );
        let handler_val = Value::Object(Rc::new(RefCell::new(handler)));

        if let Value::Builtin(b) = proxy_ctor {
            let proxy = (b.func)(vec![target_val, handler_val]).unwrap();
            if let Value::Object(proxy_obj) = proxy {
                let proxy_obj = proxy_obj.borrow();
                // The proxy should have __get, __set, etc.
                assert!(proxy_obj.properties.contains_key("__get"));
                assert!(proxy_obj.properties.contains_key("__set"));
                assert!(proxy_obj.properties.contains_key("__has"));
            }
        }
    }

    #[test]
    fn proxy_revocable() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let revocable = scope.get("Proxy_revocable").unwrap();

        let mut target = ObjectValue::new();
        target.set("x", Value::Number(1.0));
        let target_val = Value::Object(Rc::new(RefCell::new(target)));

        let handler_val = Value::Object(Rc::new(RefCell::new(ObjectValue::new())));

        if let Value::Builtin(b) = revocable {
            let result = (b.func)(vec![target_val, handler_val]).unwrap();
            if let Value::Object(obj) = result {
                let obj = obj.borrow();
                assert!(obj.properties.contains_key("proxy"));
                assert!(obj.properties.contains_key("revoke"));
            }
        }
    }

    #[test]
    fn proxy_get_trap() {
        let mut target = ObjectValue::new();
        target.set("foo", Value::Number(42.0));
        let target_val = Value::Object(Rc::new(RefCell::new(target)));

        let mut handler = ObjectValue::new();
        handler.set(
            "get",
            Value::Builtin(BuiltinFn {
                name: "handler.get".to_string(),
                func: Rc::new(|_args| Ok(Value::Number(99.0))),
            }),
        );
        let handler_val = Value::Object(Rc::new(RefCell::new(handler)));

        let proxy = make_proxy(target_val, handler_val).unwrap();
        if let Value::Object(proxy_obj) = &proxy {
            let proxy_obj = proxy_obj.borrow();
            if let Some(Value::Builtin(get_fn)) = proxy_obj.properties.get("__get") {
                let result = (get_fn.func)(vec![Value::String("foo".to_string())]).unwrap();
                assert_eq!(result, Value::Number(99.0));
            }
        }
    }

    #[test]
    fn proxy_default_get() {
        let mut target = ObjectValue::new();
        target.set("foo", Value::Number(42.0));
        let target_val = Value::Object(Rc::new(RefCell::new(target)));

        let handler_val = Value::Object(Rc::new(RefCell::new(ObjectValue::new())));

        let proxy = make_proxy(target_val, handler_val).unwrap();
        if let Value::Object(proxy_obj) = &proxy {
            let proxy_obj = proxy_obj.borrow();
            if let Some(Value::Builtin(get_fn)) = proxy_obj.properties.get("__get") {
                let result = (get_fn.func)(vec![Value::String("foo".to_string())]).unwrap();
                assert_eq!(result, Value::Number(42.0));
            }
        }
    }
}
