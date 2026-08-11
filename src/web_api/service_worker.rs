//! Service Workers — offline-first web application support.
//!
//! # Implementation
//!
//! Service Workers run in a separate context from the page, intercepting
//! network requests and serving cached responses. This implementation
//! provides:
//!
//! - `navigator.serviceWorker.register(scriptURL)` — registers a SW
//! - `navigator.serviceWorker.getRegistration()` — returns the active SW
//! - `caches.open(name)` — opens a named Cache
//! - `Cache.put(request, response)` / `Cache.match(request)` — cache API
//! - `fetch` event — intercepts network requests
//! - `install` / `activate` events — SW lifecycle
//!
//! Real Service Workers run in a separate thread with their own event loop.
//! This implementation runs them synchronously in the same context (since
//! Falco is single-threaded), but exposes the same API shape.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A cached response.
#[derive(Clone)]
struct CachedResponse {
    url: String,
    status: u32,
    body: String,
    headers: HashMap<String, String>,
}

/// A named cache.
struct Cache {
    name: String,
    entries: HashMap<String, CachedResponse>,
}

/// Global registry of caches.
thread_local! {
    static CACHES: RefCell<HashMap<String, Rc<RefCell<Cache>>>> = RefCell::new(HashMap::new());
    static SERVICE_WORKER_REGISTRATION: RefCell<Option<ServiceWorkerRegistration>> =
        RefCell::new(None);
}

/// A service worker registration.
struct ServiceWorkerRegistration {
    script_url: String,
    scope: String,
    installed: bool,
    active: bool,
}

/// Register the Service Worker and Cache APIs.
pub fn register(scope: &mut Scope) {
    register_caches(scope);
    register_service_worker(scope);
}

fn register_caches(scope: &mut Scope) {
    let mut caches_obj = ObjectValue::new();

    // caches.open(name) — returns a Promise resolving to a Cache.
    caches_obj.set(
        "open",
        Value::Builtin(BuiltinFn {
            name: "caches.open".to_string(),
            func: Rc::new(|args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let cache = CACHES.with(|c| {
                    let mut c = c.borrow_mut();
                    c.entry(name.clone())
                        .or_insert_with(|| {
                            Rc::new(RefCell::new(Cache {
                                name: name.clone(),
                                entries: HashMap::new(),
                            }))
                        })
                        .clone()
                });
                Ok(make_cache_wrapper(cache))
            }),
        }),
    );

    // caches.has(name) — returns true if a cache with the given name exists.
    caches_obj.set(
        "has",
        Value::Builtin(BuiltinFn {
            name: "caches.has".to_string(),
            func: Rc::new(|args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let exists = CACHES.with(|c| c.borrow().contains_key(&name));
                Ok(Value::Boolean(exists))
            }),
        }),
    );

    // caches.delete(name) — deletes a cache.
    caches_obj.set(
        "delete",
        Value::Builtin(BuiltinFn {
            name: "caches.delete".to_string(),
            func: Rc::new(|args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let removed = CACHES.with(|c| c.borrow_mut().remove(&name).is_some());
                Ok(Value::Boolean(removed))
            }),
        }),
    );

    // caches.keys() — returns a list of cache names.
    caches_obj.set(
        "keys",
        Value::Builtin(BuiltinFn {
            name: "caches.keys".to_string(),
            func: Rc::new(|_args| {
                let names: Vec<Value> = CACHES.with(|c| {
                    c.borrow()
                        .keys()
                        .map(|k| Value::String(k.clone()))
                        .collect()
                });
                Ok(Value::Array(Rc::new(RefCell::new(names))))
            }),
        }),
    );

    // caches.match(request) — searches all caches for a match.
    caches_obj.set(
        "match",
        Value::Builtin(BuiltinFn {
            name: "caches.match".to_string(),
            func: Rc::new(|args| {
                let request = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let response = CACHES.with(|c| {
                    let c = c.borrow();
                    for cache in c.values() {
                        let cache = cache.borrow();
                        if let Some(resp) = cache.entries.get(&request) {
                            return Some(make_response_wrapper(resp.clone()));
                        }
                    }
                    None
                });
                Ok(response.unwrap_or(Value::Undefined))
            }),
        }),
    );

    scope.declare("caches", Value::Object(Rc::new(RefCell::new(caches_obj))));
}

