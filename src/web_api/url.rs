//! URLSearchParams — parse and manipulate URL query strings.
//!
//! ```js
//! const params = new URLSearchParams("a=1&b=2");
//! params.get("a");        // "1"
//! params.set("c", "3");
//! params.toString();      // "a=1&b=2&c=3"
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register URLSearchParams.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "URLSearchParams",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams".to_string(),
            func: Rc::new(|args| {
                let init = args.first().cloned().unwrap_or(Value::Undefined);
                let pairs = parse_init(&init);
                let state = Rc::new(RefCell::new(pairs));
                Ok(make_search_params(state))
            }),
        }),
    );
}

/// The internal state: a list of (key, value) pairs.
type Params = Vec<(String, String)>;

fn make_search_params(state: Rc<RefCell<Params>>) -> Value {
    let mut obj = ObjectValue::new();

    let state_for_get = state.clone();
    obj.set(
        "get",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.get".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                let s = state_for_get.borrow();
                Ok(s.iter()
                    .find(|(k, _)| k == &key)
                    .map(|(_, v)| Value::String(v.clone()))
                    .unwrap_or(Value::Null))
            }),
        }),
    );

    let state_for_get_all = state.clone();
    obj.set(
        "getAll",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.getAll".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                let s = state_for_get_all.borrow();
                let values: Vec<Value> = s
                    .iter()
                    .filter(|(k, _)| k == &key)
                    .map(|(_, v)| Value::String(v.clone()))
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(values))))
            }),
        }),
    );

    let state_for_has = state.clone();
    obj.set(
        "has",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.has".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                let s = state_for_has.borrow();
                Ok(Value::Boolean(s.iter().any(|(k, _)| k == &key)))
            }),
        }),
    );

    let state_for_set = state.clone();
    obj.set(
        "set",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.set".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                let value = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                let mut s = state_for_set.borrow_mut();
                // Remove existing entries with the same key.
                s.retain(|(k, _)| k != &key);
                s.push((key, value));
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_append = state.clone();
    obj.set(
        "append",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.append".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                let value = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                state_for_append.borrow_mut().push((key, value));
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_delete = state.clone();
    obj.set(
        "delete",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.delete".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().map(|v| v.to_string()).unwrap_or_default();
                state_for_delete.borrow_mut().retain(|(k, _)| k != &key);
                Ok(Value::Undefined)
            }),
        }),
    );

    let state_for_to_string = state.clone();
    obj.set(
        "toString",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.toString".to_string(),
            func: Rc::new(move |_args| {
                let s = state_for_to_string.borrow();
                let pairs: Vec<String> = s
                    .iter()
                    .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
                    .collect();
                Ok(Value::String(pairs.join("&")))
            }),
        }),
    );

    let state_for_entries = state.clone();
    obj.set(
        "entries",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.entries".to_string(),
            func: Rc::new(move |_args| {
                let s = state_for_entries.borrow();
                let entries: Vec<Value> = s
                    .iter()
                    .map(|(k, v)| {
                        Value::Array(Rc::new(RefCell::new(vec![
                            Value::String(k.clone()),
                            Value::String(v.clone()),
                        ])))
                    })
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(entries))))
            }),
        }),
    );

    let state_for_keys = state.clone();
    obj.set(
        "keys",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.keys".to_string(),
            func: Rc::new(move |_args| {
                let s = state_for_keys.borrow();
                let keys: Vec<Value> =
                    s.iter().map(|(k, _)| Value::String(k.clone())).collect();
                Ok(Value::Array(Rc::new(RefCell::new(keys))))
            }),
        }),
    );

    let state_for_values = state.clone();
    obj.set(
        "values",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.values".to_string(),
            func: Rc::new(move |_args| {
                let s = state_for_values.borrow();
                let values: Vec<Value> =
                    s.iter().map(|(_, v)| Value::String(v.clone())).collect();
                Ok(Value::Array(Rc::new(RefCell::new(values))))
            }),
        }),
    );

    let state_for_for_each = state.clone();
    obj.set(
        "forEach",
        Value::Builtin(BuiltinFn {
            name: "URLSearchParams.forEach".to_string(),
            func: Rc::new(move |args| {
                let callback = args.first().cloned().unwrap_or(Value::Undefined);
                let s = state_for_for_each.borrow();
                for (k, v) in s.iter() {
                    let _ = call_callback(
                        &callback,
                        vec![
                            Value::String(v.clone()),
                            Value::String(k.clone()),
                        ],
                    );
                }
                Ok(Value::Undefined)
            }),
        }),
    );

    Value::Object(Rc::new(RefCell::new(obj)))
}

