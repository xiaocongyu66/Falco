//! WebTransport API — HTTP/3-based bidirectional streaming.
//!
//! # Overview
//!
//! WebTransport provides a modern, low-latency transport built on HTTP/3
//! (QUIC). It supports:
//!
//! - Unidirectional streams (one-way data flow)
//! - Bidirectional streams (two-way data flow)
//! - Datagrams (unreliable, unordered messages)
//!
//! ```js
//! const transport = new WebTransport("https://example.com");
//! await transport.ready;
//! const stream = await transport.createBidirectionalStream();
//! const writer = stream.writable.getWriter();
//! await writer.write(new Uint8Array([1, 2, 3]));
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the WebTransport API.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "WebTransport",
        Value::Builtin(BuiltinFn {
            name: "WebTransport".to_string(),
            func: Rc::new(|args| {
                let url = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let options = args.get(1).cloned().unwrap_or(Value::Undefined);

                let mut transport = ObjectValue::new();
                transport.set("url", Value::String(url));
                transport.set("ready", Value::Undefined); // Would be a Promise.
                transport.set("closed", Value::Undefined);
                transport.set("drained", Value::Undefined);

                // datagrams — read/write datagrams.
                let mut datagrams = ObjectValue::new();
                datagrams.set("readable", make_readable_stream());
                datagrams.set("writable", make_writable_stream());
                datagrams.set("maxDatagramSize", Value::Number(1024.0));
                transport.set("datagrams", Value::Object(Rc::new(RefCell::new(datagrams))));

                // createBidirectionalStream()
                transport.set(
                    "createBidirectionalStream",
                    Value::Builtin(BuiltinFn {
                        name: "WebTransport.createBidirectionalStream".to_string(),
                        func: Rc::new(|_args| make_bidirectional_stream()),
                    }),
                );

                // createUnidirectionalStream()
                transport.set(
                    "createUnidirectionalStream",
                    Value::Builtin(BuiltinFn {
                        name: "WebTransport.createUnidirectionalStream".to_string(),
                        func: Rc::new(|_args| Ok(make_writable_stream())),
                    }),
                );

                // incomingBidirectionalStreams
                transport.set("incomingBidirectionalStreams", make_readable_stream());

                // incomingUnidirectionalStreams
                transport.set("incomingUnidirectionalStreams", make_readable_stream());

                // close(closeInfo)
                transport.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "WebTransport.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // sendDatagram(data)
                transport.set(
                    "sendDatagram",
                    Value::Builtin(BuiltinFn {
                        name: "WebTransport.sendDatagram".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // stats
                transport.set(
                    "getStats",
                    Value::Builtin(BuiltinFn {
                        name: "WebTransport.getStats".to_string(),
                        func: Rc::new(|_args| {
                            let mut stats = ObjectValue::new();
                            stats.set("timestamp", Value::Number(0.0));
                            stats.set("bytesSent", Value::Number(0.0));
                            stats.set("bytesReceived", Value::Number(0.0));
                            stats.set("numOutgoingStreamsCreated", Value::Number(0.0));
                            stats.set("numIncomingStreamsCreated", Value::Number(0.0));
                            Ok(Value::Object(Rc::new(RefCell::new(stats))))
                        }),
                    }),
                );

                let _ = options;
                Ok(Value::Object(Rc::new(RefCell::new(transport))))
            }),
        }),
    );

    // WebTransportBidirectionalStream
    scope.declare(
        "WebTransportBidirectionalStream",
        Value::Builtin(BuiltinFn {
            name: "WebTransportBidirectionalStream".to_string(),
            func: Rc::new(|_args| make_bidirectional_stream()),
        }),
    );

    // WebTransportDatagramDuplexStream
    scope.declare(
        "WebTransportDatagramDuplexStream",
        Value::Builtin(BuiltinFn {
            name: "WebTransportDatagramDuplexStream".to_string(),
            func: Rc::new(|_args| {
                let mut duplex = ObjectValue::new();
                duplex.set("readable", make_readable_stream());
                duplex.set("writable", make_writable_stream());
                duplex.set("maxDatagramSize", Value::Number(1024.0));
                Ok(Value::Object(Rc::new(RefCell::new(duplex))))
            }),
        }),
    );

    // WebTransportError
    scope.declare(
        "WebTransportError",
        Value::Builtin(BuiltinFn {
            name: "WebTransportError".to_string(),
            func: Rc::new(|args| {
                let message = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "WebTransport error".to_string());
                let mut err = ObjectValue::new();
                err.set("name", Value::String("WebTransportError".to_string()));
                err.set("message", Value::String(message));
                err.set("streamErrorCode", Value::Null);
                err.set("source", Value::String("stream".to_string()));
                Ok(Value::Object(Rc::new(RefCell::new(err))))
            }),
        }),
    );
}

