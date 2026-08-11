//! TJS Value — runtime value types.
//!
//! All TJS values are one of: Number, String, Boolean, Null, Undefined,
//! Object (HashMap), Array (Vec), Function (closure).

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// A TJS runtime value.
#[derive(Clone)]
pub enum Value {
    Number(f64),
    /// BigInt — arbitrary-precision integer. Stored as a string to preserve
    /// precision (we use crate::tjs_ext::BigInt for arithmetic).
    BigInt(String),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
    Object(Rc<RefCell<ObjectValue>>),
    Array(Rc<RefCell<Vec<Value>>>),
    Function(Rc<FunctionValue>),
    Builtin(BuiltinFn),
}

/// An object value — a HashMap of properties.
pub struct ObjectValue {
    pub properties: HashMap<String, Value>,
    pub prototype: Option<Value>,
}

/// A function value — parameters + body + closure scope.
pub struct FunctionValue {
    pub params: Vec<String>,
    pub body: Vec<crate::tjs::parser::Stmt>,
    pub closure: Rc<RefCell<crate::tjs::interpreter::Scope>>,
    pub name: String,
    /// Optional compiled bytecode for VM execution.
    /// When set, the VM can execute this function directly without
    /// falling back to the tree-walking interpreter.
    pub vm_code: Option<Rc<Vec<crate::tjs::vm::Bytecode>>>,
    pub vm_nlocals: u32,
    /// Static properties on the function (used for class static methods/fields).
    /// When you do `MyClass.staticMethod()`, the property is looked up here.
    pub static_props: RefCell<HashMap<String, Value>>,
}

/// A builtin function implemented in Rust.
pub struct BuiltinFn {
    pub name: String,
    pub func: Rc<dyn Fn(Vec<Value>) -> Result<Value, String>>,
}

impl Clone for BuiltinFn {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            func: self.func.clone(),
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::BigInt(_) => "bigint",
            Value::String(_) => "string",
            Value::Boolean(_) => "boolean",
            Value::Null => "object", // JS quirk: typeof null === "object"
            Value::Undefined => "undefined",
            Value::Object(_) => "object",
            Value::Array(_) => "object",
            Value::Function(_) => "function",
            Value::Builtin(_) => "function",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::BigInt(s) => s != "0" && !s.is_empty(),
            Value::String(s) => !s.is_empty(),
            Value::Boolean(b) => *b,
            Value::Null => false,
            Value::Undefined => false,
            _ => true,
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            Value::BigInt(s) => s.parse().unwrap_or(f64::NAN),
            Value::String(s) => s.parse().unwrap_or(f64::NAN),
            Value::Boolean(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Null => 0.0,
            Value::Undefined => f64::NAN,
            _ => f64::NAN,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e21 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            Value::BigInt(s) => s.clone(),
            Value::String(s) => s.clone(),
            Value::Boolean(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Undefined => "undefined".to_string(),
            Value::Object(_) => "[object Object]".to_string(),
            Value::Array(arr) => {
                let arr = arr.borrow();
                arr.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            }
            Value::Function(f) => format!("function {}() {{}}", f.name),
            Value::Builtin(f) => format!("function {}() {{ [native code] }}", f.name),
        }
    }

