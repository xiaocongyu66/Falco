//! IndexedDB — async key-value object store.
//!
//! # Implementation
//!
//! True IndexedDB is a complex asynchronous transactional database. This
//! implementation provides a simplified synchronous version that exposes
//! the same API shape (databases, object stores, transactions, requests)
//! but executes operations immediately.
//!
//! Data is stored in-memory (backed by a `HashMap`) and lost when the
//! process exits. A future version could persist to disk.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// An IndexedDB database.
struct Database {
    name: String,
    version: u32,
    stores: HashMap<String, Rc<RefCell<ObjectStore>>>,
}

/// An object store within a database.
struct ObjectStore {
    name: String,
    records: Vec<(Value, Value)>, // (key, value) pairs
    key_path: Option<String>,
    auto_increment: bool,
    next_key: f64,
}

/// Global registry of databases, keyed by name.
thread_local! {
    static DATABASES: RefCell<HashMap<String, Rc<RefCell<Database>>>> =
        RefCell::new(HashMap::new());
}

/// Register the IndexedDB API.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "indexedDB",
        Value::Object(Rc::new(RefCell::new({
            let mut idb = ObjectValue::new();

            // indexedDB.open(name, version) — opens (or creates) a database.
            idb.set(
                "open",
                Value::Builtin(BuiltinFn {
                    name: "indexedDB.open".to_string(),
                    func: Rc::new(|args| {
                        let name = args
                            .first()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        let version = args
                            .get(1)
                            .map(|v| v.to_number() as u32)
                            .unwrap_or(1);

                        let db = DATABASES.with(|dbs| {
                            let mut dbs = dbs.borrow_mut();
                            dbs.entry(name.clone())
                                .or_insert_with(|| {
                                    Rc::new(RefCell::new(Database {
                                        name: name.clone(),
                                        version,
                                        stores: HashMap::new(),
                                    }))
                                })
                                .clone()
                        });

                        // Create a request object.
                        let mut request = ObjectValue::new();
                        request.set("result", make_db_wrapper(db));
                        request.set("error", Value::Null);
                        request.set("readyState", Value::String("done".to_string()));
                        // onsuccess / onerror are set by the user.
                        Ok(Value::Object(Rc::new(RefCell::new(request))))
                    }),
                }),
            );

            // indexedDB.deleteDatabase(name)
            idb.set(
                "deleteDatabase",
                Value::Builtin(BuiltinFn {
                    name: "indexedDB.deleteDatabase".to_string(),
                    func: Rc::new(|args| {
                        let name = args
                            .first()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        DATABASES.with(|dbs| {
                            dbs.borrow_mut().remove(&name);
                        });
                        let mut request = ObjectValue::new();
                        request.set("result", Value::Undefined);
                        request.set("readyState", Value::String("done".to_string()));
                        Ok(Value::Object(Rc::new(RefCell::new(request))))
                    }),
                }),
            );

            // indexedDB.cmp(a, b) — compares two keys.
            idb.set(
                "cmp",
                Value::Builtin(BuiltinFn {
                    name: "indexedDB.cmp".to_string(),
                    func: Rc::new(|args| {
                        let a = args.first().map(|v| v.to_number()).unwrap_or(0.0);
                        let b = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
                        Ok(Value::Number(if a < b { -1.0 } else if a > b { 1.0 } else { 0.0 }))
                    }),
                }),
            );

            idb
        }))),
    );
}

