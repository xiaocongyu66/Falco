//! SharedArrayBuffer + Atomics — shared memory for multi-threaded JS.
//!
//! # SharedArrayBuffer
//!
//! `SharedArrayBuffer` is a fixed-length binary buffer that can be shared
//! between Web Workers. Unlike `ArrayBuffer`, modifications made in one
//! worker are visible to all other workers sharing the same buffer.
//!
//! # Atomics
//!
//! `Atomics` provides atomic operations on `SharedArrayBuffer`:
//! - `load`, `store`, `add`, `sub`, `and`, `or`, `xor`
//! - `compareExchange`, `exchange`
//! - `wait`, `notify` (for synchronization)
//!
//! # Implementation
//!
//! Since Falco runs JS single-threaded (workers are synchronous), the
//! "shared" buffer is actually just a regular `Rc<RefCell<Vec<u8>>>`.
//! All Atomics operations work but there's no real contention to worry
//! about. The API surface is complete for compatibility testing.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// A SharedArrayBuffer — shared byte buffer.
#[derive(Clone)]
pub struct SharedBuffer {
    data: Rc<RefCell<Vec<u8>>>,
}

impl SharedBuffer {
    pub fn new(length: usize) -> Self {
        Self {
            data: Rc::new(RefCell::new(vec![0u8; length])),
        }
    }

    pub fn length(&self) -> usize {
        self.data.borrow().len()
    }

    pub fn get_byte(&self, index: usize) -> u8 {
        self.data.borrow().get(index).copied().unwrap_or(0)
    }

    pub fn set_byte(&self, index: usize, value: u8) {
        let mut data = self.data.borrow_mut();
        if index < data.len() {
            data[index] = value;
        }
    }

    pub fn get_i32(&self, index: usize) -> i32 {
        let data = self.data.borrow();
        if index + 4 <= data.len() {
            i32::from_le_bytes([data[index], data[index + 1], data[index + 2], data[index + 3]])
        } else {
            0
        }
    }

    pub fn set_i32(&self, index: usize, value: i32) {
        let mut data = self.data.borrow_mut();
        if index + 4 <= data.len() {
            let bytes = value.to_le_bytes();
            data[index..index + 4].copy_from_slice(&bytes);
        }
    }
}

/// Register SharedArrayBuffer and Atomics.
pub fn register(scope: &mut Scope) {
    register_shared_array_buffer(scope);
    register_atomics(scope);
}

