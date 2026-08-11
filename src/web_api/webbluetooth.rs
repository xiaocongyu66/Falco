//! Web Bluetooth API — BLE device communication.
//!
//! ```js
//! const device = await navigator.bluetooth.requestDevice({
//!   filters: [{ services: ['heart_rate'] }]
//! });
//! const server = await device.gatt.connect();
//! const service = await server.getPrimaryService('heart_rate');
//! const characteristic = await service.getCharacteristic('heart_rate_measurement');
//! await characteristic.startNotifications();
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the Web Bluetooth API.
pub fn register(scope: &mut Scope) {
    // navigator.bluetooth
    let mut bt_obj = ObjectValue::new();

    // requestDevice(options)
    bt_obj.set(
        "requestDevice",
        Value::Builtin(BuiltinFn {
            name: "Bluetooth.requestDevice".to_string(),
            func: Rc::new(|_args| make_bluetooth_device()),
        }),
    );

    // getAvailability()
    bt_obj.set(
        "getAvailability",
        Value::Builtin(BuiltinFn {
            name: "Bluetooth.getAvailability".to_string(),
            func: Rc::new(|_args| Ok(Value::Boolean(true))),
        }),
    );

    // getDevices() — returns already-paired devices.
    bt_obj.set(
        "getDevices",
        Value::Builtin(BuiltinFn {
            name: "Bluetooth.getDevices".to_string(),
            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
        }),
    );

    bt_obj.set(
        "requestLEScan",
        Value::Builtin(BuiltinFn {
            name: "Bluetooth.requestLEScan".to_string(),
            func: Rc::new(|_args| {
                let mut scan = ObjectValue::new();
                scan.set("active", Value::Boolean(false));
                scan.set(
                    "stop",
                    Value::Builtin(BuiltinFn {
                        name: "LEScan.stop".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(scan))))
            }),
        }),
    );

    bt_obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "Bluetooth.addEventListener".to_string(),
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

    nav.set("bluetooth", Value::Object(Rc::new(RefCell::new(bt_obj))));
    scope.declare("navigator", Value::Object(Rc::new(RefCell::new(nav))));
}

/// Create a mock BluetoothDevice.
fn make_bluetooth_device() -> Result<Value, String> {
    let mut device = ObjectValue::new();
    device.set("id", Value::String("bt-mock-001".to_string()));
    device.set("name", Value::String("Mock BLE Device".to_string()));
    device.set("gatt", make_gatt_server()?);
    device.set("watchingAdvertisements", Value::Boolean(false));

    // watchAdvertisements()
    device.set(
        "watchAdvertisements",
        Value::Builtin(BuiltinFn {
            name: "BluetoothDevice.watchAdvertisements".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // unwatchAdvertisements()
    device.set(
        "unwatchAdvertisements",
        Value::Builtin(BuiltinFn {
            name: "BluetoothDevice.unwatchAdvertisements".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    device.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "BluetoothDevice.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    device.set("ongattserverdisconnected", Value::Null);
    device.set("onadvertisementreceived", Value::Null);

    Ok(Value::Object(Rc::new(RefCell::new(device))))
}

/// Create a mock BluetoothRemoteGATTServer.
fn make_gatt_server() -> Result<Value, String> {
    let mut server = ObjectValue::new();
    server.set("connected", Value::Boolean(false));
    server.set("device", Value::Undefined);

    // connect()
    server.set(
        "connect",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTServer.connect".to_string(),
            func: Rc::new(|_args| {
                // Return a resolved promise (the server itself).
                let mut s = ObjectValue::new();
                s.set("connected", Value::Boolean(true));
                Ok(Value::Object(Rc::new(RefCell::new(s))))
            }),
        }),
    );

    // disconnect()
    server.set(
        "disconnect",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTServer.disconnect".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // getPrimaryService(service)
    server.set(
        "getPrimaryService",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTServer.getPrimaryService".to_string(),
            func: Rc::new(|_args| make_gatt_service()),
        }),
    );

    // getPrimaryServices()
    server.set(
        "getPrimaryServices",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTServer.getPrimaryServices".to_string(),
            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(server))))
}

/// Create a mock BluetoothRemoteGATTService.
fn make_gatt_service() -> Result<Value, String> {
    let mut service = ObjectValue::new();
    service.set("uuid", Value::String("0000180f-0000-1000-8000-00805f9b34fb".to_string()));
    service.set("isPrimary", Value::Boolean(true));
    service.set("device", Value::Undefined);

    // getCharacteristic(characteristic)
    service.set(
        "getCharacteristic",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTService.getCharacteristic".to_string(),
            func: Rc::new(|_args| make_gatt_characteristic()),
        }),
    );

    // getCharacteristics()
    service.set(
        "getCharacteristics",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTService.getCharacteristics".to_string(),
            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
        }),
    );

    // getIncludedService(service)
    service.set(
        "getIncludedService",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTService.getIncludedService".to_string(),
            func: Rc::new(|_args| make_gatt_service()),
        }),
    );

    // getIncludedServices()
    service.set(
        "getIncludedServices",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTService.getIncludedServices".to_string(),
            func: Rc::new(|_args| Ok(Value::Array(Rc::new(RefCell::new(vec![]))))),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(service))))
}

