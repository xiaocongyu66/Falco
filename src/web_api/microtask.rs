//! queueMicrotask — schedule a microtask to run at the end of the current task.
//!
//! ```js
//! queueMicrotask(() => console.log("runs after the current task"));
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, Value};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    /// The global microtask queue.
    static MICROTASK_QUEUE: RefCell<Vec<Value>> = RefCell::new(Vec::new());
}

/// Register queueMicrotask.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "queueMicrotask",
        Value::Builtin(BuiltinFn {
            name: "queueMicrotask".to_string(),
            func: Rc::new(|args| {
                let callback = args
                    .first()
                    .cloned()
                    .ok_or_else(|| "queueMicrotask: missing callback".to_string())?;
                MICROTASK_QUEUE.with(|q| q.borrow_mut().push(callback));
                Ok(Value::Undefined)
            }),
        }),
    );
}

/// Drain the microtask queue, calling each callback.
///
/// Should be called by the event loop after each task completes.
pub fn drain_microtasks() {
    let callbacks: Vec<Value> = MICROTASK_QUEUE.with(|q| q.borrow_mut().drain(..).collect());
    for callback in callbacks {
        let _ = call_callback(&callback, vec![]);
    }
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
    fn queue_microtask_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("queueMicrotask").is_some());
    }

    #[test]
    fn queue_microtask_drains() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let qmt = scope.get("queueMicrotask").unwrap();
        if let Value::Builtin(b) = qmt {
            // Queue a no-op callback.
            let callback = Value::Builtin(crate::tjs::value::BuiltinFn {
                name: "noop".to_string(),
                func: Rc::new(|_args| Ok(Value::Undefined)),
            });
            let _ = (b.func)(vec![callback]).unwrap();
        }
        // Drain should not panic.
        drain_microtasks();
    }
}
