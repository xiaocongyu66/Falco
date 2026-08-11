//! WebSerial API — serial port communication.
//!
//! ```js
//! const port = await navigator.serial.requestPort();
//! await port.open({ baudRate: 9600 });
//! const reader = port.readable.getReader();
//! const { value, done } = await reader.read();
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the WebSerial API.
pub fn register(scope: &mut Scope) {
    // navigator.serial
    let mut serial_obj = ObjectValue::new();

    // requestPort(options)
    serial_obj.set(
        "requestPort",
        Value::Builtin(BuiltinFn {
            name: "Serial.requestPort".to_string(),
            func: Rc::new(|_args| make_serial_port()),
        }),
    );

    // getPorts() — returns already-paired ports.
    serial_obj.set(
        "getPorts",
        Value::Builtin(BuiltinFn {
            name: "Serial.getPorts".to_string(),
            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
        }),
    );

    serial_obj.set("onconnect", Value::Null);
    serial_obj.set("ondisconnect", Value::Null);
    serial_obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "Serial.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // Add to navigator.
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

    nav.set("serial", Value::Object(Rc::new(RefCell::new(serial_obj))));
    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
}

/// Create a mock SerialPort.
fn make_serial_port() -> Result<Value, String> {
    let mut port = ObjectValue::new();

    // readable / writable streams (mock).
    let mut readable = ObjectValue::new();
    readable.set("locked", Value::Boolean(false));
    readable.set(
        "getReader",
        Value::Builtin(BuiltinFn {
            name: "SerialPort.readable.getReader".to_string(),
            func: Rc::new(|_args| {
                let mut reader = ObjectValue::new();
                reader.set(
                    "read",
                    Value::Builtin(BuiltinFn {
                        name: "SerialReader.read".to_string(),
                        func: Rc::new(|_args| {
                            let mut result = ObjectValue::new();
                            result.set("value", Value::Undefined);
                            result.set("done", Value::Boolean(true));
                            Ok(Value::Object(Rc::new(RefCell::new(result))))
                        }),
                    }),
                );
                reader.set(
                    "cancel",
                    Value::Builtin(BuiltinFn {
                        name: "SerialReader.cancel".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                reader.set(
                    "releaseLock",
                    Value::Builtin(BuiltinFn {
                        name: "SerialReader.releaseLock".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(reader))))
            }),
        }),
    );

    let mut writable = ObjectValue::new();
    writable.set("locked", Value::Boolean(false));
    writable.set(
        "getWriter",
        Value::Builtin(BuiltinFn {
            name: "SerialPort.writable.getWriter".to_string(),
            func: Rc::new(|_args| {
                let mut writer = ObjectValue::new();
                writer.set(
                    "write",
                    Value::Builtin(BuiltinFn {
                        name: "SerialWriter.write".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                writer.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "SerialWriter.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                writer.set(
                    "abort",
                    Value::Builtin(BuiltinFn {
                        name: "SerialWriter.abort".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                writer.set(
                    "releaseLock",
                    Value::Builtin(BuiltinFn {
                        name: "SerialWriter.releaseLock".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(writer))))
            }),
        }),
    );

    port.set("readable", Value::Object(Rc::new(RefCell::new(readable))));
    port.set("writable", Value::Object(Rc::new(RefCell::new(writable))));

    // open(options)
    port.set(
        "open",
        Value::Builtin(BuiltinFn {
            name: "SerialPort.open".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // close()
    port.set(
        "close",
        Value::Builtin(BuiltinFn {
            name: "SerialPort.close".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // getInfo()
    port.set(
        "getInfo",
        Value::Builtin(BuiltinFn {
            name: "SerialPort.getInfo".to_string(),
            func: Rc::new(|_args| {
                let mut info = ObjectValue::new();
                info.set("usbVendorId", Value::Undefined);
                info.set("usbProductId", Value::Undefined);
                Ok(Value::Object(Rc::new(RefCell::new(info))))
            }),
        }),
    );

    // setSignals(signals)
    port.set(
        "setSignals",
        Value::Builtin(BuiltinFn {
            name: "SerialPort.setSignals".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // getSignals()
    port.set(
        "getSignals",
        Value::Builtin(BuiltinFn {
            name: "SerialPort.getSignals".to_string(),
            func: Rc::new(|_args| {
                let mut signals = ObjectValue::new();
                signals.set("dataCarrierDetect", Value::Boolean(false));
                signals.set("clearToSend", Value::Boolean(false));
                signals.set("ring", Value::Boolean(false));
                signals.set("dataSetReady", Value::Boolean(false));
                Ok(Value::Object(Rc::new(RefCell::new(signals))))
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(port))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_navigator_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            assert!(nav_obj.borrow().properties.contains_key("serial"));
        }
    }

    #[test]
    fn request_port() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(serial_obj)) = nav_obj.properties.get("serial") {
                let serial_obj = serial_obj.borrow();
                if let Some(Value::Builtin(req_fn)) = serial_obj.properties.get("requestPort") {
                    let port = (req_fn.func)(vec![]).unwrap();
                    if let Value::Object(p) = port {
                        let p = p.borrow();
                        assert!(p.properties.contains_key("open"));
                        assert!(p.properties.contains_key("close"));
                        assert!(p.properties.contains_key("readable"));
                        assert!(p.properties.contains_key("writable"));
                    }
                }
            }
        }
    }

    #[test]
    fn readable_get_reader() {
        let port = make_serial_port().unwrap();
        if let Value::Object(obj) = &port {
            let obj = obj.borrow();
            if let Some(Value::Object(readable)) = obj.properties.get("readable") {
                let readable = readable.borrow();
                if let Some(Value::Builtin(get_reader_fn)) = readable.properties.get("getReader") {
                    let reader = (get_reader_fn.func)(vec![]).unwrap();
                    if let Value::Object(r) = reader {
                        assert!(r.borrow().properties.contains_key("read"));
                        assert!(r.borrow().properties.contains_key("cancel"));
                    }
                }
            }
        }
    }
}
