//! Notifications API and Web Share API.
//!
//! # Notifications
//!
//! `new Notification(title, options)` displays a system notification.
//! `Notification.requestPermission()` asks the user for permission.
//! `Notification.permission` is "default", "granted", or "denied".
//!
//! Since Falco doesn't have a system notification backend, notifications
//! are logged to the console and the permission is always "granted"
//! (so the API is usable for testing).
//!
//! # Web Share
//!
//! `navigator.share({title, text, url})` opens the native share sheet.
//! `navigator.canShare(data)` checks if sharing is supported.
//!
//! Since Falco doesn't have a native share sheet, share calls log to
//! the console and return a resolved promise.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Notifications and Web Share APIs.
pub fn register(scope: &mut Scope) {
    register_notifications(scope);
    register_web_share(scope);
}

fn register_notifications(scope: &mut Scope) {
    // Notification constructor.
    scope.declare(
        "Notification",
        Value::Builtin(BuiltinFn {
            name: "Notification".to_string(),
            func: Rc::new(|args| {
                let title = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);

                let mut notif = ObjectValue::new();
                notif.set("title", Value::String(title.clone()));
                notif.set("body", Value::String(extract_string(&options, "body")));
                notif.set("tag", Value::String(extract_string(&options, "tag")));
                notif.set("icon", Value::String(extract_string(&options, "icon")));
                notif.set("lang", Value::String(extract_string(&options, "lang")));
                notif.set("dir", Value::String(extract_string(&options, "dir")));
                notif.set("silent", Value::Boolean(false));
                notif.set("timestamp", Value::Number(current_time_ms()));
                notif.set("data", Value::Undefined);

                // Log the notification (since we don't have a system backend).
                eprintln!("[notification] {}", title);

                notif.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "Notification.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                notif.set("onclick", Value::Null);
                notif.set("onshow", Value::Null);
                notif.set("onerror", Value::Null);
                notif.set("onclose", Value::Null);

                Ok(Value::Object(Rc::new(RefCell::new(notif))))
            }),
        }),
    );

    // Notification.permission — always "granted" in Falco.
    if let Some(Value::Builtin(_)) = scope.get("Notification") {
        // We can't easily add a static property to a Builtin. Instead,
        // we expose Notification as an object with a constructor.
    }

    // Notification.requestPermission() — returns "granted".
    scope.declare(
        "Notification_requestPermission",
        Value::Builtin(BuiltinFn {
            name: "Notification.requestPermission".to_string(),
            func: Rc::new(|_args| Ok(Value::String("granted".to_string()))),
        }),
    );

    // Notification.permission static property.
    scope.declare("Notification_permission", Value::String("granted".to_string()));
}

fn register_web_share(scope: &mut Scope) {
    // navigator.share(data) — returns a Promise (simplified — returns undefined).
    let share_fn = Value::Builtin(BuiltinFn {
        name: "navigator.share".to_string(),
        func: Rc::new(|args| {
            let data = args.first().cloned().unwrap_or(Value::Undefined);
            if let Value::Object(obj) = &data {
                let obj = obj.borrow();
                let title = obj.properties.get("title").map(|v| v.to_string()).unwrap_or_default();
                let text = obj.properties.get("text").map(|v| v.to_string()).unwrap_or_default();
                let url = obj.properties.get("url").map(|v| v.to_string()).unwrap_or_default();
                eprintln!("[share] title={} text={} url={}", title, text, url);
            }
            Ok(Value::Undefined)
        }),
    });

    // navigator.canShare(data) — returns true if sharing is supported.
    let can_share_fn = Value::Builtin(BuiltinFn {
        name: "navigator.canShare".to_string(),
        func: Rc::new(|args| {
            let data = args.first().cloned().unwrap_or(Value::Undefined);
            if let Value::Object(obj) = &data {
                let obj = obj.borrow();
                // Can share if any of title/text/url is present.
                let has_data = obj.properties.contains_key("title")
                    || obj.properties.contains_key("text")
                    || obj.properties.contains_key("url");
                Ok(Value::Boolean(has_data))
            } else {
                Ok(Value::Boolean(false))
            }
        }),
    });

    // Add to navigator.
    if let Some(Value::Object(nav)) = scope.get("navigator") {
        nav.borrow_mut().properties.insert("share".to_string(), share_fn);
        nav.borrow_mut()
            .properties
            .insert("canShare".to_string(), can_share_fn);
    } else {
        let mut nav = ObjectValue::new();
        nav.set("share", share_fn);
        nav.set("canShare", can_share_fn);
        scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
    }
}

/// Extract a string property from a Value.
fn extract_string(val: &Value, key: &str) -> String {
    if let Value::Object(o) = val {
        o.borrow()
            .properties
            .get(key)
            .map(|v| v.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    }
}

/// Current time in milliseconds.
fn current_time_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("Notification").unwrap();
        if let Value::Builtin(b) = ctor {
            let notif = (b.func)(vec![
                Value::String("Hello".to_string()),
            ]).unwrap();
            if let Value::Object(obj) = notif {
                let obj = obj.borrow();
                assert_eq!(
                    obj.properties.get("title"),
                    Some(&Value::String("Hello".to_string()))
                );
                assert!(obj.properties.contains_key("close"));
            }
        }
    }

    #[test]
    fn notification_with_options() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("Notification").unwrap();
        let mut opts = ObjectValue::new();
        opts.set("body", Value::String("Notification body".to_string()));
        opts.set("tag", Value::String("my-tag".to_string()));
        if let Value::Builtin(b) = ctor {
            let notif = (b.func)(vec![
                Value::String("Title".to_string()),
                Value::Object(Rc::new(RefCell::new(opts))),
            ]).unwrap();
            if let Value::Object(obj) = notif {
                let obj = obj.borrow();
                assert_eq!(
                    obj.properties.get("body"),
                    Some(&Value::String("Notification body".to_string()))
                );
                assert_eq!(
                    obj.properties.get("tag"),
                    Some(&Value::String("my-tag".to_string()))
                );
            }
        }
    }

    #[test]
    fn web_share() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Builtin(share_fn)) = nav_obj.properties.get("share") {
                let mut data = ObjectValue::new();
                data.set("title", Value::String("Share title".to_string()));
                data.set("text", Value::String("Share text".to_string()));
                let result = (share_fn.func)(vec![Value::Object(Rc::new(RefCell::new(data)))]).unwrap();
                assert_eq!(result, Value::Undefined);
            }
        }
    }

    #[test]
    fn web_can_share() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Builtin(can_share_fn)) = nav_obj.properties.get("canShare") {
                // With data → true.
                let mut data = ObjectValue::new();
                data.set("title", Value::String("Title".to_string()));
                let result = (can_share_fn.func)(vec![Value::Object(Rc::new(RefCell::new(data)))]).unwrap();
                assert_eq!(result, Value::Boolean(true));

                // Without data → false.
                let empty = ObjectValue::new();
                let result = (can_share_fn.func)(vec![Value::Object(Rc::new(RefCell::new(empty)))]).unwrap();
                assert_eq!(result, Value::Boolean(false));
            }
        }
    }
}