    pub fn equals(&self, other: &Value) -> bool {
        // Strict equality (===).
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::BigInt(a), Value::BigInt(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Undefined, Value::Undefined) => true,
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    pub fn loose_equals(&self, other: &Value) -> bool {
        // Loose equality (==).
        match (self, other) {
            (a, b) if a.equals(b) => true,
            (Value::Number(a), Value::String(b)) => *a == b.parse::<f64>().unwrap_or(f64::NAN),
            (Value::String(a), Value::Number(b)) => a.parse::<f64>().unwrap_or(f64::NAN) == *b,
            (Value::Boolean(b), other) => {
                Value::Number(if *b { 1.0 } else { 0.0 }).loose_equals(other)
            }
            (other, Value::Boolean(b)) => {
                other.loose_equals(&Value::Number(if *b { 1.0 } else { 0.0 }))
            }
            (Value::Null, Value::Undefined) => true,
            (Value::Undefined, Value::Null) => true,
            _ => false,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", n),
            Value::BigInt(s) => write!(f, "{}n", s),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Undefined => write!(f, "undefined"),
            Value::Object(_) => write!(f, "[Object]"),
            Value::Array(arr) => write!(f, "[Array({})]", arr.borrow().len()),
            Value::Function(func) => write!(f, "[Function: {}]", func.name),
            Value::Builtin(func) => write!(f, "[Builtin: {}]", func.name),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

impl Default for ObjectValue {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectValue {
    pub fn new() -> Self {
        Self {
            properties: HashMap::new(),
            prototype: None,
        }
    }

    pub fn get(&self, key: &str) -> Value {
        if let Some(v) = self.properties.get(key) {
            v.clone()
        } else if let Some(proto) = &self.prototype {
            proto.get_property(key)
        } else {
            Value::Undefined
        }
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.properties.insert(key.to_string(), value);
    }
}

impl Value {
    /// Get a property from any value type.
    pub fn get_property(&self, key: &str) -> Value {
        match self {
            Value::Object(obj) => obj.borrow().get(key),
            Value::Array(arr) => {
                // `arr` is the Rc<RefCell<Vec<Value>>> backing this array.
                // Builtins below capture a clone so they can read/mutate it.
                let arr_clone = arr.clone();
                let arr_clone2 = arr.clone();
                match key {
                    "length" => Value::Number(arr.borrow().len() as f64),
                    "push" => Value::Builtin(BuiltinFn {
                        name: "push".to_string(),
                        func: Rc::new(move |args| {
                            arr_clone.borrow_mut().extend(args);
                            Ok(Value::Number(arr_clone.borrow().len() as f64))
                        }),
                    }),
                    "pop" => Value::Builtin(BuiltinFn {
                        name: "pop".to_string(),
                        func: Rc::new(move |_| {
                            Ok(arr_clone2.borrow_mut().pop().unwrap_or(Value::Undefined))
                        }),
                    }),
                    "join" => Value::Builtin(BuiltinFn {
                        name: "join".to_string(),
                        func: Rc::new(move |args| {
                            let sep = match args.first() {
                                Some(Value::String(s)) => s.clone(),
                                _ => ",".to_string(),
                            };
                            let items: Vec<String> = arr_clone
                                .borrow()
                                .iter()
                                .map(|v| match v {
                                    Value::Undefined | Value::Null => String::new(),
                                    other => other.to_string(),
                                })
                                .collect();
                            Ok(Value::String(items.join(&sep)))
                        }),
                    }),
                    "map" => Value::Builtin(BuiltinFn {
                        name: "map".to_string(),
                        func: Rc::new(move |args| {
                            let cb = args.first().cloned().unwrap_or(Value::Undefined);
                            let src = arr_clone.borrow();
                            let mut out = Vec::with_capacity(src.len());
                            for (i, v) in src.iter().enumerate() {
                                let r = crate::tjs::interpreter::call_js(
                                    &cb,
                                    vec![
                                        v.clone(),
                                        Value::Number(i as f64),
                                        Value::Array(arr_clone.clone()),
                                    ],
                                )?;
                                out.push(r);
                            }
                            Ok(Value::Array(Rc::new(RefCell::new(out))))
                        }),
                    }),
                    "filter" => Value::Builtin(BuiltinFn {
                        name: "filter".to_string(),
                        func: Rc::new(move |args| {
                            let cb = args.first().cloned().unwrap_or(Value::Undefined);
                            let src = arr_clone.borrow();
                            let mut out = Vec::new();
                            for (i, v) in src.iter().enumerate() {
                                let keep = crate::tjs::interpreter::call_js(
                                    &cb,
                                    vec![
                                        v.clone(),
                                        Value::Number(i as f64),
                                        Value::Array(arr_clone.clone()),
                                    ],
                                )?;
                                if keep.is_truthy() {
                                    out.push(v.clone());
                                }
                            }
                            Ok(Value::Array(Rc::new(RefCell::new(out))))
                        }),
                    }),
                    "reduce" => Value::Builtin(BuiltinFn {
                        name: "reduce".to_string(),
                        func: Rc::new(move |args| {
                            let cb = args.first().cloned().unwrap_or(Value::Undefined);
                            let has_initial = args.get(1).is_some();
                            let initial = args.get(1).cloned();
                            let src = arr_clone.borrow();
                            let mut acc = match initial {
                                Some(v) => v,
                                None => {
                                    if src.is_empty() {
                                        return Err(
                                            "reduce of empty array with no initial value".into()
                                        );
                                    }
                                    src[0].clone()
                                }
                            };
                            let start = if has_initial { 0 } else { 1 };
                            for i in start..src.len() {
                                acc = crate::tjs::interpreter::call_js(
                                    &cb,
                                    vec![
                                        acc,
                                        src[i].clone(),
                                        Value::Number(i as f64),
                                        Value::Array(arr_clone.clone()),
                                    ],
                                )?;
                            }
                            Ok(acc)
                        }),
                    }),
                    "forEach" => Value::Builtin(BuiltinFn {
                        name: "forEach".to_string(),
                        func: Rc::new(move |args| {
                            let cb = args.first().cloned().unwrap_or(Value::Undefined);
                            let src = arr_clone.borrow();
                            for (i, v) in src.iter().enumerate() {
                                crate::tjs::interpreter::call_js(
                                    &cb,
                                    vec![
                                        v.clone(),
                                        Value::Number(i as f64),
                                        Value::Array(arr_clone.clone()),
                                    ],
                                )?;
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                    "slice" => Value::Builtin(BuiltinFn {
                        name: "slice".to_string(),
                        func: Rc::new(move |args| {
                            let src = arr_clone.borrow();
                            let len = src.len() as i64;
                            let mut start = match args.first() {
                                Some(Value::Number(n)) => *n as i64,
                                _ => 0,
                            };
                            let mut end = match args.get(1) {
                                Some(Value::Number(n)) => *n as i64,
                                _ => len,
                            };
                            if start < 0 {
                                start += len;
                            }
                            if end < 0 {
                                end += len;
                            }
                            start = start.clamp(0, len);
                            end = end.clamp(0, len);
                            let out: Vec<Value> = src[start as usize..end as usize].to_vec();
                            Ok(Value::Array(Rc::new(RefCell::new(out))))
                        }),
                    }),
                    "concat" => Value::Builtin(BuiltinFn {
                        name: "concat".to_string(),
                        func: Rc::new(move |args| {
                            let mut out = arr_clone.borrow().clone();
                            for a in args {
                                match a {
                                    Value::Array(o) => out.extend(o.borrow().iter().cloned()),
                                    other => out.push(other),
                                }
                            }
                            Ok(Value::Array(Rc::new(RefCell::new(out))))
                        }),
                    }),
                    "includes" => Value::Builtin(BuiltinFn {
                        name: "includes".to_string(),
                        func: Rc::new(move |args| {
                            let target = args.first().cloned().unwrap_or(Value::Undefined);
                            let found = arr_clone.borrow().iter().any(|v| *v == target);
                            Ok(Value::Boolean(found))
                        }),
                    }),
                    "indexOf" => Value::Builtin(BuiltinFn {
                        name: "indexOf".to_string(),
                        func: Rc::new(move |args| {
                            let target = args.first().cloned().unwrap_or(Value::Undefined);
                            let src = arr_clone.borrow();
                            let idx = src.iter().position(|v| *v == target);
                            Ok(Value::Number(idx.map(|i| i as f64).unwrap_or(-1.0)))
                        }),
                    }),
                    "reverse" => Value::Builtin(BuiltinFn {
                        name: "reverse".to_string(),
                        func: Rc::new(move |_| {
                            arr_clone.borrow_mut().reverse();
                            Ok(Value::Array(arr_clone.clone()))
                        }),
                    }),
                    "find" => Value::Builtin(BuiltinFn {
                        name: "find".to_string(),
                        func: Rc::new(move |args| {
                            let cb = args.first().cloned().unwrap_or(Value::Undefined);
                            let src = arr_clone.borrow();
                            for (i, v) in src.iter().enumerate() {
                                let ok = crate::tjs::interpreter::call_js(
                                    &cb,
                                    vec![
                                        v.clone(),
                                        Value::Number(i as f64),
                                        Value::Array(arr_clone.clone()),
                                    ],
                                )?;
                                if ok.is_truthy() {
                                    return Ok(v.clone());
                                }
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                    "some" => Value::Builtin(BuiltinFn {
                        name: "some".to_string(),
                        func: Rc::new(move |args| {
                            let cb = args.first().cloned().unwrap_or(Value::Undefined);
                            let src = arr_clone.borrow();
                            for (i, v) in src.iter().enumerate() {
                                let ok = crate::tjs::interpreter::call_js(
                                    &cb,
                                    vec![
                                        v.clone(),
                                        Value::Number(i as f64),
                                        Value::Array(arr_clone.clone()),
                                    ],
                                )?;
                                if ok.is_truthy() {
                                    return Ok(Value::Boolean(true));
                                }
                            }
                            Ok(Value::Boolean(false))
                        }),
                    }),
                    "every" => Value::Builtin(BuiltinFn {
                        name: "every".to_string(),
                        func: Rc::new(move |args| {
                            let cb = args.first().cloned().unwrap_or(Value::Undefined);
                            let src = arr_clone.borrow();
                            for (i, v) in src.iter().enumerate() {
                                let ok = crate::tjs::interpreter::call_js(
                                    &cb,
                                    vec![
                                        v.clone(),
                                        Value::Number(i as f64),
                                        Value::Array(arr_clone.clone()),
                                    ],
                                )?;
                                if !ok.is_truthy() {
                                    return Ok(Value::Boolean(false));
                                }
                            }
                            Ok(Value::Boolean(true))
                        }),
                    }),
                    _ => {
                        if let Ok(idx) = key.parse::<usize>() {
                            let arr = arr.borrow();
                            if idx < arr.len() {
                                arr[idx].clone()
                            } else {
                                Value::Undefined
                            }
                        } else {
                            Value::Undefined
                        }
                    }
                }
            }
            Value::String(s) => {
                match key {
                    "length" => Value::Number(s.chars().count() as f64),
                    "toUpperCase" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "toUpperCase".to_string(),
                            func: Rc::new(move |_| Ok(Value::String(s.to_uppercase()))),
                        })
                    }
                    "toLowerCase" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "toLowerCase".to_string(),
                            func: Rc::new(move |_| Ok(Value::String(s.to_lowercase()))),
                        })
                    }
                    "charAt" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "charAt".to_string(),
                            func: Rc::new(move |args| {
                                let idx = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
                                Ok(Value::String(
                                    s.chars()
                                        .nth(idx)
                                        .map(|c| c.to_string())
                                        .unwrap_or_default(),
                                ))
                            }),
                        })
                    }
                    "indexOf" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "indexOf".to_string(),
                            func: Rc::new(move |args| {
                                let search =
                                    args.first().map(|v| v.to_string()).unwrap_or_default();
                                Ok(Value::Number(
                                    s.find(&search)
                                        .map(|i| {
                                            // Convert byte index to char index.
                                            s[..i].chars().count() as f64
                                        })
                                        .unwrap_or(-1.0),
                                ))
                            }),
                        })
                    }
                    "includes" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "includes".to_string(),
                            func: Rc::new(move |args| {
                                let search =
                                    args.first().map(|v| v.to_string()).unwrap_or_default();
                                Ok(Value::Boolean(s.contains(&search)))
                            }),
                        })
                    }
                    "startsWith" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "startsWith".to_string(),
                            func: Rc::new(move |args| {
                                let search =
                                    args.first().map(|v| v.to_string()).unwrap_or_default();
                                Ok(Value::Boolean(s.starts_with(&search)))
                            }),
                        })
                    }
                    "endsWith" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "endsWith".to_string(),
                            func: Rc::new(move |args| {
                                let search =
                                    args.first().map(|v| v.to_string()).unwrap_or_default();
                                Ok(Value::Boolean(s.ends_with(&search)))
                            }),
                        })
                    }
                    "slice" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "slice".to_string(),
                            func: Rc::new(move |args| {
                                let chars: Vec<char> = s.chars().collect();
                                let start = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                                let end = args
                                    .get(1)
                                    .map(|v| v.to_number() as i64)
                                    .unwrap_or(chars.len() as i64);
                                let start = if start < 0 {
                                    (chars.len() as i64 + start).max(0) as usize
                                } else {
                                    start as usize
                                };
                                let end = if end < 0 {
                                    (chars.len() as i64 + end).max(0) as usize
                                } else {
                                    end.min(chars.len() as i64) as usize
                                };
                                if start >= end {
                                    return Ok(Value::String(String::new()));
                                }
                                Ok(Value::String(chars[start..end].iter().collect()))
                            }),
                        })
                    }
                    "substring" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "substring".to_string(),
                            func: Rc::new(move |args| {
                                let chars: Vec<char> = s.chars().collect();
                                let mut start =
                                    args.first().map(|v| v.to_number() as usize).unwrap_or(0);
                                let mut end = args
                                    .get(1)
                                    .map(|v| v.to_number() as usize)
                                    .unwrap_or(chars.len());
                                if start > end {
                                    std::mem::swap(&mut start, &mut end);
                                }
                                start = start.min(chars.len());
                                end = end.min(chars.len());
                                Ok(Value::String(chars[start..end].iter().collect()))
                            }),
                        })
                    }
                    "trim" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "trim".to_string(),
                            func: Rc::new(move |_| Ok(Value::String(s.trim().to_string()))),
                        })
                    }
                    "replace" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "replace".to_string(),
                            func: Rc::new(move |args| {
                                let to = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                                // If the first arg is a RegExp object (has __regex_ptr),
                                // use real regex replacement.
                                if let Some(Value::Object(o)) = args.first() {
                                    if o.borrow().properties.get("__regex_ptr").is_some() {
                                        let ptr_val = o.borrow().properties.get("__regex_ptr").cloned();
                                        let is_global = o.borrow().properties.get("global")
                                            .map(|v| matches!(v, Value::Boolean(true)))
                                            .unwrap_or(false);
                                        if let Some(Value::Number(ptr)) = ptr_val {
                                            let re = unsafe { &*(ptr as usize as *mut regex::Regex) };
                                            if is_global {
                                                return Ok(Value::String(re.replace_all(&s, to.as_str()).to_string()));
                                            } else {
                                                return Ok(Value::String(re.replace(&s, to.as_str()).to_string()));
                                            }
                                        }
                                    }
                                }
                                let from = args.first().map(|v| v.to_string()).unwrap_or_default();
                                Ok(Value::String(s.replacen(&from, &to, 1)))
                            }),
                        })
                    }
                    "match" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "match".to_string(),
                            func: Rc::new(move |args| {
                                if let Some(Value::Object(o)) = args.first() {
                                    if o.borrow().properties.get("__regex_ptr").is_some() {
                                        let ptr_val = o.borrow().properties.get("__regex_ptr").cloned();
                                        if let Some(Value::Number(ptr)) = ptr_val {
                                            let re = unsafe { &*(ptr as usize as *mut regex::Regex) };
                                            if let Some(caps) = re.captures(&s) {
                                                let mut result = ObjectValue::new();
                                                let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                                                result.set("0", Value::String(full.to_string()));
                                                for i in 1..caps.len() {
                                                    if let Some(m) = caps.get(i) {
                                                        result.set(&i.to_string(), Value::String(m.as_str().to_string()));
                                                    }
                                                }
                                                result.set("index", Value::Number(caps.get(0).map(|m| m.start()).unwrap_or(0) as f64));
                                                result.set("input", Value::String(s.clone()));
                                                return Ok(Value::Object(Rc::new(RefCell::new(result))));
                                            }
                                            return Ok(Value::Null);
                                        }
                                    }
                                }
                                // Fallback: simple string search.
                                let pattern = args.first().map(|v| v.to_string()).unwrap_or_default();
                                if let Some(pos) = s.find(&pattern) {
                                    let mut result = ObjectValue::new();
                                    result.set("0", Value::String(pattern));
                                    result.set("index", Value::Number(pos as f64));
                                    result.set("input", Value::String(s.clone()));
                                    Ok(Value::Object(Rc::new(RefCell::new(result))))
                                } else {
                                    Ok(Value::Null)
                                }
                            }),
                        })
                    }
                    "search" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "search".to_string(),
                            func: Rc::new(move |args| {
                                if let Some(Value::Object(o)) = args.first() {
                                    if o.borrow().properties.get("__regex_ptr").is_some() {
                                        let ptr_val = o.borrow().properties.get("__regex_ptr").cloned();
                                        if let Some(Value::Number(ptr)) = ptr_val {
                                            let re = unsafe { &*(ptr as usize as *mut regex::Regex) };
                                            if let Some(m) = re.find(&s) {
                                                return Ok(Value::Number(m.start() as f64));
                                            }
                                            return Ok(Value::Number(-1.0));
                                        }
                                    }
                                }
                                let pattern = args.first().map(|v| v.to_string()).unwrap_or_default();
                                Ok(Value::Number(s.find(&pattern).map(|p| p as f64).unwrap_or(-1.0)))
                            }),
                        })
                    }
                    "split" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "split".to_string(),
                            func: Rc::new(move |args| {
                                // If arg is a RegExp, use regex split.
                                if let Some(Value::Object(o)) = args.first() {
                                    if o.borrow().properties.get("__regex_ptr").is_some() {
                                        let ptr_val = o.borrow().properties.get("__regex_ptr").cloned();
                                        if let Some(Value::Number(ptr)) = ptr_val {
                                            let re = unsafe { &*(ptr as usize as *mut regex::Regex) };
                                            let parts: Vec<Value> = re.split(&s).map(|p| Value::String(p.to_string())).collect();
                                            return Ok(Value::Array(Rc::new(RefCell::new(parts))));
                                        }
                                    }
                                }
                                let sep = args.first().map(|v| v.to_string()).unwrap_or_default();
                                let limit = args.get(1).map(|v| v.to_number() as usize).unwrap_or(usize::MAX);
                                let parts: Vec<Value> = if sep.is_empty() {
                                    s.chars().take(limit).map(|c| Value::String(c.to_string())).collect()
                                } else {
                                    s.split(&sep).take(limit).map(|p| Value::String(p.to_string())).collect()
                                };
                                Ok(Value::Array(Rc::new(RefCell::new(parts))))
                            }),
                        })
                    }
                    "repeat" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "repeat".to_string(),
                            func: Rc::new(move |args| {
                                let n = args.first().map(|v| v.to_number()).unwrap_or(0.0) as usize;
                                Ok(Value::String(s.repeat(n)))
                            }),
                        })
                    }
                    "padStart" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "padStart".to_string(),
                            func: Rc::new(move |args| {
                                let target =
                                    args.first().map(|v| v.to_number()).unwrap_or(0.0) as usize;
                                let pad = args
                                    .get(1)
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| " ".to_string());
                                if s.len() >= target || pad.is_empty() {
                                    Ok(Value::String(s.clone()))
                                } else {
                                    let mut out = String::new();
                                    let need = target - s.len();
                                    while out.len() < need {
                                        out.push_str(&pad);
                                    }
                                    out.truncate(need);
                                    out.push_str(s.as_str());
                                    Ok(Value::String(out))
                                }
                            }),
                        })
                    }
                    "padEnd" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "padEnd".to_string(),
                            func: Rc::new(move |args| {
                                let target =
                                    args.first().map(|v| v.to_number()).unwrap_or(0.0) as usize;
                                let pad = args
                                    .get(1)
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| " ".to_string());
                                if s.len() >= target || pad.is_empty() {
                                    Ok(Value::String(s.clone()))
                                } else {
                                    let mut out = s.clone();
                                    let need = target - s.len();
                                    while out.len() < target {
                                        out.push_str(&pad);
                                    }
                                    out.truncate(target);
                                    Ok(Value::String(out))
                                }
                            }),
                        })
                    }
                    "trimStart" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "trimStart".to_string(),
                            func: Rc::new(move |_| Ok(Value::String(s.trim_start().to_string()))),
                        })
                    }
                    "trimEnd" => {
                        let s = s.clone();
                        Value::Builtin(BuiltinFn {
                            name: "trimEnd".to_string(),
                            func: Rc::new(move |_| Ok(Value::String(s.trim_end().to_string()))),
                        })
                    }
                    _ => {
                        if let Ok(idx) = key.parse::<usize>() {
                            s.chars()
                                .nth(idx)
                                .map(|c| Value::String(c.to_string()))
                                .unwrap_or(Value::Undefined)
                        } else {
                            Value::Undefined
                        }
                    }
                }
            }
            Value::Function(f) => {
                // Check static properties first (class static methods/fields).
                if let Some(v) = f.static_props.borrow().get(key).cloned() {
                    return v;
                }
                // Check closure for "prototype" property.
                if key == "prototype" {
                    if let Some(v) = f.closure.borrow().vars.borrow().iter()
                        .find(|(k, _)| k == "prototype")
                        .map(|(_, v)| v.clone())
                    {
                        return v;
                    }
                }
                // Common function methods.
                match key {
                    "call" => Value::Builtin(BuiltinFn {
                        name: "call".to_string(),
                        func: Rc::new(|_| Ok(Value::Undefined)),
                    }),
                    "apply" => Value::Builtin(BuiltinFn {
                        name: "apply".to_string(),
                        func: Rc::new(|_| Ok(Value::Undefined)),
                    }),
                    "bind" => Value::Builtin(BuiltinFn {
                        name: "bind".to_string(),
                        func: Rc::new(|_| Ok(Value::Undefined)),
                    }),
                    "name" => Value::String(f.name.clone()),
                    "length" => Value::Number(f.params.len() as f64),
                    _ => Value::Undefined,
                }
            }
            Value::Builtin(b) => {
                // Check if this is a known builtin with static methods.
                match b.name.as_str() {
                    "Array" => match key {
                        "isArray" => Value::Builtin(BuiltinFn {
                            name: "Array.isArray".to_string(),
                            func: Rc::new(|args| {
                                Ok(Value::Boolean(matches!(args.first(), Some(Value::Array(_)))))
                            }),
                        }),
                        "from" => Value::Builtin(BuiltinFn {
                            name: "Array.from".to_string(),
                            func: Rc::new(|args| {
                                match args.first().cloned().unwrap_or(Value::Undefined) {
                                    Value::Array(arr) => return Ok(Value::Array(arr)),
                                    Value::String(s) => {
                                        let chars: Vec<Value> = s.chars().map(|c| Value::String(c.to_string())).collect();
                                        return Ok(Value::Array(Rc::new(RefCell::new(chars))));
                                    }
                                    _ => {}
                                }
                                Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                            }),
                        }),
                        "of" => Value::Builtin(BuiltinFn {
                            name: "Array.of".to_string(),
                            func: Rc::new(|args| Ok(Value::Array(Rc::new(RefCell::new(args))))),
                        }),
                        _ => Value::Undefined,
                    },
                    "Object" => match key {
                        "keys" => Value::Builtin(BuiltinFn {
                            name: "Object.keys".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Object(o)) = args.first() {
                                    let keys: Vec<Value> = o.borrow().properties.keys().map(|k| Value::String(k.clone())).collect();
                                    return Ok(Value::Array(Rc::new(RefCell::new(keys))));
                                }
                                Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                            }),
                        }),
                        "values" => Value::Builtin(BuiltinFn {
                            name: "Object.values".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Object(o)) = args.first() {
                                    let vals: Vec<Value> = o.borrow().properties.values().cloned().collect();
                                    return Ok(Value::Array(Rc::new(RefCell::new(vals))));
                                }
                                Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                            }),
                        }),
                        "entries" => Value::Builtin(BuiltinFn {
                            name: "Object.entries".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Object(o)) = args.first() {
                                    let entries: Vec<Value> = o.borrow().properties.iter().map(|(k, v)| {
                                        Value::Array(Rc::new(RefCell::new(vec![Value::String(k.clone()), v.clone()])))
                                    }).collect();
                                    return Ok(Value::Array(Rc::new(RefCell::new(entries))));
                                }
                                Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                            }),
                        }),
                        "assign" => Value::Builtin(BuiltinFn {
                            name: "Object.assign".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Object(target)) = args.first() {
                                    for src in args.iter().skip(1) {
                                        if let Value::Object(s) = src {
                                            let props: Vec<(String, Value)> = s.borrow().properties.iter().map(|(k,v)| (k.clone(), v.clone())).collect();
                                            for (k, v) in props {
                                                target.borrow_mut().set(&k, v);
                                            }
                                        }
                                    }
                                    return Ok(args.first().cloned().unwrap_or(Value::Undefined));
                                }
                                Ok(Value::Undefined)
                            }),
                        }),
                        "freeze" => Value::Builtin(BuiltinFn {
                            name: "Object.freeze".to_string(),
                            func: Rc::new(|args| Ok(args.first().cloned().unwrap_or(Value::Undefined))),
                        }),
                        "create" => Value::Builtin(BuiltinFn {
                            name: "Object.create".to_string(),
                            func: Rc::new(|args| {
                                let proto = args.first().cloned().unwrap_or(Value::Null);
                                let mut obj = ObjectValue::new();
                                if !matches!(proto, Value::Null) {
                                    obj.prototype = Some(proto);
                                }
                                Ok(Value::Object(Rc::new(RefCell::new(obj))))
                            }),
                        }),
                        "getPrototypeOf" => Value::Builtin(BuiltinFn {
                            name: "Object.getPrototypeOf".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Object(o)) = args.first() {
                                    return Ok(o.borrow().prototype.clone().unwrap_or(Value::Null));
                                }
                                Ok(Value::Null)
                            }),
                        }),
                        "is" => Value::Builtin(BuiltinFn {
                            name: "Object.is".to_string(),
                            func: Rc::new(|args| {
                                let a = args.first().cloned().unwrap_or(Value::Undefined);
                                let b = args.get(1).cloned().unwrap_or(Value::Undefined);
                                Ok(Value::Boolean(a.equals(&b)))
                            }),
                        }),
                        _ => Value::Undefined,
                    },
                    "Number" => match key {
                        "isInteger" => Value::Builtin(BuiltinFn {
                            name: "Number.isInteger".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Number(n)) = args.first() {
                                    return Ok(Value::Boolean(n.fract() == 0.0 && n.is_finite()));
                                }
                                Ok(Value::Boolean(false))
                            }),
                        }),
                        "isFinite" => Value::Builtin(BuiltinFn {
                            name: "Number.isFinite".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Number(n)) = args.first() {
                                    return Ok(Value::Boolean(n.is_finite()));
                                }
                                Ok(Value::Boolean(false))
                            }),
                        }),
                        "isNaN" => Value::Builtin(BuiltinFn {
                            name: "Number.isNaN".to_string(),
                            func: Rc::new(|args| {
                                if let Some(Value::Number(n)) = args.first() {
                                    return Ok(Value::Boolean(n.is_nan()));
                                }
                                Ok(Value::Boolean(false))
                            }),
                        }),
                        "parseInt" => Value::Builtin(BuiltinFn {
                            name: "Number.parseInt".to_string(),
                            func: Rc::new(|args| {
                                let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                                let radix = args.get(1).map(|v| v.to_number() as u32).unwrap_or(10);
                                let radix = if radix == 0 { 10 } else { radix };
                                Ok(Value::Number(i64::from_str_radix(s.trim(), radix).map(|v| v as f64).unwrap_or(f64::NAN)))
                            }),
                        }),
                        "parseFloat" => Value::Builtin(BuiltinFn {
                            name: "Number.parseFloat".to_string(),
                            func: Rc::new(|args| {
                                let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                                Ok(Value::Number(s.trim().parse::<f64>().unwrap_or(f64::NAN)))
                            }),
                        }),
                        "MAX_SAFE_INTEGER" => Value::Number(9007199254740991.0),
                        "MIN_SAFE_INTEGER" => Value::Number(-9007199254740991.0),
                        "MAX_VALUE" => Value::Number(f64::MAX),
                        "MIN_VALUE" => Value::Number(f64::MIN_POSITIVE),
                        "EPSILON" => Value::Number(f64::EPSILON),
                        "POSITIVE_INFINITY" => Value::Number(f64::INFINITY),
                        "NEGATIVE_INFINITY" => Value::Number(f64::NEG_INFINITY),
                        "NaN" => Value::Number(f64::NAN),
                        _ => Value::Undefined,
                    },
                    _ => match key {
                        "call" => Value::Builtin(BuiltinFn {
                            name: "call".to_string(),
                            func: Rc::new(|_| Ok(Value::Undefined)),
                        }),
                        "apply" => Value::Builtin(BuiltinFn {
                            name: "apply".to_string(),
                            func: Rc::new(|_| Ok(Value::Undefined)),
                        }),
                        "bind" => Value::Builtin(BuiltinFn {
                            name: "bind".to_string(),
                            func: Rc::new(|_| Ok(Value::Undefined)),
                        }),
                        _ => Value::Undefined,
                    },
                }
            }
            _ => Value::Undefined,
        }
    }

    /// Set a property on any value type.
    pub fn set_property(&self, key: &str, value: Value) {
        match self {
            Value::Object(obj) => {
                obj.borrow_mut().set(key, value);
            }
            Value::Array(arr) => {
                if key == "length" {
                    let len = value.to_number() as usize;
                    let mut a = arr.borrow_mut();
                    a.resize(len, Value::Undefined);
                } else if let Ok(idx) = key.parse::<usize>() {
                    let mut a = arr.borrow_mut();
                    while a.len() <= idx {
                        a.push(Value::Undefined);
                    }
                    a[idx] = value;
                }
            }
            _ => {}
        }
    }
}
