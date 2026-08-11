//! Screen Capture API + Contact Picker API.
//!
//! # Screen Capture
//!
//! `navigator.mediaDevices.getDisplayMedia()` captures the screen or a
//! window. Returns a MediaStream (from the WebRTC module).
//!
//! # Contact Picker
//!
//! `navigator.contacts.select(properties)` opens the contact picker and
//! returns the selected contacts.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Screen Capture and Contact Picker APIs.
pub fn register(scope: &mut Scope) {
    register_screen_capture(scope);
    register_contact_picker(scope);
}

fn register_screen_capture(scope: &mut Scope) {
    // navigator.mediaDevices.getDisplayMedia(constraints)
    let get_display_media = Value::Builtin(BuiltinFn {
        name: "mediaDevices.getDisplayMedia".to_string(),
        func: Rc::new(|_args| {
            // Return a mock MediaStream.
            let mut stream = ObjectValue::new();
            stream.set("id", Value::String("display-stream-001".to_string()));
            stream.set("active", Value::Boolean(true));
            stream.set(
                "getTracks",
                Value::Builtin(BuiltinFn {
                    name: "MediaStream.getTracks".to_string(),
                    func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
                }),
            );
            stream.set(
                "getVideoTracks",
                Value::Builtin(BuiltinFn {
                    name: "MediaStream.getVideoTracks".to_string(),
                    func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
                }),
            );
            stream.set(
                "getAudioTracks",
                Value::Builtin(BuiltinFn {
                    name: "MediaStream.getAudioTracks".to_string(),
                    func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
                }),
            );
            Ok(Value::Object(Rc::new(RefCell::new(stream))))
        }),
    });

    // navigator.mediaDevices
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

    // Create mediaDevices object if it doesn't exist.
    let media_devices = if let Some(Value::Object(md)) = nav.properties.get("mediaDevices") {
        // Copy existing mediaDevices.
        let md_ref = md.borrow();
        let mut copy = ObjectValue::new();
        for (k, v) in md_ref.properties.iter() {
            copy.properties.insert(k.clone(), v.clone());
        }
        copy.prototype = md_ref.prototype.clone();
        copy
    } else {
        ObjectValue::new()
    };

    let md_rc = Rc::new(RefCell::new(media_devices));
    md_rc.borrow_mut().set("getDisplayMedia", get_display_media);

    // Also add getUserMedia if not present.
    if md_rc.borrow().properties.get("getUserMedia").is_none() {
        md_rc.borrow_mut().set(
            "getUserMedia",
            Value::Builtin(BuiltinFn {
                name: "mediaDevices.getUserMedia".to_string(),
                func: Rc::new(|_args| {
                    let mut stream = ObjectValue::new();
                    stream.set("id", Value::String("user-stream-001".to_string()));
                    stream.set("active", Value::Boolean(true));
                    Ok(Value::Object(Rc::new(RefCell::new(stream))))
                }),
            }),
        );
    }

    // enumerateDevices()
    if md_rc.borrow().properties.get("enumerateDevices").is_none() {
        md_rc.borrow_mut().set(
            "enumerateDevices",
            Value::Builtin(BuiltinFn {
                name: "mediaDevices.enumerateDevices".to_string(),
                func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
            }),
        );
    }

    nav.set("mediaDevices", Value::Object(md_rc));
    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
}

fn register_contact_picker(scope: &mut Scope) {
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

    // navigator.contacts
    let mut contacts = ObjectValue::new();
    contacts.set(
        "select",
        Value::Builtin(BuiltinFn {
            name: "contacts.select".to_string(),
            func: Rc::new(|args| {
                // properties: array of strings like ["name", "email", "tel"]
                let _props = args.first().cloned();
                let _options = args.get(1).cloned();
                // Return an empty array (no contacts selected).
                Ok(Value::Array(Rc::new(RefCell::new(vec![]))))
            }),
        }),
    );
    contacts.set(
        "getProperties",
        Value::Builtin(BuiltinFn {
            name: "contacts.getProperties".to_string(),
            func: Rc::new(|_args| {
                Ok(Value::Array(Rc::new(RefCell::new(vec![
                    Value::String("name".to_string()),
                    Value::String("email".to_string()),
                    Value::String("tel".to_string()),
                    Value::String("address".to_string()),
                    Value::String("icon".to_string()),
                ]))))
            }),
        }),
    );

    nav.set("contacts", Value::Object(Rc::new(RefCell::new(contacts))));
    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_display_media_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(md)) = nav_obj.properties.get("mediaDevices") {
                let md = md.borrow();
                assert!(md.properties.contains_key("getDisplayMedia"));
                assert!(md.properties.contains_key("getUserMedia"));
                assert!(md.properties.contains_key("enumerateDevices"));
            }
        }
    }

    #[test]
    fn get_display_media_returns_stream() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(md)) = nav_obj.properties.get("mediaDevices") {
                let md = md.borrow();
                if let Some(Value::Builtin(gdm_fn)) = md.properties.get("getDisplayMedia") {
                    let result = (gdm_fn.func)(vec![]).unwrap();
                    if let Value::Object(stream) = result {
                        let stream = stream.borrow();
                        assert_eq!(stream.properties.get("active"), Some(&Value::Boolean(true)));
                    }
                }
            }
        }
    }

    #[test]
    fn contacts_select() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(contacts)) = nav_obj.properties.get("contacts") {
                let contacts = contacts.borrow();
                if let Some(Value::Builtin(select_fn)) = contacts.properties.get("select") {
                    let result = (select_fn.func)(vec![
                        Value::Array(Rc::new(RefCell::new(vec![
                            Value::String("name".to_string()),
                            Value::String("email".to_string()),
                        ]))),
                    ]).unwrap();
                    if let Value::Array(arr) = result {
                        assert!(arr.borrow().is_empty());
                    }
                }
            }
        }
    }

    #[test]
    fn contacts_get_properties() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(contacts)) = nav_obj.properties.get("contacts") {
                let contacts = contacts.borrow();
                if let Some(Value::Builtin(get_props_fn)) = contacts.properties.get("getProperties") {
                    let result = (get_props_fn.func)(vec![]).unwrap();
                    if let Value::Array(arr) = result {
                        let arr = arr.borrow();
                        assert!(arr.len() >= 3); // name, email, tel at minimum
                    }
                }
            }
        }
    }
}
