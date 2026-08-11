//! WebUSB API — USB device access from JavaScript.
//!
//! # Overview
//!
//! WebUSB allows web pages to communicate with USB devices. The API:
//!
//! ```js
//! const device = await navigator.usb.requestDevice({ filters: [{ vendorId: 0x1234 }] });
//! await device.open();
//! await device.selectConfiguration(1);
//! await device.claimInterface(0);
//! await device.transferOut(1, data);
//! ```
//!
//! # Implementation
//!
//! Real WebUSB requires OS-level USB access, which browsers gate behind
//! a permission prompt. Falco implements the **API surface** with stub
//! implementations — no actual USB transfers occur. This allows testing
//! code that uses the WebUSB API without requiring physical hardware.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the WebUSB API.
pub fn register(scope: &mut Scope) {
    let mut usb_obj = ObjectValue::new();

    // requestDevice({ filters }) — returns a Promise resolving to a USBDevice.
    usb_obj.set(
        "requestDevice",
        Value::Builtin(BuiltinFn {
            name: "USB.requestDevice".to_string(),
            func: Rc::new(|args| {
                let filters = if let Some(Value::Object(opts)) = args.first() {
                    opts.borrow().properties.get("filters").cloned()
                } else {
                    None
                };
                let _ = filters;
                make_usb_device()
            }),
        }),
    );

    // getDevices() — returns already-paired devices.
    usb_obj.set(
        "getDevices",
        Value::Builtin(BuiltinFn {
            name: "USB.getDevices".to_string(),
            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
        }),
    );

    // Events.
    usb_obj.set("onconnect", Value::Null);
    usb_obj.set("ondisconnect", Value::Null);
    usb_obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "USB.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    scope.declare("USB", Value::Object(Rc::new(RefCell::new(usb_obj))));

    // navigator.usb
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

    let mut usb_nav = ObjectValue::new();
    usb_nav.set(
        "requestDevice",
        Value::Builtin(BuiltinFn {
            name: "navigator.usb.requestDevice".to_string(),
            func: Rc::new(|args| {
                let _ = args;
                make_usb_device()
            }),
        }),
    );
    usb_nav.set(
        "getDevices",
        Value::Builtin(BuiltinFn {
            name: "navigator.usb.getDevices".to_string(),
            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
        }),
    );
    usb_nav.set("onconnect", Value::Null);
    usb_nav.set("ondisconnect", Value::Null);
    usb_nav.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "navigator.usb.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    nav.set("usb", Value::Object(Rc::new(RefCell::new(usb_nav))));
    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
}

/// Create a mock USBDevice.
fn make_usb_device() -> Result<Value, String> {
    let mut device = ObjectValue::new();
    device.set("usbVersionMajor", Value::Number(2.0));
    device.set("usbVersionMinor", Value::Number(0.0));
    device.set("usbVersionSubminor", Value::Number(0.0));
    device.set("deviceClass", Value::Number(0.0));
    device.set("deviceSubclass", Value::Number(0.0));
    device.set("deviceProtocol", Value::Number(0.0));
    device.set("vendorId", Value::Number(0x1234 as f64));
    device.set("productId", Value::Number(0x5678 as f64));
    device.set("manufacturerName", Value::String("Falco Mock USB".to_string()));
    device.set("productName", Value::String("Mock Device".to_string()));
    device.set("serialNumber", Value::String("000001".to_string()));
    device.set("configuration", Value::Null);
    device.set("configurations", Value::Array(Rc::new(RefCell::new(vec![]))));
    device.set("opened", Value::Boolean(false));

    // open()
    device.set(
        "open",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.open".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // close()
    device.set(
        "close",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.close".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // selectConfiguration(configurationValue)
    device.set(
        "selectConfiguration",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.selectConfiguration".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // claimInterface(interfaceNumber)
    device.set(
        "claimInterface",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.claimInterface".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // releaseInterface(interfaceNumber)
    device.set(
        "releaseInterface",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.releaseInterface".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // selectAlternateInterface(interfaceNumber, alternateSetting)
    device.set(
        "selectAlternateInterface",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.selectAlternateInterface".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // controlTransferIn(setup, length)
    device.set(
        "controlTransferIn",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.controlTransferIn".to_string(),
            func: Rc::new(|_args| {
                let mut result = ObjectValue::new();
                result.set("status", Value::String("ok".to_string()));
                result.set("data", Value::Array(Rc::new(RefCell::new(vec![]))));
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );

    // controlTransferOut(setup, data)
    device.set(
        "controlTransferOut",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.controlTransferOut".to_string(),
            func: Rc::new(|_args| {
                let mut result = ObjectValue::new();
                result.set("status", Value::String("ok".to_string()));
                result.set("bytesWritten", Value::Number(0.0));
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );

    // transferIn(endpointNumber, length)
    device.set(
        "transferIn",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.transferIn".to_string(),
            func: Rc::new(|_args| {
                let mut result = ObjectValue::new();
                result.set("status", Value::String("ok".to_string()));
                result.set("data", Value::Array(Rc::new(RefCell::new(vec![]))));
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );

    // transferOut(endpointNumber, data)
    device.set(
        "transferOut",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.transferOut".to_string(),
            func: Rc::new(|_args| {
                let mut result = ObjectValue::new();
                result.set("status", Value::String("ok".to_string()));
                result.set("bytesWritten", Value::Number(0.0));
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );

    // reset()
    device.set(
        "reset",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.reset".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // isochronousTransferIn(endpointNumber, packetLengths)
    device.set(
        "isochronousTransferIn",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.isochronousTransferIn".to_string(),
            func: Rc::new(|_args| {
                let mut result = ObjectValue::new();
                result.set("status", Value::String("ok".to_string()));
                result.set("data", Value::Array(Rc::new(RefCell::new(vec![]))));
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );

    // isochronousTransferOut(endpointNumber, data, packetLengths)
    device.set(
        "isochronousTransferOut",
        Value::Builtin(BuiltinFn {
            name: "USBDevice.isochronousTransferOut".to_string(),
            func: Rc::new(|_args| {
                let mut result = ObjectValue::new();
                result.set("status", Value::String("ok".to_string()));
                result.set("bytesWritten", Value::Number(0.0));
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(device))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_global_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("USB").is_some());
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            assert!(nav_obj.borrow().properties.contains_key("usb"));
        }
    }

    #[test]
    fn request_device_returns_mock() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(usb_obj)) = nav_obj.properties.get("usb") {
                let usb_obj = usb_obj.borrow();
                if let Some(Value::Builtin(req_fn)) = usb_obj.properties.get("requestDevice") {
                    let device = (req_fn.func)(vec![]).unwrap();
                    if let Value::Object(d) = device {
                        let d = d.borrow();
                        assert!(d.properties.contains_key("open"));
                        assert!(d.properties.contains_key("close"));
                        assert!(d.properties.contains_key("transferIn"));
                        assert!(d.properties.contains_key("transferOut"));
                    }
                }
            }
        }
    }

    #[test]
    fn transfer_out_returns_status() {
        let device = make_usb_device().unwrap();
        if let Value::Object(obj) = &device {
            let obj = obj.borrow();
            if let Some(Value::Builtin(transfer_fn)) = obj.properties.get("transferOut") {
                let result = (transfer_fn.func)(vec![]).unwrap();
                if let Value::Object(r) = result {
                    let r = r.borrow();
                    assert_eq!(r.properties.get("status"), Some(&Value::String("ok".to_string())));
                }
            }
        }
    }
}