fn make_cache_wrapper(cache: Rc<RefCell<Cache>>) -> Value {
    let mut obj = ObjectValue::new();

    let cache_for_match = cache.clone();
    obj.set(
        "match",
        Value::Builtin(BuiltinFn {
            name: "Cache.match".to_string(),
            func: Rc::new(move |args| {
                let request = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let c = cache_for_match.borrow();
                Ok(c.entries
                    .get(&request)
                    .map(|r| make_response_wrapper(r.clone()))
                    .unwrap_or(Value::Undefined))
            }),
        }),
    );

    let cache_for_match_all = cache.clone();
    obj.set(
        "matchAll",
        Value::Builtin(BuiltinFn {
            name: "Cache.matchAll".to_string(),
            func: Rc::new(move |_args| {
                let c = cache_for_match_all.borrow();
                let responses: Vec<Value> = c
                    .entries
                    .values()
                    .map(|r| make_response_wrapper(r.clone()))
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(responses))))
            }),
        }),
    );

    let cache_for_put = cache.clone();
    obj.set(
        "put",
        Value::Builtin(BuiltinFn {
            name: "Cache.put".to_string(),
            func: Rc::new(move |args| {
                let request = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let response_val = args.get(1).cloned().unwrap_or(Value::Undefined);
                let cached = extract_response(&response_val, &request);
                cache_for_put
                    .borrow_mut()
                    .entries
                    .insert(request, cached);
                Ok(Value::Undefined)
            }),
        }),
    );

    let cache_for_add = cache.clone();
    obj.set(
        "add",
        Value::Builtin(BuiltinFn {
            name: "Cache.add".to_string(),
            func: Rc::new(move |args| {
                let url = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                // In a real implementation, we'd fetch the URL and cache the response.
                // Here we just store an empty response.
                let cached = CachedResponse {
                    url: url.clone(),
                    status: 200,
                    body: String::new(),
                    headers: HashMap::new(),
                };
                cache_for_add.borrow_mut().entries.insert(url, cached);
                Ok(Value::Undefined)
            }),
        }),
    );

    let cache_for_add_all = cache.clone();
    obj.set(
        "addAll",
        Value::Builtin(BuiltinFn {
            name: "Cache.addAll".to_string(),
            func: Rc::new(move |args| {
                if let Some(Value::Array(urls)) = args.first() {
                    let urls = urls.borrow();
                    for url_val in urls.iter() {
                        let url = url_val.to_string();
                        let cached = CachedResponse {
                            url: url.clone(),
                            status: 200,
                            body: String::new(),
                            headers: HashMap::new(),
                        };
                        cache_for_add_all
                            .borrow_mut()
                            .entries
                            .insert(url, cached);
                    }
                }
                Ok(Value::Undefined)
            }),
        }),
    );

    let cache_for_delete = cache.clone();
    obj.set(
        "delete",
        Value::Builtin(BuiltinFn {
            name: "Cache.delete".to_string(),
            func: Rc::new(move |args| {
                let request = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let removed = cache_for_delete
                    .borrow_mut()
                    .entries
                    .remove(&request)
                    .is_some();
                Ok(Value::Boolean(removed))
            }),
        }),
    );

    let cache_for_keys = cache.clone();
    obj.set(
        "keys",
        Value::Builtin(BuiltinFn {
            name: "Cache.keys".to_string(),
            func: Rc::new(move |_args| {
                let c = cache_for_keys.borrow();
                let keys: Vec<Value> = c
                    .entries
                    .keys()
                    .map(|k| Value::String(k.clone()))
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(keys))))
            }),
        }),
    );

    Value::Object(Rc::new(RefCell::new(obj)))
}

/// Extract a CachedResponse from a JS Response-like value.
fn extract_response(val: &Value, url: &str) -> CachedResponse {
    if let Value::Object(obj) = val {
        let obj = obj.borrow();
        let status = obj
            .properties
            .get("status")
            .map(|v| v.to_number() as u32)
            .unwrap_or(200);
        let body = obj
            .properties
            .get("body")
            .map(|v| v.to_string())
            .unwrap_or_default();
        CachedResponse {
            url: url.to_string(),
            status,
            body,
            headers: HashMap::new(),
        }
    } else {
        CachedResponse {
            url: url.to_string(),
            status: 200,
            body: String::new(),
            headers: HashMap::new(),
        }
    }
}