fn make_db_wrapper(db: Rc<RefCell<Database>>) -> Value {
    let mut obj = ObjectValue::new();
    obj.set("name", Value::String(db.borrow().name.clone()));
    obj.set("version", Value::Number(db.borrow().version as f64));
    obj.set(
        "objectStoreNames",
        Value::Array(Rc::new(RefCell::new(
            db.borrow()
                .stores
                .keys()
                .map(|k| Value::String(k.clone()))
                .collect(),
        ))),
    );

    let db_for_tx = db.clone();
    obj.set(
        "transaction",
        Value::Builtin(BuiltinFn {
            name: "IDBDatabase.transaction".to_string(),
            func: Rc::new(move |args| {
                let store_names: Vec<String> = match args.first() {
                    Some(Value::String(s)) => vec![s.clone()],
                    Some(Value::Array(arr)) => arr
                        .borrow()
                        .iter()
                        .map(|v| v.to_string())
                        .collect(),
                    _ => vec![],
                };
                make_transaction(db_for_tx.clone(), store_names)
            }),
        }),
    );

    let db_for_create = db.clone();
    obj.set(
        "createObjectStore",
        Value::Builtin(BuiltinFn {
            name: "IDBDatabase.createObjectStore".to_string(),
            func: Rc::new(move |args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);
                let key_path = if let Value::Object(o) = &options {
                    o.borrow()
                        .properties
                        .get("keyPath")
                        .map(|v| v.to_string())
                } else {
                    None
                };
                let auto_inc = if let Value::Object(o) = &options {
                    o.borrow()
                        .properties
                        .get("autoIncrement")
                        .map(|v| matches!(v, Value::Boolean(true)))
                        .unwrap_or(false)
                } else {
                    false
                };
                let store = Rc::new(RefCell::new(ObjectStore {
                    name: name.clone(),
                    records: Vec::new(),
                    key_path,
                    auto_increment: auto_inc,
                    next_key: 1.0,
                }));
                db_for_create
                    .borrow_mut()
                    .stores
                    .insert(name.clone(), store.clone());
                make_store_wrapper(store)
            }),
        }),
    );

    let db_for_delete = db.clone();
    obj.set(
        "deleteObjectStore",
        Value::Builtin(BuiltinFn {
            name: "IDBDatabase.deleteObjectStore".to_string(),
            func: Rc::new(move |args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                db_for_delete.borrow_mut().stores.remove(&name);
                Ok(Value::Undefined)
            }),
        }),
    );

    let db_for_close = db.clone();
    obj.set(
        "close",
        Value::Builtin(BuiltinFn {
            name: "IDBDatabase.close".to_string(),
            func: Rc::new(move |_args| {
                let _ = db_for_close.borrow();
                Ok(Value::Undefined)
            }),
        }),
    );

    Value::Object(Rc::new(RefCell::new(obj)))
}

