//! TextEncoder / TextDecoder — string encoding/decoding per the Encoding Standard.
//!
//! # TextEncoder
//!
//! Encodes a JS string into UTF-8 bytes. Returns a `Uint8Array`-like object.
//!
//! ```js
//! const encoder = new TextEncoder();
//! const bytes = encoder.encode("hello");  // Uint8Array of 5 bytes
//! ```
//!
//! # TextDecoder
//!
//! Decodes bytes (from a Uint8Array-like) into a JS string. Supports
//! UTF-8, UTF-16LE, UTF-16BE, and ISO-8859-1 (Latin-1).
//!
//! ```js
//! const decoder = new TextDecoder("utf-8");
//! const str = decoder.decode(bytes);  // "hello"
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register TextEncoder and TextDecoder on the scope.
pub fn register(scope: &mut Scope) {
    // TextEncoder — always UTF-8.
    scope.declare(
        "TextEncoder",
        Value::Builtin(BuiltinFn {
            name: "TextEncoder".to_string(),
            func: Rc::new(|args| {
                let mut obj = ObjectValue::new();
                obj.set("encoding", Value::String("utf-8".to_string()));

                obj.set(
                    "encode",
                    Value::Builtin(BuiltinFn {
                        name: "TextEncoder.encode".to_string(),
                        func: Rc::new(|args| {
                            let input = args
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            let bytes = input.as_bytes();
                            let arr: Vec<Value> =
                                bytes.iter().map(|b| Value::Number(*b as f64)).collect();
                            Ok(Value::Array(Rc::new(RefCell::new(arr))))
                        }),
                    }),
                );

                // encodeInto(target, source) — writes into a Uint8Array.
                obj.set(
                    "encodeInto",
                    Value::Builtin(BuiltinFn {
                        name: "TextEncoder.encodeInto".to_string(),
                        func: Rc::new(|args| {
                            let source = args
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            let bytes = source.as_bytes();
                            if let Some(Value::Array(target)) = args.get(1) {
                                let mut target = target.borrow_mut();
                                let mut written = 0usize;
                                let mut read = 0usize;
                                for (i, &b) in bytes.iter().enumerate() {
                                    if i >= target.len() {
                                        break;
                                    }
                                    target[i] = Value::Number(b as f64);
                                    written += 1;
                                    read += 1;
                                }
                                let mut result = ObjectValue::new();
                                result.set("read", Value::Number(read as f64));
                                result.set("written", Value::Number(written as f64));
                                Ok(Value::Object(Rc::new(RefCell::new(result))))
                            } else {
                                Err("encodeInto: target must be a Uint8Array".to_string())
                            }
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // TextDecoder — supports multiple encodings.
    scope.declare(
        "TextDecoder",
        Value::Builtin(BuiltinFn {
            name: "TextDecoder".to_string(),
            func: Rc::new(|args| {
                let encoding = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "utf-8".to_string())
                    .to_lowercase();

                let mut obj = ObjectValue::new();
                obj.set("encoding", Value::String(encoding.clone()));
                obj.set("fatal", Value::Boolean(false));
                obj.set("ignoreBOM", Value::Boolean(false));

                let enc_for_decode = encoding.clone();
                obj.set(
                    "decode",
                    Value::Builtin(BuiltinFn {
                        name: "TextDecoder.decode".to_string(),
                        func: Rc::new(move |args| {
                            let bytes = extract_bytes(&args.first().cloned().unwrap_or(Value::Undefined))?;
                            let s = decode_with_encoding(&enc_for_decode, &bytes)?;
                            Ok(Value::String(s))
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );
}

/// Extract a byte vec from a Value (array of numbers, or string).
fn extract_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::Array(arr) => {
            let arr = arr.borrow();
            arr.iter()
                .map(|v| {
                    let n = v.to_number();
                    if n < 0.0 || n > 255.0 {
                        Err(format!("byte out of range: {}", n))
                    } else {
                        Ok(n as u8)
                    }
                })
                .collect()
        }
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        _ => Err(format!("cannot extract bytes from {}", val.type_name())),
    }
}

/// Decode bytes to a string using the given encoding.
fn decode_with_encoding(encoding: &str, bytes: &[u8]) -> Result<String, String> {
    match encoding {
        "utf-8" | "utf8" => {
            // Strip BOM if present.
            let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                &bytes[3..]
            } else {
                bytes
            };
            String::from_utf8(bytes.to_vec()).map_err(|e| format!("invalid UTF-8: {}", e))
        }
        "utf-16le" | "utf-16" => {
            let mut u16s = Vec::with_capacity(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                u16s.push(u16::from_le_bytes([chunk[0], chunk[1]]));
            }
            String::from_utf16(&u16s).map_err(|e| format!("invalid UTF-16LE: {}", e))
        }
        "utf-16be" => {
            let mut u16s = Vec::with_capacity(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                u16s.push(u16::from_be_bytes([chunk[0], chunk[1]]));
            }
            String::from_utf16(&u16s).map_err(|e| format!("invalid UTF-16BE: {}", e))
        }
        "iso-8859-1" | "latin1" | "ascii" => {
            // Latin-1: each byte is a char.
            Ok(bytes.iter().map(|&b| b as char).collect())
        }
        "windows-1252" | "cp1252" => {
            // Like Latin-1 but with extra chars in 0x80-0x9F.
            Ok(bytes
                .iter()
                .map(|&b| {
                    if b < 0x80 {
                        b as char
                    } else {
                        CP1252_TABLE[(b - 0x80) as usize]
                    }
                })
                .collect())
        }
        _ => Err(format!("unsupported encoding: {}", encoding)),
    }
}

/// Windows-1252 mapping for bytes 0x80-0xFF.
const CP1252_TABLE: [char; 128] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
    '\u{A0}', '¡', '¢', '£', '¤', '¥', '¦', '§', '¨', '©', 'ª', '«', '¬', '\u{AD}', '®', '¯',
    '°', '±', '²', '³', '´', 'µ', '¶', '·', '¸', '¹', 'º', '»', '¼', '½', '¾', '¿',
    'À', 'Á', 'Â', 'Ã', 'Ä', 'Å', 'Æ', 'Ç', 'È', 'É', 'Ê', 'Ë', 'Ì', 'Í', 'Î', 'Ï',
    'Ð', 'Ñ', 'Ò', 'Ó', 'Ô', 'Õ', 'Ö', '×', 'Ø', 'Ù', 'Ú', 'Û', 'Ü', 'Ý', 'Þ', 'ß',
    'à', 'á', 'â', 'ã', 'ä', 'å', 'æ', 'ç', 'è', 'é', 'ê', 'ë', 'ì', 'í', 'î', 'ï',
    'ð', 'ñ', 'ò', 'ó', 'ô', 'õ', 'ö', '÷', 'ø', 'ù', 'ú', 'û', 'ü', 'ý', 'þ', 'ÿ',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_encoder_utf8() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let encoder = scope.get("TextEncoder").unwrap();
        if let Value::Builtin(b) = encoder {
            let obj = (b.func)(vec![]).unwrap();
            if let Value::Object(o) = obj {
                let o = o.borrow();
                if let Some(Value::Builtin(encode_fn)) = o.properties.get("encode") {
                    let result = (encode_fn.func)(vec![Value::String("hello".to_string())]).unwrap();
                    if let Value::Array(arr) = result {
                        let arr = arr.borrow();
                        let bytes: Vec<u8> =
                            arr.iter().map(|v| v.to_number() as u8).collect();
                        assert_eq!(bytes, b"hello");
                    }
                }
            }
        }
    }

    #[test]
    fn text_decoder_utf8() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let decoder = scope.get("TextDecoder").unwrap();
        if let Value::Builtin(b) = decoder {
            let obj = (b.func)(vec![Value::String("utf-8".to_string())]).unwrap();
            if let Value::Object(o) = obj {
                let o = o.borrow();
                if let Some(Value::Builtin(decode_fn)) = o.properties.get("decode") {
                    let bytes_val = Value::Array(Rc::new(RefCell::new(
                        b"hello".iter().map(|b| Value::Number(*b as f64)).collect(),
                    )));
                    let result = (decode_fn.func)(vec![bytes_val]).unwrap();
                    assert_eq!(result, Value::String("hello".to_string()));
                }
            }
        }
    }

    #[test]
    fn text_decoder_utf8_with_bom() {
        let bytes = vec![0xEF, 0xBB, 0xBF, b'h', b'i'];
        let s = decode_with_encoding("utf-8", &bytes).unwrap();
        assert_eq!(s, "hi");
    }

    #[test]
    fn text_decoder_utf16le() {
        // "hi" in UTF-16LE: [0x68, 0x00, 0x69, 0x00]
        let bytes = [0x68, 0x00, 0x69, 0x00];
        let s = decode_with_encoding("utf-16le", &bytes).unwrap();
        assert_eq!(s, "hi");
    }

    #[test]
    fn text_decoder_latin1() {
        let bytes = [0x48, 0x69]; // "Hi"
        let s = decode_with_encoding("latin1", &bytes).unwrap();
        assert_eq!(s, "Hi");
    }

    #[test]
    fn text_decoder_windows1252_euro_sign() {
        // 0x80 in CP1252 is the Euro sign €
        let bytes = [0x80];
        let s = decode_with_encoding("windows-1252", &bytes).unwrap();
        assert_eq!(s, "€");
    }

    #[test]
    fn encode_into() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let encoder = scope.get("TextEncoder").unwrap();
        if let Value::Builtin(b) = encoder {
            let obj = (b.func)(vec![]).unwrap();
            if let Value::Object(o) = obj {
                let o = o.borrow();
                if let Some(Value::Builtin(encode_into_fn)) = o.properties.get("encodeInto") {
                    let target = Value::Array(Rc::new(RefCell::new(vec![
                        Value::Number(0.0),
                        Value::Number(0.0),
                        Value::Number(0.0),
                        Value::Number(0.0),
                        Value::Number(0.0),
                    ])));
                    let result = (encode_into_fn.func)(vec![
                        Value::String("hi".to_string()),
                        target,
                    ]).unwrap();
                    if let Value::Object(r) = result {
                        let r = r.borrow();
                        assert_eq!(r.properties.get("read"), Some(&Value::Number(2.0)));
                        assert_eq!(r.properties.get("written"), Some(&Value::Number(2.0)));
                    }
                }
            }
        }
    }
}