/// Create a mock ReadableStream.
fn make_readable_stream() -> Value {
    let mut stream = ObjectValue::new();
    stream.set("locked", Value::Boolean(false));
    stream.set(
        "getReader",
        Value::Builtin(BuiltinFn {
            name: "ReadableStream.getReader".to_string(),
            func: Rc::new(|_args| {
                let mut reader = ObjectValue::new();
                reader.set(
                    "read",
                    Value::Builtin(BuiltinFn {
                        name: "ReadableStreamReader.read".to_string(),
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
                        name: "ReadableStreamReader.cancel".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                reader.set(
                    "releaseLock",
                    Value::Builtin(BuiltinFn {
                        name: "ReadableStreamReader.releaseLock".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(reader))))
            }),
        }),
    );
    stream.set(
        "cancel",
        Value::Builtin(BuiltinFn {
            name: "ReadableStream.cancel".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    stream.set(
        "pipeTo",
        Value::Builtin(BuiltinFn {
            name: "ReadableStream.pipeTo".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    stream.set(
        "pipeThrough",
        Value::Builtin(BuiltinFn {
            name: "ReadableStream.pipeThrough".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    stream.set(
        "tee",
        Value::Builtin(BuiltinFn {
            name: "ReadableStream.tee".to_string(),
            func: Rc::new(|_args| {
                Ok(Value::Array(Rc::new(RefCell::new(vec![
                    Value::Object(Rc::new(RefCell::new(ObjectValue::new()))),
                    Value::Object(Rc::new(RefCell::new(ObjectValue::new()))),
                ]))))
            }),
        }),
    );
    Value::Object(Rc::new(RefCell::new(stream)))
}

/// Create a mock WritableStream.
fn make_writable_stream() -> Value {
    let mut stream = ObjectValue::new();
    stream.set("locked", Value::Boolean(false));
    stream.set(
        "getWriter",
        Value::Builtin(BuiltinFn {
            name: "WritableStream.getWriter".to_string(),
            func: Rc::new(|_args| {
                let mut writer = ObjectValue::new();
                writer.set(
                    "write",
                    Value::Builtin(BuiltinFn {
                        name: "WritableStreamDefaultWriter.write".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                writer.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "WritableStreamDefaultWriter.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                writer.set(
                    "abort",
                    Value::Builtin(BuiltinFn {
                        name: "WritableStreamDefaultWriter.abort".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                writer.set(
                    "releaseLock",
                    Value::Builtin(BuiltinFn {
                        name: "WritableStreamDefaultWriter.releaseLock".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                writer.set("desiredSize", Value::Number(1.0));
                Ok(Value::Object(Rc::new(RefCell::new(writer))))
            }),
        }),
    );
    stream.set(
        "abort",
        Value::Builtin(BuiltinFn {
            name: "WritableStream.abort".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    stream.set(
        "close",
        Value::Builtin(BuiltinFn {
            name: "WritableStream.close".to_string(),
            func: Rc::new(|_args| Ok(Value::Undefined)),
        }),
    );
    Value::Object(Rc::new(RefCell::new(stream)))
}

/// Create a mock bidirectional stream.
fn make_bidirectional_stream() -> Result<Value, String> {
    let mut stream = ObjectValue::new();
    stream.set("readable", make_readable_stream());
    stream.set("writable", make_writable_stream());
    Ok(Value::Object(Rc::new(RefCell::new(stream))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webtransport_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("WebTransport").unwrap();
        if let Value::Builtin(b) = ctor {
            let transport = (b.func)(vec![Value::String("https://example.com".to_string())]).unwrap();
            if let Value::Object(obj) = transport {
                let obj = obj.borrow();
                assert_eq!(
                    obj.properties.get("url"),
                    Some(&Value::String("https://example.com".to_string()))
                );
                assert!(obj.properties.contains_key("datagrams"));
                assert!(obj.properties.contains_key("createBidirectionalStream"));
                assert!(obj.properties.contains_key("createUnidirectionalStream"));
                assert!(obj.properties.contains_key("close"));
            }
        }
    }

    #[test]
    fn create_bidirectional_stream() {
        let stream = make_bidirectional_stream().unwrap();
        if let Value::Object(obj) = &stream {
            let obj = obj.borrow();
            assert!(obj.properties.contains_key("readable"));
            assert!(obj.properties.contains_key("writable"));
        }
    }

    #[test]
    fn readable_stream_get_reader() {
        let rs = make_readable_stream();
        if let Value::Object(obj) = &rs {
            let obj = obj.borrow();
            if let Some(Value::Builtin(get_reader_fn)) = obj.properties.get("getReader") {
                let reader = (get_reader_fn.func)(vec![]).unwrap();
                if let Value::Object(r) = reader {
                    assert!(r.borrow().properties.contains_key("read"));
                    assert!(r.borrow().properties.contains_key("cancel"));
                }
            }
        }
    }

    #[test]
    fn writable_stream_get_writer() {
        let ws = make_writable_stream();
        if let Value::Object(obj) = &ws {
            let obj = obj.borrow();
            if let Some(Value::Builtin(get_writer_fn)) = obj.properties.get("getWriter") {
                let writer = (get_writer_fn.func)(vec![]).unwrap();
                if let Value::Object(w) = writer {
                    assert!(w.borrow().properties.contains_key("write"));
                    assert!(w.borrow().properties.contains_key("close"));
                }
            }
        }
    }

    #[test]
    fn all_transport_classes_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        for name in &[
            "WebTransport",
            "WebTransportBidirectionalStream",
            "WebTransportDatagramDuplexStream",
            "WebTransportError",
        ] {
            assert!(scope.get(name).is_some(), "missing: {}", name);
        }
    }
}