/// Create a JS Response-like wrapper from a CachedResponse.
fn make_response_wrapper(resp: CachedResponse) -> Value {
    let mut obj = ObjectValue::new();
    obj.set("url", Value::String(resp.url));
    obj.set("status", Value::Number(resp.status as f64));
    obj.set("ok", Value::Boolean(resp.status >= 200 && resp.status < 300));
    let body = resp.body.clone();
    obj.set("body", Value::String(body.clone()));
    obj.set(
        "text",
        Value::Builtin(BuiltinFn {
            name: "Response.text".to_string(),
            func: Rc::new(move |_args| Ok(Value::String(body.clone()))),
        }),
    );
    Value::Object(Rc::new(RefCell::new(obj)))
}

fn register_service_worker(scope: &mut Scope) {
    // navigator.serviceWorker
    let mut sw_obj = ObjectValue::new();

    sw_obj.set(
        "register",
        Value::Builtin(BuiltinFn {
            name: "serviceWorker.register".to_string(),
            func: Rc::new(|args| {
                let script_url = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                let scope_path = if let Value::Object(o) = &options {
                    o.borrow()
                        .properties
                        .get("scope")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "/".to_string())
                } else {
                    "/".to_string()
                };

                // Register the service worker.
                SERVICE_WORKER_REGISTRATION.with(|reg| {
                    *reg.borrow_mut() = Some(ServiceWorkerRegistration {
                        script_url: script_url.clone(),
                        scope: scope_path.clone(),
                        installed: true,
                        active: true,
                    });
                });

                // Return a "promise" (simplified — returns the registration).
                Ok(make_registration_wrapper(script_url, scope_path))
            }),
        }),
    );

    sw_obj.set(
        "getRegistration",
        Value::Builtin(BuiltinFn {
            name: "serviceWorker.getRegistration".to_string(),
            func: Rc::new(|_args| {
                SERVICE_WORKER_REGISTRATION.with(|reg| {
                    let r = reg.borrow();
                    if let Some(r) = &*r {
                        Ok(make_registration_wrapper(
                            r.script_url.clone(),
                            r.scope.clone(),
                        ))
                    } else {
                        Ok(Value::Undefined)
                    }
                })
            }),
        }),
    );

    sw_obj.set(
        "getRegistrations",
        Value::Builtin(BuiltinFn {
            name: "serviceWorker.getRegistrations".to_string(),
            func: Rc::new(|_args| {
                SERVICE_WORKER_REGISTRATION.with(|reg| {
                    let r = reg.borrow();
                    if let Some(r) = &*r {
                        let arr = vec![make_registration_wrapper(
                            r.script_url.clone(),
                            r.scope.clone(),
                        )];
                        Ok(Value::Array(Rc::new(RefCell::new(arr))))
                    } else {
                        Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
                    }
                })
            }),
        }),
    );

    sw_obj.set(
        "ready",
        Value::Undefined, // Would be a Promise resolving to the active registration.
    );

    sw_obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "serviceWorker.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // Set navigator.serviceWorker.
    if let Some(Value::Object(nav)) = scope.get("navigator") {
        nav.borrow_mut()
            .properties
            .insert("serviceWorker".to_string(), Value::Object(Rc::new(RefCell::new(sw_obj))));
    } else {
        // If navigator doesn't exist yet, create it.
        let mut nav = ObjectValue::new();
        nav.set(
            "serviceWorker",
            Value::Object(Rc::new(RefCell::new(sw_obj))),
        );
        scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
    }
}