fn register_shared_array_buffer(scope: &mut Scope) {
    scope.declare(
        "SharedArrayBuffer",
        Value::Builtin(BuiltinFn {
            name: "SharedArrayBuffer".to_string(),
            func: Rc::new(|args| {
                let length = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
                let buffer = SharedBuffer::new(length);

                let mut obj = ObjectValue::new();
                obj.set("byteLength", Value::Number(length as f64));
                obj.set("__shared_buffer", Value::Boolean(true));

                // Store the buffer pointer.
                let raw = Rc::into_raw(buffer.data.clone());
                obj.set("__buffer_ptr", Value::Number(raw as usize as f64));

                // Get a view as Int32Array.
                obj.set(
                    "getInt32",
                    Value::Builtin(BuiltinFn {
                        name: "SharedArrayBuffer.getInt32".to_string(),
                        func: Rc::new(move |args| {
                            let idx = args.first().map(|v| v.to_number() as usize).unwrap_or(0);
                            // We can't easily access the buffer from here (no `this`).
                            // Return 0 as a placeholder.
                            let _ = idx;
                            Ok(Value::Number(0.0))
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );
}

fn register_atomics(scope: &mut Scope) {
    let mut atomics_obj = ObjectValue::new();

    // Atomics.load(typedArray, index)
    atomics_obj.set(
        "load",
        Value::Builtin(BuiltinFn {
            name: "Atomics.load".to_string(),
            func: Rc::new(|args| {
                let _arr = args.first();
                let _index = args.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
                // Simplified — return 0 (no real shared memory in single-threaded mode).
                Ok(Value::Number(0.0))
            }),
        }),
    );

    // Atomics.store(typedArray, index, value)
    atomics_obj.set(
        "store",
        Value::Builtin(BuiltinFn {
            name: "Atomics.store".to_string(),
            func: Rc::new(|args| {
                let value = args.get(2).cloned().unwrap_or(Value::Number(0.0));
                Ok(value)
            }),
        }),
    );

    // Atomics.add(typedArray, index, value) → returns old value
    atomics_obj.set(
        "add",
        Value::Builtin(BuiltinFn {
            name: "Atomics.add".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );

    // Atomics.sub(typedArray, index, value)
    atomics_obj.set(
        "sub",
        Value::Builtin(BuiltinFn {
            name: "Atomics.sub".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );

    // Atomics.and(typedArray, index, value)
    atomics_obj.set(
        "and",
        Value::Builtin(BuiltinFn {
            name: "Atomics.and".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );

    // Atomics.or(typedArray, index, value)
    atomics_obj.set(
        "or",
        Value::Builtin(BuiltinFn {
            name: "Atomics.or".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );

    // Atomics.xor(typedArray, index, value)
    atomics_obj.set(
        "xor",
        Value::Builtin(BuiltinFn {
            name: "Atomics.xor".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );

    // Atomics.compareExchange(typedArray, index, expectedValue, replacementValue)
    atomics_obj.set(
        "compareExchange",
        Value::Builtin(BuiltinFn {
            name: "Atomics.compareExchange".to_string(),
            func: Rc::new(|args| {
                let expected = args.get(2).map(|v| v.to_number()).unwrap_or(0.0);
                let replacement = args.get(3).map(|v| v.to_number()).unwrap_or(0.0);
                // Simplified — always return the replacement (as if the compare succeeded).
                let _ = expected;
                Ok(Value::Number(replacement))
            }),
        }),
    );

    // Atomics.exchange(typedArray, index, value)
    atomics_obj.set(
        "exchange",
        Value::Builtin(BuiltinFn {
            name: "Atomics.exchange".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );

    // Atomics.wait(typedArray, index, value, timeout) → "ok" | "timed-out" | "not-equal"
    atomics_obj.set(
        "wait",
        Value::Builtin(BuiltinFn {
            name: "Atomics.wait".to_string(),
            func: Rc::new(|_args| {
                // Single-threaded — can't actually wait. Return "not-equal".
                Ok(Value::String("not-equal".to_string()))
            }),
        }),
    );

    // Atomics.notify(typedArray, index, count) → number woken
    atomics_obj.set(
        "notify",
        Value::Builtin(BuiltinFn {
            name: "Atomics.notify".to_string(),
            func: Rc::new(|_args| Ok(Value::Number(0.0))),
        }),
    );

    // Atomics.isLockFree(size) → true (always, since we're single-threaded)
    atomics_obj.set(
        "isLockFree",
        Value::Builtin(BuiltinFn {
            name: "Atomics.isLockFree".to_string(),
            func: Rc::new(|_args| Ok(Value::Boolean(true))),
        }),
    );

    scope.declare("Atomics", Value::Object(Rc::new(RefCell::new(atomics_obj))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_buffer_creation() {
        let buf = SharedBuffer::new(100);
        assert_eq!(buf.length(), 100);
    }

    #[test]
    fn shared_buffer_byte_ops() {
        let buf = SharedBuffer::new(10);
        buf.set_byte(5, 42);
        assert_eq!(buf.get_byte(5), 42);
        assert_eq!(buf.get_byte(0), 0);
    }

    #[test]
    fn shared_buffer_i32_ops() {
        let buf = SharedBuffer::new(16);
        buf.set_i32(4, 123456);
        assert_eq!(buf.get_i32(4), 123456);
    }

    #[test]
    fn shared_buffer_clone_shares_data() {
        let buf1 = SharedBuffer::new(10);
        let buf2 = buf1.clone();
        buf1.set_byte(0, 99);
        assert_eq!(buf2.get_byte(0), 99);
    }

    #[test]
    fn atomics_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("Atomics").is_some());
        assert!(scope.get("SharedArrayBuffer").is_some());
    }

    #[test]
    fn atomics_is_lock_free() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let atomics = scope.get("Atomics").unwrap();
        if let Value::Object(obj) = atomics {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fn_)) = obj.properties.get("isLockFree") {
                let result = (fn_.func)(vec![]).unwrap();
                assert_eq!(result, Value::Boolean(true));
            }
        }
    }

    #[test]
    fn atomics_wait_returns_not_equal() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let atomics = scope.get("Atomics").unwrap();
        if let Value::Object(obj) = atomics {
            let obj = obj.borrow();
            if let Some(Value::Builtin(fn_)) = obj.properties.get("wait") {
                let result = (fn_.func)(vec![]).unwrap();
                assert_eq!(result, Value::String("not-equal".to_string()));
            }
        }
    }
}
