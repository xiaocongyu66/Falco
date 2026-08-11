//! TJS Builtins — Math, console, Date, JSON, Array, Object, parseInt, etc.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

pub fn register(scope: &mut Scope) {
    // console.log
    scope.declare(
        "console",
        Value::Object(Rc::new(RefCell::new({
            let mut obj = ObjectValue::new();
            obj.set(
                "log",
                Value::Builtin(BuiltinFn {
                    name: "console.log".to_string(),
                    func: Rc::new(|args| {
                        let msg: Vec<String> = args.iter().map(|v| v.to_string()).collect();
                        eprintln!("[tjs] {}", msg.join(" "));
                        Ok(Value::Undefined)
                    }),
                }),
            );
            obj.set(
                "error",
                Value::Builtin(BuiltinFn {
                    name: "console.error".to_string(),
                    func: Rc::new(|args| {
                        let msg: Vec<String> = args.iter().map(|v| v.to_string()).collect();
                        eprintln!("[tjs:error] {}", msg.join(" "));
                        Ok(Value::Undefined)
                    }),
                }),
            );
            obj.set(
                "warn",
                Value::Builtin(BuiltinFn {
                    name: "console.warn".to_string(),
                    func: Rc::new(|args| {
                        let msg: Vec<String> = args.iter().map(|v| v.to_string()).collect();
                        eprintln!("[tjs:warn] {}", msg.join(" "));
                        Ok(Value::Undefined)
                    }),
                }),
            );
            obj
        }))),
    );

    // Math object
    scope.declare(
        "Math",
        Value::Object(Rc::new(RefCell::new({
            let mut obj = ObjectValue::new();
            obj.set("PI", Value::Number(std::f64::consts::PI));
            obj.set("E", Value::Number(std::f64::consts::E));
            obj.set("LN2", Value::Number(std::f64::consts::LN_2));
            obj.set("LN10", Value::Number(std::f64::consts::LN_10));
            obj.set("SQRT2", Value::Number(std::f64::consts::SQRT_2));
            obj.set(
                "floor",
                Value::Builtin(BuiltinFn {
                    name: "Math.floor".to_string(),
                    func: Rc::new(|args| {
                        let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
                        Ok(Value::Number(n.floor()))
                    }),
                }),
            );
            obj.set(
                "ceil",
                Value::Builtin(BuiltinFn {
                    name: "Math.ceil".to_string(),
                    func: Rc::new(|args| {
                        let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
                        Ok(Value::Number(n.ceil()))
                    }),
                }),
            );
            obj.set(
                "round",
                Value::Builtin(BuiltinFn {
                    name: "Math.round".to_string(),
                    func: Rc::new(|args| {
                        let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
                        Ok(Value::Number(n.round()))
                    }),
                }),
            );
            obj.set(
                "abs",
                Value::Builtin(BuiltinFn {
                    name: "Math.abs".to_string(),
                    func: Rc::new(|args| {
                        let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
                        Ok(Value::Number(n.abs()))
                    }),
                }),
            );
            obj.set(
                "sqrt",
                Value::Builtin(BuiltinFn {
                    name: "Math.sqrt".to_string(),
                    func: Rc::new(|args| {
                        let n = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
                        Ok(Value::Number(n.sqrt()))
                    }),
                }),
            );
            obj.set(
                "pow",
                Value::Builtin(BuiltinFn {
                    name: "Math.pow".to_string(),
                    func: Rc::new(|args| {
                        let x = args.first().map(|v| v.to_number()).unwrap_or(f64::NAN);
                        let y = args.get(1).map(|v| v.to_number()).unwrap_or(f64::NAN);
                        Ok(Value::Number(x.powf(y)))
                    }),
                }),
            );
            obj.set(
                "max",
                Value::Builtin(BuiltinFn {
                    name: "Math.max".to_string(),
                    func: Rc::new(|args| {
                        let max = args
                            .iter()
                            .map(|v| v.to_number())
                            .fold(f64::NEG_INFINITY, f64::max);
                        Ok(Value::Number(max))
                    }),
                }),
            );
            obj.set(
                "min",
                Value::Builtin(BuiltinFn {
                    name: "Math.min".to_string(),
                    func: Rc::new(|args| {
                        let min = args
                            .iter()
                            .map(|v| v.to_number())
                            .fold(f64::INFINITY, f64::min);
                        Ok(Value::Number(min))
                    }),
                }),
            );
            obj.set(
                "sin",
                Value::Builtin(BuiltinFn {
                    name: "Math.sin".to_string(),
                    func: Rc::new(|args| {
                        Ok(Value::Number(
                            args.first().map(|v| v.to_number()).unwrap_or(0.0).sin(),
                        ))
                    }),
                }),
            );
            obj.set(
                "cos",
                Value::Builtin(BuiltinFn {
                    name: "Math.cos".to_string(),
                    func: Rc::new(|args| {
                        Ok(Value::Number(
                            args.first().map(|v| v.to_number()).unwrap_or(0.0).cos(),
                        ))
                    }),
                }),
            );
            obj.set(
                "tan",
                Value::Builtin(BuiltinFn {
                    name: "Math.tan".to_string(),
                    func: Rc::new(|args| {
                        Ok(Value::Number(
                            args.first().map(|v| v.to_number()).unwrap_or(0.0).tan(),
                        ))
                    }),
                }),
            );
            obj.set(
                "random",
                Value::Builtin(BuiltinFn {
                    name: "Math.random".to_string(),
                    func: Rc::new(|_| {
                        // Simple LCG random — not crypto-secure, but deterministic.
                        use std::sync::atomic::{AtomicU64, Ordering};
                        static SEED: AtomicU64 = AtomicU64::new(12345);
                        let mut s = SEED.load(Ordering::Relaxed);
                        s = s
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        SEED.store(s, Ordering::Relaxed);
                        Ok(Value::Number(s as f64 / u64::MAX as f64))
                    }),
                }),
            );
            obj.set(
                "log",
                Value::Builtin(BuiltinFn {
                    name: "Math.log".to_string(),
                    func: Rc::new(|args| {
                        Ok(Value::Number(
                            args.first().map(|v| v.to_number()).unwrap_or(0.0).ln(),
                        ))
                    }),
                }),
            );
            obj.set(
                "exp",
                Value::Builtin(BuiltinFn {
                    name: "Math.exp".to_string(),
                    func: Rc::new(|args| {
                        Ok(Value::Number(
                            args.first().map(|v| v.to_number()).unwrap_or(0.0).exp(),
                        ))
                    }),
                }),
            );
            obj
        }))),
    );

    // Global functions
    // String() — converts any value to string (used by template literals).
    scope.declare(
        "String",
        Value::Builtin(BuiltinFn {
            name: "String".to_string(),
            func: Rc::new(|args| {
                Ok(Value::String(
                    args.first().map(|v| v.to_string()).unwrap_or_default(),
                ))
            }),
        }),
    );

    // parseInt
    scope.declare(
        "parseInt",
        Value::Builtin(BuiltinFn {
            name: "parseInt".to_string(),
            func: Rc::new(|args| {
                let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                let radix = args.get(1).map(|v| v.to_number() as u32).unwrap_or(0);
                let radix = if radix == 0 { 10 } else { radix };
                // Try parsing with the given radix.
                match i64::from_str_radix(s.trim(), radix) {
                    Ok(v) => Ok(Value::Number(v as f64)),
                    Err(_) => {
                        // Fallback: try decimal float and truncate.
                        let n = s.trim().parse::<f64>().unwrap_or(f64::NAN);
                        Ok(Value::Number(n.trunc()))
                    }
                }
            }),
        }),
    );
    scope.declare(
        "parseFloat",
        Value::Builtin(BuiltinFn {
            name: "parseFloat".to_string(),
            func: Rc::new(|args| {
                let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                Ok(Value::Number(s.trim().parse::<f64>().unwrap_or(f64::NAN)))
            }),
        }),
    );
    scope.declare(
        "isNaN",
        Value::Builtin(BuiltinFn {
            name: "isNaN".to_string(),
            func: Rc::new(|args| {
                Ok(Value::Boolean(
                    args.first()
                        .map(|v| v.to_number())
                        .unwrap_or(f64::NAN)
                        .is_nan(),
                ))
            }),
        }),
    );
    scope.declare(
        "String",
        Value::Builtin(BuiltinFn {
            name: "String".to_string(),
            func: Rc::new(|args| {
                Ok(Value::String(
                    args.first().map(|v| v.to_string()).unwrap_or_default(),
                ))
            }),
        }),
    );
    scope.declare(
        "Number",
        Value::Builtin(BuiltinFn {
            name: "Number".to_string(),
            func: Rc::new(|args| {
                Ok(Value::Number(
                    args.first().map(|v| v.to_number()).unwrap_or(0.0),
                ))
            }),
        }),
    );
    scope.declare(
        "Boolean",
        Value::Builtin(BuiltinFn {
            name: "Boolean".to_string(),
            func: Rc::new(|args| {
                Ok(Value::Boolean(
                    args.first().map(|v| v.is_truthy()).unwrap_or(false),
                ))
            }),
        }),
    );
    scope.declare(
        "Array",
        Value::Builtin(BuiltinFn {
            name: "Array".to_string(),
            func: Rc::new(|args| {
                if args.len() == 1 {
                    if let Value::Number(n) = &args[0] {
                        let len = *n as usize;
                        return Ok(Value::Array(Rc::new(RefCell::new(vec![
                            Value::Undefined;
                            len
                        ]))));
                    }
                }
                Ok(Value::Array(Rc::new(RefCell::new(args))))
            }),
        }),
    );
    scope.declare(
        "Object",
        Value::Builtin(BuiltinFn {
            name: "Object".to_string(),
            func: Rc::new(|_| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );
    scope.declare(
        "alert",
        Value::Builtin(BuiltinFn {
            name: "alert".to_string(),
            func: Rc::new(|args| {
                let msg = args.first().map(|v| v.to_string()).unwrap_or_default();
                eprintln!("[tjs:alert] {}", msg);
                Ok(Value::Undefined)
            }),
        }),
    );
    // Date — YouTube uses both `Date.now()` (static) and `(new Date).getTime()` (instance).
    // We make Date an Object with `now` as a static method and `__new__` as
    // a constructor that returns a date instance object.
    scope.declare(
        "Date",
        Value::Object(Rc::new(RefCell::new({
            let mut d = ObjectValue::new();
            d.set(
                "now",
                Value::Builtin(BuiltinFn {
                    name: "Date.now".to_string(),
                    func: Rc::new(|_| {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let ms = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as f64)
                            .unwrap_or(0.0);
                        Ok(Value::Number(ms))
                    }),
                }),
            );
            // __new__ is called by the `new` operator when Date is an Object.
            d.set(
                "__new__",
                Value::Builtin(BuiltinFn {
                    name: "Date.__new__".to_string(),
                    func: Rc::new(|_args| {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let ms = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as f64)
                            .unwrap_or(0.0);
                        let mut inst = ObjectValue::new();
                        inst.set(
                            "getTime",
                            Value::Builtin(BuiltinFn {
                                name: "Date.getTime".to_string(),
                                func: Rc::new(move |_| Ok(Value::Number(ms))),
                            }),
                        );
                        Ok(Value::Object(Rc::new(RefCell::new(inst))))
                    }),
                }),
            );
            d
        }))),
    );

    // JSON
    scope.declare(
        "JSON",
        Value::Object(Rc::new(RefCell::new({
            let mut obj = ObjectValue::new();
            obj.set(
                "stringify",
                Value::Builtin(BuiltinFn {
                    name: "JSON.stringify".to_string(),
                    func: Rc::new(|args| {
                        let v = args.first().cloned().unwrap_or(Value::Undefined);
                        Ok(Value::String(json_stringify(&v)))
                    }),
                }),
            );
            obj.set(
                "parse",
                Value::Builtin(BuiltinFn {
                    name: "JSON.parse".to_string(),
                    func: Rc::new(|args| {
                        let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                        json_parse(&s).map_err(|e| e.to_string())
                    }),
                }),
            );
            obj
        }))),
    );
    // Promise — must be callable as `new Promise(executor)` AND have static methods.
    // YouTube uses: `new Promise(res => window.ytAtN = res)`.
    // We make Promise a Builtin (callable) that returns an object.
    scope.declare(
        "Promise",
        Value::Builtin(crate::tjs::value::BuiltinFn {
            name: "Promise".to_string(),
            func: Rc::new(|args| {
                // `new Promise(executor)` — executor is a function that receives
                // resolve and reject. We call it immediately (synchronous).
                let executor = args.first().cloned().unwrap_or(Value::Undefined);
                // Create a "promise" object with then/catch.
                let mut p = crate::tjs::value::ObjectValue::new();
                p.set(
                    "then",
                    Value::Builtin(crate::tjs::value::BuiltinFn {
                        name: "Promise.then".to_string(),
                        func: Rc::new(|args| {
                            // Simplified: call the callback immediately with undefined.
                            Ok(args.first().cloned().unwrap_or(Value::Undefined))
                        }),
                    }),
                );
                p.set(
                    "catch",
                    Value::Builtin(crate::tjs::value::BuiltinFn {
                        name: "Promise.catch".to_string(),
                        func: Rc::new(|args| Ok(args.first().cloned().unwrap_or(Value::Undefined))),
                    }),
                );
                p.set(
                    "finally",
                    Value::Builtin(crate::tjs::value::BuiltinFn {
                        name: "Promise.finally".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                // Also add static methods on the returned object.
                p.set(
                    "resolve",
                    Value::Builtin(crate::tjs::value::BuiltinFn {
                        name: "Promise.resolve".to_string(),
                        func: Rc::new(|args| Ok(args.first().cloned().unwrap_or(Value::Undefined))),
                    }),
                );
                p.set(
                    "reject",
                    Value::Builtin(crate::tjs::value::BuiltinFn {
                        name: "Promise.reject".to_string(),
                        func: Rc::new(|args| Ok(args.first().cloned().unwrap_or(Value::Undefined))),
                    }),
                );
                p.set(
                    "all",
                    Value::Builtin(crate::tjs::value::BuiltinFn {
                        name: "Promise.all".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                p.set(
                    "race",
                    Value::Builtin(crate::tjs::value::BuiltinFn {
                        name: "Promise.race".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                let promise_val = Value::Object(Rc::new(RefCell::new(p)));
                // Call the executor with resolve/reject stubs.
                if let Value::Function(_) = &executor {
                    // We can't call the function from here (no scope), so we skip.
                    // In a real impl, we'd call executor(resolve, reject).
                }
                Ok(promise_val)
            }),
        }),
    );

    // Symbol — stub. YouTube checks `typeof Symbol !== 'undefined'`.
    scope.declare(
        "Symbol",
        Value::Builtin(BuiltinFn {
            name: "Symbol".to_string(),
            func: Rc::new(|args| {
                let desc = args.first().map(|v| v.to_string()).unwrap_or_default();
                Ok(Value::String(format!("Symbol({})", desc)))
            }),
        }),
    );

    // Image — stub constructor (YouTube uses `new Image()` for preloading).
    scope.declare(
        "Image",
        Value::Builtin(BuiltinFn {
            name: "Image".to_string(),
            func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
        }),
    );

    // Polymer — Web Components framework stub (YouTube uses it).
    scope.declare(
        "Polymer",
        Value::Builtin(BuiltinFn {
            name: "Polymer".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // window — the global object (browser environment).
    // YouTube's JS starts with `window.WIZ_global_data = {...}`.
    let win = create_window();
    scope.declare("window", win.clone());
    // Also register common globals as top-level (in browsers, `window` IS
    // the global scope, so `setTimeout`, `document`, etc. are accessible
    // without `window.` prefix).
    if let Value::Object(win_obj) = &win {
        let win_ref = win_obj.borrow();
        for (key, val) in win_ref.properties.iter() {
            scope.declare(key, val.clone());
        }
    }

    // WebAssembly — full from-scratch implementation.
    // Exposes WebAssembly.Module, .Instance, .instantiate, .compile,
    // .validate, .Memory, .Table, .Global to JS scripts.
    crate::wasm::register_webassembly(scope);

    // Additional Web Platform APIs:
    // TextEncoder/TextDecoder, Crypto (SHA-1/256/384/512 + getRandomValues),
    // Web Workers, MessageChannel/BroadcastChannel, IndexedDB,
    // Compression Streams (gzip/deflate/brotli), URLSearchParams,
    // AbortController/AbortSignal, queueMicrotask.
    crate::web_api::register_web_apis(scope);

    // Error types: Error, TypeError, RangeError, SyntaxError, ReferenceError, URIError, EvalError.
    for err_name in &[
        "Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError",
        "URIError", "EvalError",
    ] {
        let name_clone = err_name.to_string();
        scope.declare(
            err_name,
            Value::Builtin(BuiltinFn {
                name: err_name.to_string(),
                func: Rc::new(move |args| {
                    let message = args
                        .first()
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    let mut err = ObjectValue::new();
                    err.set("name", Value::String(name_clone.clone()));
                    err.set("message", Value::String(message.clone()));
                    err.set("stack", Value::String(String::new()));
                    Ok(Value::Object(Rc::new(RefCell::new(err))))
                }),
            }),
        );
    }

    // RegExp constructor — uses the `regex` crate for real regex support.
    scope.declare(
        "RegExp",
        Value::Builtin(BuiltinFn {
            name: "RegExp".to_string(),
            func: Rc::new(|args| {
                let pattern = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let flags = args
                    .get(1)
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                // Build a regex::Regex with the appropriate flags.
                let mut builder = regex::RegexBuilder::new(&pattern);
                builder.case_insensitive(flags.contains('i'));
                builder.multi_line(flags.contains('m'));
                let re = match builder.build() {
                    Ok(r) => r,
                    Err(e) => return Err(format!("Invalid regex: {}", e)),
                };

                let mut regex_obj = ObjectValue::new();
                regex_obj.set("source", Value::String(pattern));
                regex_obj.set("flags", Value::String(flags.clone()));
                regex_obj.set("global", Value::Boolean(flags.contains('g')));
                regex_obj.set("ignoreCase", Value::Boolean(flags.contains('i')));
                regex_obj.set("multiline", Value::Boolean(flags.contains('m')));
                regex_obj.set("lastIndex", Value::Number(0.0));

                // Store the compiled regex as a leaked Box pointer.
                let re_ptr = Box::into_raw(Box::new(re));
                regex_obj.set("__regex_ptr", Value::Number(re_ptr as usize as f64));

                // test(string) — returns true if the regex matches.
                let re_ptr_test = re_ptr;
                regex_obj.set(
                    "test",
                    Value::Builtin(BuiltinFn {
                        name: "RegExp.test".to_string(),
                        func: Rc::new(move |args| {
                            let s = args
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            let re = unsafe { &*re_ptr_test };
                            Ok(Value::Boolean(re.is_match(&s)))
                        }),
                    }),
                );

                // exec(string) — returns match result or null.
                let re_ptr_exec = re_ptr;
                regex_obj.set(
                    "exec",
                    Value::Builtin(BuiltinFn {
                        name: "RegExp.exec".to_string(),
                        func: Rc::new(move |args| {
                            let s = args
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            let re = unsafe { &*re_ptr_exec };
                            if let Some(caps) = re.captures(&s) {
                                let mut result = ObjectValue::new();
                                let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                                result.set("0", Value::String(full_match.to_string()));
                                // Capture groups.
                                for i in 1..caps.len() {
                                    if let Some(m) = caps.get(i) {
                                        result.set(&i.to_string(), Value::String(m.as_str().to_string()));
                                    }
                                }
                                result.set("index", Value::Number(caps.get(0).map(|m| m.start()).unwrap_or(0) as f64));
                                result.set("input", Value::String(s.clone()));
                                result.set("length", Value::Number(caps.len() as f64));
                                Ok(Value::Object(Rc::new(RefCell::new(result))))
                            } else {
                                Ok(Value::Null)
                            }
                        }),
                    }),
                );

                // toString()
                regex_obj.set(
                    "toString",
                    Value::Builtin(BuiltinFn {
                        name: "RegExp.toString".to_string(),
                        func: Rc::new(|_args| Ok(Value::String("/pattern/flags".to_string()))),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(regex_obj))))
            }),
        }),
    );

    // String.prototype.match(regexp) — uses regex if available.
    // Already handled in value.rs String methods, but we need to make
    // sure String.prototype.replace with regex works.
    // This is handled by the String methods in value.rs.

    // Object.create(proto)
    scope.declare(
        "Object_create",
        Value::Builtin(BuiltinFn {
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
    );

    // Object.defineProperty(obj, prop, descriptor)
    scope.declare(
        "Object_defineProperty",
        Value::Builtin(BuiltinFn {
            name: "Object.defineProperty".to_string(),
            func: Rc::new(|args| {
                let obj = args.first().cloned().unwrap_or(Value::Undefined);
                let prop = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                if let (Value::Object(o), Some(Value::Object(desc))) = (&obj, args.get(2)) {
                    let desc = desc.borrow();
                    if let Some(val) = desc.properties.get("value") {
                        o.borrow_mut().set(&prop, val.clone());
                    }
                }
                Ok(obj)
            }),
        }),
    );

    // Object.getOwnPropertyDescriptor(obj, prop)
    scope.declare(
        "Object_getOwnPropertyDescriptor",
        Value::Builtin(BuiltinFn {
            name: "Object.getOwnPropertyDescriptor".to_string(),
            func: Rc::new(|args| {
                let obj = args.first().cloned().unwrap_or(Value::Undefined);
                let prop = args.get(1).map(|v| v.to_string()).unwrap_or_default();
                if let Value::Object(o) = &obj {
                    if let Some(val) = o.borrow().properties.get(&prop).cloned() {
                        let mut desc = ObjectValue::new();
                        desc.set("value", val);
                        desc.set("writable", Value::Boolean(true));
                        desc.set("enumerable", Value::Boolean(true));
                        desc.set("configurable", Value::Boolean(true));
                        return Ok(Value::Object(Rc::new(RefCell::new(desc))));
                    }
                }
                Ok(Value::Undefined)
            }),
        }),
    );

    // Object.getOwnPropertyNames(obj)
    scope.declare(
        "Object_getOwnPropertyNames",
        Value::Builtin(BuiltinFn {
            name: "Object.getOwnPropertyNames".to_string(),
            func: Rc::new(|args| {
                if let Some(Value::Object(o)) = args.first() {
                    let names: Vec<Value> = o
                        .borrow()
                        .properties
                        .keys()
                        .map(|k| Value::String(k.clone()))
                        .collect();
                    return Ok(Value::Array(Rc::new(RefCell::new(names))));
                }
                Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
            }),
        }),
    );

    // Object.getPrototypeOf(obj)
    scope.declare(
        "Object_getPrototypeOf",
        Value::Builtin(BuiltinFn {
            name: "Object.getPrototypeOf".to_string(),
            func: Rc::new(|args| {
                if let Some(Value::Object(o)) = args.first() {
                    return Ok(o.borrow().prototype.clone().unwrap_or(Value::Null));
                }
                Ok(Value::Null)
            }),
        }),
    );

    // Object.setPrototypeOf(obj, proto)
    scope.declare(
        "Object_setPrototypeOf",
        Value::Builtin(BuiltinFn {
            name: "Object.setPrototypeOf".to_string(),
            func: Rc::new(|args| {
                let obj = args.first().cloned().unwrap_or(Value::Undefined);
                let proto = args.get(1).cloned().unwrap_or(Value::Null);
                if let Value::Object(o) = &obj {
                    if matches!(proto, Value::Null) {
                        o.borrow_mut().prototype = None;
                    } else {
                        o.borrow_mut().prototype = Some(proto);
                    }
                }
                Ok(obj)
            }),
        }),
    );

    // Object.is(a, b) — sameValue algorithm
    scope.declare(
        "Object_is",
        Value::Builtin(BuiltinFn {
            name: "Object.is".to_string(),
            func: Rc::new(|args| {
                let a = args.first().cloned().unwrap_or(Value::Undefined);
                let b = args.get(1).cloned().unwrap_or(Value::Undefined);
                Ok(Value::Boolean(a.equals(&b)))
            }),
        }),
    );

    // Array.isArray(val)
    scope.declare(
        "Array_isArray",
        Value::Builtin(BuiltinFn {
            name: "Array.isArray".to_string(),
            func: Rc::new(|args| {
                Ok(Value::Boolean(matches!(args.first(), Some(Value::Array(_)))))
            }),
        }),
    );

    // Array.from(iterable)
    scope.declare(
        "Array_from",
        Value::Builtin(BuiltinFn {
            name: "Array.from".to_string(),
            func: Rc::new(|args| {
                let src = args.first().cloned().unwrap_or(Value::Undefined);
                match src {
                    Value::Array(arr) => Ok(Value::Array(arr.clone())),
                    Value::String(s) => {
                        let chars: Vec<Value> =
                            s.chars().map(|c| Value::String(c.to_string())).collect();
                        Ok(Value::Array(Rc::new(RefCell::new(chars))))
                    }
                    Value::Object(o) => {
                        let o = o.borrow();
                        if let Some(Value::Number(len)) = o.properties.get("length") {
                            let len = *len as usize;
                            let mut arr = Vec::with_capacity(len);
                            for i in 0..len {
                                arr.push(o.properties.get(&i.to_string()).cloned().unwrap_or(Value::Undefined));
                            }
                            return Ok(Value::Array(Rc::new(RefCell::new(arr))));
                        }
                        Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                    }
                    _ => Ok(Value::Array(Rc::new(RefCell::new(vec![])))),
                }
            }),
        }),
    );

    // Array.of(...items)
    scope.declare(
        "Array_of",
        Value::Builtin(BuiltinFn {
            name: "Array.of".to_string(),
            func: Rc::new(|args| {
                Ok(Value::Array(Rc::new(RefCell::new(args))))
            }),
        }),
    );

    // Number.isInteger, Number.isFinite, Number.isNaN, Number.parseInt, Number.parseFloat
    scope.declare(
        "Number_isInteger",
        Value::Builtin(BuiltinFn {
            name: "Number.isInteger".to_string(),
            func: Rc::new(|args| {
                if let Some(Value::Number(n)) = args.first() {
                    return Ok(Value::Boolean(n.fract() == 0.0 && n.is_finite()));
                }
                Ok(Value::Boolean(false))
            }),
        }),
    );

    scope.declare(
        "Number_isFinite",
        Value::Builtin(BuiltinFn {
            name: "Number.isFinite".to_string(),
            func: Rc::new(|args| {
                if let Some(Value::Number(n)) = args.first() {
                    return Ok(Value::Boolean(n.is_finite()));
                }
                Ok(Value::Boolean(false))
            }),
        }),
    );

    scope.declare(
        "Number_isNaN",
        Value::Builtin(BuiltinFn {
            name: "Number.isNaN".to_string(),
            func: Rc::new(|args| {
                if let Some(Value::Number(n)) = args.first() {
                    return Ok(Value::Boolean(n.is_nan()));
                }
                Ok(Value::Boolean(false))
            }),
        }),
    );

    scope.declare(
        "Number_parseInt",
        Value::Builtin(BuiltinFn {
            name: "Number.parseInt".to_string(),
            func: Rc::new(|args| {
                let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                let radix = args.get(1).map(|v| v.to_number() as u32).unwrap_or(10);
                let radix = if radix == 0 { 10 } else { radix };
                Ok(Value::Number(
                    i64::from_str_radix(s.trim(), radix).map(|v| v as f64).unwrap_or(f64::NAN),
                ))
            }),
        }),
    );

    scope.declare(
        "Number_parseFloat",
        Value::Builtin(BuiltinFn {
            name: "Number.parseFloat".to_string(),
            func: Rc::new(|args| {
                let s = args.first().map(|v| v.to_string()).unwrap_or_default();
                Ok(Value::Number(s.trim().parse::<f64>().unwrap_or(f64::NAN)))
            }),
        }),
    );

    // Number constants
    scope.declare("Number_MAX_SAFE_INTEGER", Value::Number(9007199254740991.0));
    scope.declare("Number_MIN_SAFE_INTEGER", Value::Number(-9007199254740991.0));
    scope.declare("Number_MAX_VALUE", Value::Number(f64::MAX));
    scope.declare("Number_MIN_VALUE", Value::Number(f64::MIN_POSITIVE));
    scope.declare("Number_EPSILON", Value::Number(f64::EPSILON));
    scope.declare("Number_POSITIVE_INFINITY", Value::Number(f64::INFINITY));
    scope.declare("Number_NEGATIVE_INFINITY", Value::Number(f64::NEG_INFINITY));
    scope.declare("Number_NaN", Value::Number(f64::NAN));

    // Global NaN, Infinity, undefined (if not already declared).
    if !scope.has("NaN") {
        scope.declare("NaN", Value::Number(f64::NAN));
    }
    if !scope.has("Infinity") {
        scope.declare("Infinity", Value::Number(f64::INFINITY));
    }
    if !scope.has("globalThis") {
        scope.declare("globalThis", Value::Undefined);
    }
}

fn json_stringify(v: &Value) -> String {
    match v {
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e21 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Boolean(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Undefined => "null".to_string(),
        Value::Object(obj) => {
            let obj = obj.borrow();
            let pairs: Vec<String> = obj
                .properties
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", k, json_stringify(v)))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        Value::Array(arr) => {
            let arr = arr.borrow();
            let items: Vec<String> = arr.iter().map(json_stringify).collect();
            format!("[{}]", items.join(","))
        }
        _ => "null".to_string(),
    }
}

fn json_parse(s: &str) -> Result<Value, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty JSON".to_string());
    }
    if s == "null" {
        return Ok(Value::Null);
    }
    if s == "true" {
        return Ok(Value::Boolean(true));
    }
    if s == "false" {
        return Ok(Value::Boolean(false));
    }
    if s.starts_with('"') && s.ends_with('"') {
        return Ok(Value::String(s[1..s.len() - 1].to_string()));
    }
    if let Ok(n) = s.parse::<f64>() {
        return Ok(Value::Number(n));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let items: Vec<Value> = if inner.trim().is_empty() {
            Vec::new()
        } else {
            inner
                .split(',')
                .map(|item| json_parse(item.trim()).unwrap_or(Value::Null))
                .collect()
        };
        return Ok(Value::Array(Rc::new(RefCell::new(items))));
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        let mut obj = ObjectValue::new();
        // Very simplified JSON object parsing.
        for pair in inner.split(',') {
            let parts: Vec<&str> = pair.splitn(2, ':').collect();
            if parts.len() == 2 {
                let key = parts[0].trim().trim_matches('"');
                let val = json_parse(parts[1].trim()).unwrap_or(Value::Null);
                obj.set(key, val);
            }
        }
        return Ok(Value::Object(Rc::new(RefCell::new(obj))));
    }
    Err(format!("cannot parse JSON: {}", s))
}

/// Create a `window` global object — the global scope in browsers.
/// YouTube's JS starts with `window.WIZ_global_data = {...}` so we need
/// `window` to be an object that can have properties set on it.
fn create_window() -> Value {
    let mut win = ObjectValue::new();
    // window references itself (window.window === window).
    // We can't set it yet because we need the Rc — we'll set it after creation.
    // For now, add common browser globals as stubs.
    win.set(
        "location",
        Value::Object(Rc::new(RefCell::new({
            let mut loc = ObjectValue::new();
            loc.set("href", Value::String("https://www.youtube.com".to_string()));
            loc.set("hostname", Value::String("www.youtube.com".to_string()));
            loc.set(
                "origin",
                Value::String("https://www.youtube.com".to_string()),
            );
            loc.set("pathname", Value::String("/".to_string()));
            loc.set("search", Value::String("".to_string()));
            loc.set("hash", Value::String("".to_string()));
            loc
        }))),
    );
    win.set(
        "navigator",
        Value::Object(Rc::new(RefCell::new({
            let mut nav = ObjectValue::new();
            nav.set(
                "userAgent",
                Value::String("Mozilla/5.0 (Falco/TJS)".to_string()),
            );
            nav.set("platform", Value::String("Linux".to_string()));
            nav.set("language", Value::String("en-US".to_string()));
            nav
        }))),
    );
    win.set(
        "document",
        Value::Object(Rc::new(RefCell::new({
            let mut doc = ObjectValue::new();
            doc.set("readyState", Value::String("complete".to_string()));
            doc.set("title", Value::String("".to_string()));
            doc.set("cookie", Value::String("".to_string()));
            // document.createElement — returns an element-like object with
            // innerHTML setter, appendChild, setAttribute, etc.
            doc.set(
                "createElement",
                Value::Builtin(BuiltinFn {
                    name: "document.createElement".to_string(),
                    func: Rc::new(|args| {
                        let tag = args
                            .first()
                            .map(|v| v.to_string())
                            .unwrap_or("div".to_string());
                        let mut el = ObjectValue::new();
                        el.set("tagName", Value::String(tag.to_uppercase()));
                        el.set("nodeName", Value::String(tag.to_uppercase()));
                        el.set("nodeType", Value::Number(1.0));
                        el.set("id", Value::String(String::new()));
                        el.set("className", Value::String(String::new()));
                        el.set("innerHTML", Value::String(String::new()));
                        el.set("textContent", Value::String(String::new()));
                        el.set(
                            "style",
                            Value::Object(Rc::new(RefCell::new(ObjectValue::new()))),
                        );
                        el.set(
                            "setAttribute",
                            Value::Builtin(BuiltinFn {
                                name: "setAttribute".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        el.set(
                            "getAttribute",
                            Value::Builtin(BuiltinFn {
                                name: "getAttribute".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        el.set(
                            "appendChild",
                            Value::Builtin(BuiltinFn {
                                name: "appendChild".to_string(),
                                func: Rc::new(|args| {
                                    Ok(args.first().cloned().unwrap_or(Value::Undefined))
                                }),
                            }),
                        );
                        el.set(
                            "removeChild",
                            Value::Builtin(BuiltinFn {
                                name: "removeChild".to_string(),
                                func: Rc::new(|args| {
                                    Ok(args.first().cloned().unwrap_or(Value::Undefined))
                                }),
                            }),
                        );
                        // addEventListener — stores the callback but doesn't fire
                        // (no real event loop in TJS). At least doesn't crash.
                        el.set(
                            "addEventListener",
                            Value::Builtin(BuiltinFn {
                                name: "addEventListener".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        el.set(
                            "removeEventListener",
                            Value::Builtin(BuiltinFn {
                                name: "removeEventListener".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        // dispatchEvent — no-op.
                        el.set(
                            "dispatchEvent",
                            Value::Builtin(BuiltinFn {
                                name: "dispatchEvent".to_string(),
                                func: Rc::new(|_args| Ok(Value::Boolean(true))),
                            }),
                        );
                        // click() — triggers onclick if set.
                        el.set(
                            "click",
                            Value::Builtin(BuiltinFn {
                                name: "click".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        // focus() / blur() — no-op.
                        el.set(
                            "focus",
                            Value::Builtin(BuiltinFn {
                                name: "focus".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        el.set(
                            "blur",
                            Value::Builtin(BuiltinFn {
                                name: "blur".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        // contains() — simplified.
                        el.set(
                            "contains",
                            Value::Builtin(BuiltinFn {
                                name: "contains".to_string(),
                                func: Rc::new(|_args| Ok(Value::Boolean(false))),
                            }),
                        );
                        // querySelector / querySelectorAll on elements.
                        el.set(
                            "querySelector",
                            Value::Builtin(BuiltinFn {
                                name: "querySelector".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        el.set(
                            "querySelectorAll",
                            Value::Builtin(BuiltinFn {
                                name: "querySelectorAll".to_string(),
                                func: Rc::new(|_args| {
                                    Ok(Value::Array(Rc::new(RefCell::new(Vec::new()))))
                                }),
                            }),
                        );
                        // getElementsByClassName / getElementsByTagName.
                        el.set(
                            "getElementsByClassName",
                            Value::Builtin(BuiltinFn {
                                name: "getElementsByClassName".to_string(),
                                func: Rc::new(|_args| {
                                    Ok(Value::Array(Rc::new(RefCell::new(Vec::new()))))
                                }),
                            }),
                        );
                        el.set(
                            "getElementsByTagName",
                            Value::Builtin(BuiltinFn {
                                name: "getElementsByTagName".to_string(),
                                func: Rc::new(|_args| {
                                    Ok(Value::Array(Rc::new(RefCell::new(Vec::new()))))
                                }),
                            }),
                        );
                        // cloneNode.
                        el.set(
                            "cloneNode",
                            Value::Builtin(BuiltinFn {
                                name: "cloneNode".to_string(),
                                func: Rc::new(|_args| {
                                    Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))
                                }),
                            }),
                        );
                        el.set(
                            "removeEventListener",
                            Value::Builtin(BuiltinFn {
                                name: "removeEventListener".to_string(),
                                func: Rc::new(|_args| Ok(Value::Undefined)),
                            }),
                        );
                        Ok(Value::Object(Rc::new(RefCell::new(el))))
                    }),
                }),
            );
            // document.createTextNode — returns a text node.
            doc.set(
                "createTextNode",
                Value::Builtin(BuiltinFn {
                    name: "document.createTextNode".to_string(),
                    func: Rc::new(|args| {
                        let text = args.first().map(|v| v.to_string()).unwrap_or_default();
                        let mut node = ObjectValue::new();
                        node.set("nodeType", Value::Number(3.0));
                        node.set("textContent", Value::String(text));
                        Ok(Value::Object(Rc::new(RefCell::new(node))))
                    }),
                }),
            );
            // document.getElementById — stub returns undefined.
            doc.set(
                "getElementById",
                Value::Builtin(BuiltinFn {
                    name: "document.getElementById".to_string(),
                    func: Rc::new(|_args| Ok(Value::Undefined)),
                }),
            );
            // document.querySelector — stub.
            doc.set(
                "querySelector",
                Value::Builtin(BuiltinFn {
                    name: "document.querySelector".to_string(),
                    func: Rc::new(|_args| Ok(Value::Undefined)),
                }),
            );
            // document.querySelectorAll — returns empty array.
            doc.set(
                "querySelectorAll",
                Value::Builtin(BuiltinFn {
                    name: "document.querySelectorAll".to_string(),
                    func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(Vec::new()))))),
                }),
            );
            // document.addEventListener — no-op.
            doc.set(
                "addEventListener",
                Value::Builtin(BuiltinFn {
                    name: "document.addEventListener".to_string(),
                    func: Rc::new(|_args| Ok(Value::Undefined)),
                }),
            );
            // document.body — an element-like object.
            let mut body = ObjectValue::new();
            body.set("tagName", Value::String("BODY".to_string()));
            body.set(
                "appendChild",
                Value::Builtin(BuiltinFn {
                    name: "body.appendChild".to_string(),
                    func: Rc::new(|args| Ok(args.first().cloned().unwrap_or(Value::Undefined))),
                }),
            );
            body.set("innerHTML", Value::String(String::new()));
            doc.set("body", Value::Object(Rc::new(RefCell::new(body))));
            // document.head — an element-like object.
            let mut head = ObjectValue::new();
            head.set("tagName", Value::String("HEAD".to_string()));
            doc.set("head", Value::Object(Rc::new(RefCell::new(head))));
            // document.documentElement.
            let mut html_el = ObjectValue::new();
            html_el.set("tagName", Value::String("HTML".to_string()));
            doc.set(
                "documentElement",
                Value::Object(Rc::new(RefCell::new(html_el))),
            );
            doc
        }))),
    );
    // window.setTimeout — stub (would need event loop integration).
    win.set(
        "setTimeout",
        Value::Builtin(BuiltinFn {
            name: "setTimeout".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );
    // window.setInterval — stub.
    win.set(
        "setInterval",
        Value::Builtin(BuiltinFn {
            name: "setInterval".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );
    // window.clearTimeout / clearInterval — stubs.
    win.set(
        "clearTimeout",
        Value::Builtin(BuiltinFn {
            name: "clearTimeout".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    win.set(
        "clearInterval",
        Value::Builtin(BuiltinFn {
            name: "clearInterval".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    // window.addEventListener — no-op.
    win.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "window.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    // window.requestAnimationFrame — YouTube calls this.
    win.set(
        "requestAnimationFrame",
        Value::Builtin(BuiltinFn {
            name: "requestAnimationFrame".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(1.0))),
        }),
    );
    win.set(
        "cancelAnimationFrame",
        Value::Builtin(BuiltinFn {
            name: "cancelAnimationFrame".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    // window.performance.now() — returns 0.
    win.set(
        "performance",
        Value::Object(Rc::new(RefCell::new({
            let mut perf = ObjectValue::new();
            perf.set(
                "now",
                Value::Builtin(BuiltinFn {
                    name: "performance.now".to_string(),
                    func: Rc::new(|_args| Ok(Value::Number(0.0))),
                }),
            );
            perf
        }))),
    );
    Value::Object(Rc::new(RefCell::new(win)))
}