/// Parse the constructor argument into a list of (key, value) pairs.
fn parse_init(init: &Value) -> Params {
    match init {
        Value::String(s) => parse_query_string(s),
        Value::Object(obj) => {
            // Record-like: { key: "value", ... }
            let obj = obj.borrow();
            obj.properties
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect()
        }
        Value::Array(arr) => {
            // Array of [key, value] pairs.
            let arr = arr.borrow();
            arr.iter()
                .filter_map(|v| {
                    if let Value::Array(pair) = v {
                        let pair = pair.borrow();
                        Some((
                            pair.first().map(|v| v.to_string()).unwrap_or_default(),
                            pair.get(1).map(|v| v.to_string()).unwrap_or_default(),
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Parse a query string like "a=1&b=2" into pairs.
fn parse_query_string(s: &str) -> Params {
    if s.is_empty() {
        return Vec::new();
    }
    let s = s.strip_prefix('?').unwrap_or(s);
    s.split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = url_decode(parts.next().unwrap_or(""));
            let value = url_decode(parts.next().unwrap_or(""));
            Some((key, value))
        })
        .collect()
}

/// URL-encode a string (application/x-www-form-urlencoded).
fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push('+'),
            _ => result.push_str(&format!("%{:02X}", b)),
        }
    }
    result
}

/// URL-decode a string.
fn url_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                result.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &bytes[i + 1..i + 3];
                if let Some(byte) = u8::from_str_radix(
                    &String::from_utf8_lossy(hex),
                    16,
                )
                .ok()
                {
                    result.push(byte);
                }
                i += 3;
            }
            _ => {
                result.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Call a JS callback.
fn call_callback(callback: &Value, args: Vec<Value>) -> Result<Value, String> {
    match callback {
        Value::Builtin(b) => (b.func)(args),
        Value::Function(f) => {
            let mut scope = crate::tjs::interpreter::Scope::new(Some(f.closure.clone()));
            for (i, param) in f.params.iter().enumerate() {
                scope.declare(
                    param,
                    args.get(i).cloned().unwrap_or(Value::Undefined),
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
    fn parse_basic() {
        let params = parse_query_string("a=1&b=2&c=3");
        assert_eq!(params.len(), 3);
        assert_eq!(params[0], ("a".to_string(), "1".to_string()));
        assert_eq!(params[1], ("b".to_string(), "2".to_string()));
        assert_eq!(params[2], ("c".to_string(), "3".to_string()));
    }

    #[test]
    fn parse_with_question_mark() {
        let params = parse_query_string("?a=1");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "a");
    }

    #[test]
    fn parse_empty_value() {
        let params = parse_query_string("a=&b=2");
        assert_eq!(params[0].1, "");
        assert_eq!(params[1].1, "2");
    }

    #[test]
    fn url_encode_basic() {
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("a=b"), "a%3Db");
        assert_eq!(url_encode("a+b"), "a%2Bb");
    }

    #[test]
    fn url_decode_basic() {
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("a%3Db"), "a=b");
        assert_eq!(url_decode("a%2Bb"), "a+b");
    }

    #[test]
    fn url_encode_decode_round_trip() {
        let inputs = vec!["hello", "hello world", "a=b&c=d", "100% pure", "été"];
        for input in inputs {
            let encoded = url_encode(input);
            let decoded = url_decode(&encoded);
            assert_eq!(decoded, input, "round-trip failed for: {}", input);
        }
    }

    #[test]
    fn url_search_params_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("URLSearchParams").is_some());
    }

    #[test]
    fn url_search_params_get_set() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("URLSearchParams").unwrap();
        if let Value::Builtin(b) = ctor {
            let params = (b.func)(vec![Value::String("a=1&b=2".to_string())]).unwrap();
            if let Value::Object(obj) = params {
                let obj = obj.borrow();
                // get("a") should return "1"
                if let Some(Value::Builtin(get_fn)) = obj.properties.get("get") {
                    let result = (get_fn.func)(vec![Value::String("a".to_string())]).unwrap();
                    assert_eq!(result, Value::String("1".to_string()));
                }
                // has("b") should return true
                if let Some(Value::Builtin(has_fn)) = obj.properties.get("has") {
                    let result = (has_fn.func)(vec![Value::String("b".to_string())]).unwrap();
                    assert_eq!(result, Value::Boolean(true));
                }
                // has("c") should return false
                if let Some(Value::Builtin(has_fn)) = obj.properties.get("has") {
                    let result = (has_fn.func)(vec![Value::String("c".to_string())]).unwrap();
                    assert_eq!(result, Value::Boolean(false));
                }
            }
        }
    }
}