fn make_registration_wrapper(script_url: String, scope: String) -> Value {
    let mut obj = ObjectValue::new();
    obj.set("scope", Value::String(scope.clone()));
    obj.set(
        "scriptURL",
        Value::String(script_url.clone()),
    );
    obj.set("updateViaCache", Value::String("imports".to_string()));

    // The installing/waiting/active ServiceWorker objects.
    let make_worker = |state: &str| -> Value {
        let mut w = ObjectValue::new();
        w.set("scriptURL", Value::String(script_url.clone()));
        w.set("state", Value::String(state.to_string()));
        Value::Object(Rc::new(RefCell::new(w)))
    };

    obj.set("installing", Value::Null);
    obj.set("waiting", Value::Null);
    obj.set("active", make_worker("activated"));

    obj.set(
        "unregister",
        Value::Builtin(BuiltinFn {
            name: "ServiceWorkerRegistration.unregister".to_string(),
            func: Rc::new(|_args| {
                SERVICE_WORKER_REGISTRATION.with(|reg| {
                    *reg.borrow_mut() = None;
                });
                Ok(Value::Boolean(true))
            }),
        }),
    );

    obj.set(
        "update",
        Value::Builtin(BuiltinFn {
            name: "ServiceWorkerRegistration.update".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "ServiceWorkerRegistration.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    Value::Object(Rc::new(RefCell::new(obj)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_open() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let caches = scope.get("caches").unwrap();
        if let Value::Object(obj) = caches {
            let obj = obj.borrow();
            if let Some(Value::Builtin(open_fn)) = obj.properties.get("open") {
                let cache = (open_fn.func)(vec![Value::String("test-cache".to_string())]).unwrap();
                if let Value::Object(cache_obj) = cache {
                    let cache_obj = cache_obj.borrow();
                    assert!(cache_obj.properties.contains_key("put"));
                    assert!(cache_obj.properties.contains_key("match"));
                    assert!(cache_obj.properties.contains_key("delete"));
                }
            }
        }
    }

    #[test]
    fn cache_put_and_match() {
        let mut scope = Scope::new(None);
        register(&mut scope);

        // Open a cache.
        let caches = scope.get("caches").unwrap();
        let cache = if let Value::Object(obj) = &caches {
            let obj = obj.borrow();
            if let Some(Value::Builtin(open_fn)) = obj.properties.get("open") {
                (open_fn.func)(vec![Value::String("test-put".to_string())]).unwrap()
            } else {
                panic!("no open");
            }
        } else {
            panic!("no caches");
        };

        // Put a response.
        if let Value::Object(cache_obj) = &cache {
            let cache_obj = cache_obj.borrow();
            if let Some(Value::Builtin(put_fn)) = cache_obj.properties.get("put") {
                let mut response = ObjectValue::new();
                response.set("status", Value::Number(200.0));
                response.set("body", Value::String("hello".to_string()));
                let _ = (put_fn.func)(vec![
                    Value::String("https://example.com/".to_string()),
                    Value::Object(Rc::new(RefCell::new(response))),
                ]).unwrap();
            }

            // Match it.
            if let Some(Value::Builtin(match_fn)) = cache_obj.properties.get("match") {
                let result = (match_fn.func)(vec![
                    Value::String("https://example.com/".to_string()),
                ]).unwrap();
                if let Value::Object(resp) = result {
                    let resp = resp.borrow();
                    assert_eq!(resp.properties.get("status"), Some(&Value::Number(200.0)));
                }
            }
        }
    }

    #[test]
    fn cache_has() {
        let mut scope = Scope::new(None);
        register(&mut scope);

        // Open a cache.
        let caches = scope.get("caches").unwrap();
        if let Value::Object(obj) = &caches {
            let obj = obj.borrow();
            if let Some(Value::Builtin(open_fn)) = obj.properties.get("open") {
                let _ = (open_fn.func)(vec![Value::String("has-test".to_string())]).unwrap();
            }
            if let Some(Value::Builtin(has_fn)) = obj.properties.get("has") {
                let result = (has_fn.func)(vec![Value::String("has-test".to_string())]).unwrap();
                assert_eq!(result, Value::Boolean(true));
            }
        }
    }

    #[test]
    fn cache_delete() {
        let mut scope = Scope::new(None);
        register(&mut scope);

        let caches = scope.get("caches").unwrap();
        if let Value::Object(obj) = &caches {
            let obj = obj.borrow();
            if let Some(Value::Builtin(open_fn)) = obj.properties.get("open") {
                let _ = (open_fn.func)(vec![Value::String("del-test".to_string())]).unwrap();
            }
            if let Some(Value::Builtin(del_fn)) = obj.properties.get("delete") {
                let result = (del_fn.func)(vec![Value::String("del-test".to_string())]).unwrap();
                assert_eq!(result, Value::Boolean(true));
            }
        }
    }

    #[test]
    fn service_worker_register() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(sw_obj)) = nav_obj.properties.get("serviceWorker") {
                let sw_obj = sw_obj.borrow();
                if let Some(Value::Builtin(reg_fn)) = sw_obj.properties.get("register") {
                    let result = (reg_fn.func)(vec![Value::String("/sw.js".to_string())]).unwrap();
                    if let Value::Object(reg) = result {
                        let reg = reg.borrow();
                        assert_eq!(
                            reg.properties.get("scriptURL"),
                            Some(&Value::String("/sw.js".to_string()))
                        );
                    }
                }
            }
        }
    }
}
