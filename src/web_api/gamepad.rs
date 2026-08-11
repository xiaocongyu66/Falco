//! Gamepad API — controller input.
//!
//! ```js
//! window.addEventListener('gamepadconnected', (e) => {
//!   const gp = navigator.getGamepads()[e.gamepad.index];
//!   gp.buttons[0].pressed;  // true/false
//!   gp.axes[0];  // -1.0 to 1.0
//! });
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Gamepad API.
pub fn register(scope: &mut Scope) {
    // Get or create navigator.
    let nav_val = scope.get("navigator");
    let mut nav = if let Some(Value::Object(nav_rc)) = nav_val {
        let n = nav_rc.borrow();
        let mut copy = ObjectValue::new();
        for (k, v) in n.properties.iter() {
            copy.properties.insert(k.clone(), v.clone());
        }
        copy.prototype = n.prototype.clone();
        copy
    } else {
        ObjectValue::new()
    };

    // navigator.getGamepads() — returns an array of gamepad objects (or null).
    nav.set(
        "getGamepads",
        Value::Builtin(BuiltinFn {
            name: "navigator.getGamepads".to_string(),
            func: Rc::new(|_args| {
                // Return 4 null slots (standard gamepad limit).
                let pads = vec![Value::Null; 4];
                Ok(Value::Array(Rc::new(RefCell::new(pads))))
            }),
        }),
    );

    scope.declare(
        "GamepadButton",
        Value::Builtin(BuiltinFn {
            name: "GamepadButton".to_string(),
            func: Rc::new(|_args| {
                let mut btn = ObjectValue::new();
                btn.set("pressed", Value::Boolean(false));
                btn.set("touched", Value::Boolean(false));
                btn.set("value", Value::Number(0.0));
                Ok(Value::Object(Rc::new(RefCell::new(btn))))
            }),
        }),
    );

    // GamepadEvent constructor.
    scope.declare(
        "GamepadEvent",
        Value::Builtin(BuiltinFn {
            name: "GamepadEvent".to_string(),
            func: Rc::new(|args| {
                let mut event = ObjectValue::new();
                let gamepad = args
                    .get(1)
                    .and_then(|v| {
                        if let Value::Object(o) = v {
                            o.borrow().properties.get("gamepad").cloned()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Value::Undefined);
                event.set("gamepad", gamepad);
                Ok(Value::Object(Rc::new(RefCell::new(event))))
            }),
        }),
    );

    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_gamepads_returns_array() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Builtin(gp_fn)) = nav_obj.properties.get("getGamepads") {
                let result = (gp_fn.func)(vec![]).unwrap();
                if let Value::Array(arr) = result {
                    let arr = arr.borrow();
                    assert_eq!(arr.len(), 4);
                    assert!(arr.iter().all(|v| matches!(v, Value::Null)));
                }
            }
        }
    }

    #[test]
    fn gamepad_button_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("GamepadButton").unwrap();
        if let Value::Builtin(b) = ctor {
            let btn = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = btn {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("pressed"), Some(&Value::Boolean(false)));
                assert_eq!(obj.properties.get("value"), Some(&Value::Number(0.0)));
            }
        }
    }
}