/// Create a mock BluetoothRemoteGATTCharacteristic.
fn make_gatt_characteristic() -> Result<Value, String> {
    let mut char_obj = ObjectValue::new();
    char_obj.set("uuid", Value::String("00002a19-0000-1000-8000-00805f9b34fb".to_string()));
    char_obj.set("value", Value::Undefined);
    char_obj.set(
        "properties",
        Value::Object(Rc::new(RefCell::new({
            let mut props = ObjectValue::new();
            props.set("broadcast", Value::Boolean(false));
            props.set("read", Value::Boolean(true));
            props.set("writeWithoutResponse", Value::Boolean(false));
            props.set("write", Value::Boolean(false));
            props.set("notify", Value::Boolean(true));
            props.set("indicate", Value::Boolean(false));
            props.set("authenticatedSignedWrites", Value::Boolean(false));
            props.set("reliableWrite", Value::Boolean(false));
            props.set("writableAuxiliaries", Value::Boolean(false));
            props
        }))),
    );

    // readValue()
    char_obj.set(
        "readValue",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTCharacteristic.readValue".to_string(),
            func: Rc::new(|_args| {
                // Return a DataView-like object with empty data.
                let mut dv = ObjectValue::new();
                dv.set("buffer", Value::Array(Rc::new(RefCell::new(vec![]))));
                dv.set("byteLength", Value::Number(0.0));
                Ok(Value::Object(Rc::new(RefCell::new(dv))))
            }),
        }),
    );

    // writeValue(value)
    char_obj.set(
        "writeValue",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTCharacteristic.writeValue".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // writeValueWithResponse(value)
    char_obj.set(
        "writeValueWithResponse",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTCharacteristic.writeValueWithResponse".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // writeValueWithoutResponse(value)
    char_obj.set(
        "writeValueWithoutResponse",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTCharacteristic.writeValueWithoutResponse".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // startNotifications()
    char_obj.set(
        "startNotifications",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTCharacteristic.startNotifications".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    // stopNotifications()
    char_obj.set(
        "stopNotifications",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTCharacteristic.stopNotifications".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    char_obj.set(
        "addEventListener",
        Value::Builtin(BuiltinFn {
            name: "BluetoothRemoteGATTCharacteristic.addEventListener".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );

    char_obj.set("oncharacteristicvaluechanged", Value::Null);

    Ok(Value::Object(Rc::new(RefCell::new(char_obj))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_navigator_exists() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            assert!(nav_obj.borrow().properties.contains_key("bluetooth"));
        }
    }

    #[test]
    fn request_device() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(bt_obj)) = nav_obj.properties.get("bluetooth") {
                let bt_obj = bt_obj.borrow();
                if let Some(Value::Builtin(req_fn)) = bt_obj.properties.get("requestDevice") {
                    let device = (req_fn.func)(vec![]).unwrap();
                    if let Value::Object(d) = device {
                        let d = d.borrow();
                        assert!(d.properties.contains_key("gatt"));
                        assert!(d.properties.contains_key("name"));
                    }
                }
            }
        }
    }

    #[test]
    fn gatt_server_methods() {
        let server = make_gatt_server().unwrap();
        if let Value::Object(obj) = &server {
            let obj = obj.borrow();
            assert!(obj.properties.contains_key("connect"));
            assert!(obj.properties.contains_key("disconnect"));
            assert!(obj.properties.contains_key("getPrimaryService"));
            assert!(obj.properties.contains_key("getPrimaryServices"));
        }
    }

    #[test]
    fn characteristic_methods() {
        let char_obj = make_gatt_characteristic().unwrap();
        if let Value::Object(obj) = &char_obj {
            let obj = obj.borrow();
            assert!(obj.properties.contains_key("readValue"));
            assert!(obj.properties.contains_key("writeValue"));
            assert!(obj.properties.contains_key("startNotifications"));
            assert!(obj.properties.contains_key("stopNotifications"));
            assert!(obj.properties.contains_key("properties"));
        }
    }

    #[test]
    fn get_availability() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let nav = scope.get("navigator").unwrap();
        if let Value::Object(nav_obj) = nav {
            let nav_obj = nav_obj.borrow();
            if let Some(Value::Object(bt_obj)) = nav_obj.properties.get("bluetooth") {
                let bt_obj = bt_obj.borrow();
                if let Some(Value::Builtin(avail_fn)) = bt_obj.properties.get("getAvailability") {
                    let result = (avail_fn.func)(vec![]).unwrap();
                    assert_eq!(result, Value::Boolean(true));
                }
            }
        }
    }
}