fn make_transaction(db: Rc<RefCell<Database>>, store_names: Vec<String>) -> Result<Value, String> {
    let mut tx_obj = ObjectValue::new();
    tx_obj.set(
        "objectStoreNames",
        Value::Array(Rc::new(RefCell::new(
            store_names.iter().map(|s| Value::String(s.clone())).collect(),
        ))),
    );
    tx_obj.set("mode", Value::String("readwrite".to_string()));
    tx_obj.set("done", Value::Boolean(true));

    // transaction.objectStore(name)
    let db_for_store = db.clone();
    tx_obj.set(
        "objectStore",
        Value::Builtin(BuiltinFn {
            name: "IDBTransaction.objectStore".to_string(),
            func: Rc::new(move |args| {
                let name = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let store = db_for_store
                    .borrow()
                    .stores
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| format!("object store \"{}\" not found", name))?;
                make_store_wrapper(store)
            }),
        }),
    );

    tx_obj.set(
        "abort",
        Value::Builtin(BuiltinFn {
            name: "IDBTransaction.abort".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    tx_obj.set(
        "commit",
        Value::Builtin(BuiltinFn {
            name: "IDBTransaction.commit".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(tx_obj))))
}

fn make_store_wrapper(store: Rc<RefCell<ObjectStore>>) -> Result<Value, String> {
    let mut obj = ObjectValue::new();
    obj.set("name", Value::String(store.borrow().name.clone()));
    obj.set(
        "keyPath",
        match &store.borrow().key_path {
            Some(kp) => Value::String(kp.clone()),
            None => Value::Null,
        },
    );
    obj.set(
        "autoIncrement",
        Value::Boolean(store.borrow().auto_increment),
    );

    let store_for_get = store.clone();
    obj.set(
        "get",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.get".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().cloned().unwrap_or(Value::Undefined);
                let s = store_for_get.borrow();
                let result = s
                    .records
                    .iter()
                    .find(|(k, _)| values_equal(k, &key))
                    .map(|(_, v)| v.clone())
                    .unwrap_or(Value::Undefined);
                make_request(result)
            }),
        }),
    );

    let store_for_get_all = store.clone();
    obj.set(
        "getAll",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.getAll".to_string(),
            func: Rc::new(move |_args| {
                let s = store_for_get_all.borrow();
                let values: Vec<Value> = s.records.iter().map(|(_, v)| v.clone()).collect();
                make_request(Value::Array(Rc::new(RefCell::new(values))))
            }),
        }),
    );

    let store_for_get_keys = store.clone();
    obj.set(
        "getAllKeys",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.getAllKeys".to_string(),
            func: Rc::new(move |_args| {
                let s = store_for_get_keys.borrow();
                let keys: Vec<Value> = s.records.iter().map(|(k, _)| k.clone()).collect();
                make_request(Value::Array(Rc::new(RefCell::new(keys))))
            }),
        }),
    );

    let store_for_put = store.clone();
    obj.set(
        "put",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.put".to_string(),
            func: Rc::new(move |args| {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                let key_arg = args.get(1).cloned();

                let mut s = store_for_put.borrow_mut();
                let key = if s.auto_increment {
                    if let Some(k) = key_arg {
                        k
                    } else {
                        let k = Value::Number(s.next_key);
                        s.next_key += 1.0;
                        k
                    }
                } else if let Some(k) = key_arg {
                    k
                } else {
                    // Try to extract key from value using keyPath.
                    if let Some(kp) = &s.key_path {
                        if let Value::Object(o) = &value {
                            o.borrow().properties.get(kp).cloned().unwrap_or(Value::Undefined)
                        } else {
                            Value::Undefined
                        }
                    } else {
                        Value::Undefined
                    }
                };

                // Remove existing record with the same key (if any).
                s.records.retain(|(k, _)| !values_equal(k, &key));
                s.records.push((key.clone(), value));
                make_request(key)
            }),
        }),
    );

    let store_for_add = store.clone();
    obj.set(
        "add",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.add".to_string(),
            func: Rc::new(move |args| {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                let key_arg = args.get(1).cloned();
                let mut s = store_for_add.borrow_mut();
                let key = if s.auto_increment {
                    if let Some(k) = key_arg {
                        k
                    } else {
                        let k = Value::Number(s.next_key);
                        s.next_key += 1.0;
                        k
                    }
                } else {
                    key_arg.unwrap_or(Value::Undefined)
                };
                // Check for duplicate key.
                if s.records.iter().any(|(k, _)| values_equal(k, &key)) {
                    return Err("Key already exists in the object store".to_string());
                }
                s.records.push((key.clone(), value));
                make_request(key)
            }),
        }),
    );

    let store_for_delete = store.clone();
    obj.set(
        "delete",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.delete".to_string(),
            func: Rc::new(move |args| {
                let key = args.first().cloned().unwrap_or(Value::Undefined);
                let mut s = store_for_delete.borrow_mut();
                s.records.retain(|(k, _)| !values_equal(k, &key));
                make_request(Value::Undefined)
            }),
        }),
    );

    let store_for_clear = store.clone();
    obj.set(
        "clear",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.clear".to_string(),
            func: Rc::new(move |_args| {
                store_for_clear.borrow_mut().records.clear();
                make_request(Value::Undefined)
            }),
        }),
    );

    let store_for_count = store.clone();
    obj.set(
        "count",
        Value::Builtin(BuiltinFn {
            name: "IDBObjectStore.count".to_string(),
            func: Rc::new(move |_args| {
                let s = store_for_count.borrow();
                make_request(Value::Number(s.records.len() as f64))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(obj))))
}

/// Create an IDBRequest object with the given result.
fn make_request(result: Value) -> Result<Value, String> {
    let mut req = ObjectValue::new();
    req.set("result", result);
    req.set("error", Value::Null);
    req.set("readyState", Value::String("done".to_string()));
    req.set("source", Value::Undefined);
    Ok(Value::Object(Rc::new(RefCell::new(req))))
}

/// Compare two Values for equality (for key matching).
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexeddb_open_creates_database() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let idb = scope.get("indexedDB").unwrap();
        if let Value::Object(obj) = idb {
            let obj = obj.borrow();
            if let Some(Value::Builtin(open_fn)) = obj.properties.get("open") {
                let request = (open_fn.func)(vec![
                    Value::String("test-db".to_string()),
                    Value::Number(1.0),
                ]).unwrap();
                if let Value::Object(req) = request {
                    let req = req.borrow();
                    assert_eq!(req.properties.get("readyState"), Some(&Value::String("done".to_string())));
                    assert!(req.properties.contains_key("result"));
                }
            }
        }
    }

    #[test]
    fn indexeddb_cmp() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let idb = scope.get("indexedDB").unwrap();
        if let Value::Object(obj) = idb {
            let obj = obj.borrow();
            if let Some(Value::Builtin(cmp_fn)) = obj.properties.get("cmp") {
                let result = (cmp_fn.func)(vec![
                    Value::Number(1.0),
                    Value::Number(2.0),
                ]).unwrap();
                assert_eq!(result, Value::Number(-1.0));
            }
        }
    }

    #[test]
    fn database_create_store_and_put() {
        let mut scope = Scope::new(None);
        register(&mut scope);

        // Open a database.
        let idb = scope.get("indexedDB").unwrap();
        let db = if let Value::Object(obj) = &idb {
            let obj = obj.borrow();
            if let Some(Value::Builtin(open_fn)) = obj.properties.get("open") {
                let request = (open_fn.func)(vec![
                    Value::String("test-db-2".to_string()),
                ]).unwrap();
                if let Value::Object(req) = request {
                    req.borrow().properties.get("result").cloned().unwrap()
                } else {
                    panic!("expected request object");
                }
            } else {
                panic!("no open function");
            }
        } else {
            panic!("expected object");
        };

        // Create an object store.
        if let Value::Object(db_obj) = &db {
            let db_obj = db_obj.borrow();
            if let Some(Value::Builtin(create_fn)) = db_obj.properties.get("createObjectStore") {
                let store = (create_fn.func)(vec![
                    Value::String("items".to_string()),
                ]).unwrap();

                // Put a value.
                if let Value::Object(store_obj) = &store {
                    let store_obj = store_obj.borrow();
                    if let Some(Value::Builtin(put_fn)) = store_obj.properties.get("put") {
                        let _ = (put_fn.func)(vec![
                            Value::String("hello".to_string()),
                            Value::Number(1.0),
                        ]).unwrap();
                    }

                    // Get it back.
                    if let Some(Value::Builtin(get_fn)) = store_obj.properties.get("get") {
                        let request = (get_fn.func)(vec![Value::Number(1.0)]).unwrap();
                        if let Value::Object(req) = request {
                            let req = req.borrow();
                            assert_eq!(req.properties.get("result"), Some(&Value::String("hello".to_string())));
                        }
                    }

                    // Count.
                    if let Some(Value::Builtin(count_fn)) = store_obj.properties.get("count") {
                        let request = (count_fn.func)(vec![]).unwrap();
                        if let Value::Object(req) = request {
                            let req = req.borrow();
                            assert_eq!(req.properties.get("result"), Some(&Value::Number(1.0)));
                        }
                    }
                }
            }
        }
    }
}
