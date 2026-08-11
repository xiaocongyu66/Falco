//! Compression Streams API — `CompressionStream` / `DecompressionStream`.
//!
//! Provides a stream-based interface to compress/decompress data using
//! gzip, deflate, or brotli. Backed by the `flate2` and `brotli` crates
//! that Falco already depends on.
//!
//! ```js
//! const cs = new CompressionStream("gzip");
//! // In a real browser, you'd pipe a ReadableStream through it.
//! // Falco's simplified API exposes compress(data) -> Uint8Array.
//! ```

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

/// Register CompressionStream and DecompressionStream.
pub fn register(scope: &mut Scope) {
    // CompressionStream
    scope.declare(
        "CompressionStream",
        Value::Builtin(BuiltinFn {
            name: "CompressionStream".to_string(),
            func: Rc::new(|args| {
                let format = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "gzip".to_string())
                    .to_lowercase();

                let mut obj = ObjectValue::new();
                obj.set("format", Value::String(format.clone()));

                let format_for_compress = format.clone();
                obj.set(
                    "compress",
                    Value::Builtin(BuiltinFn {
                        name: "CompressionStream.compress".to_string(),
                        func: Rc::new(move |args| {
                            let data = extract_bytes(
                                &args.first().cloned().unwrap_or(Value::Undefined),
                            )?;
                            let compressed = compress(&format_for_compress, &data)?;
                            Ok(bytes_to_array(&compressed))
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    // DecompressionStream
    scope.declare(
        "DecompressionStream",
        Value::Builtin(BuiltinFn {
            name: "DecompressionStream".to_string(),
            func: Rc::new(|args| {
                let format = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "gzip".to_string())
                    .to_lowercase();

                let mut obj = ObjectValue::new();
                obj.set("format", Value::String(format.clone()));

                let format_for_decompress = format.clone();
                obj.set(
                    "decompress",
                    Value::Builtin(BuiltinFn {
                        name: "DecompressionStream.decompress".to_string(),
                        func: Rc::new(move |args| {
                            let data = extract_bytes(
                                &args.first().cloned().unwrap_or(Value::Undefined),
                            )?;
                            let decompressed = decompress(&format_for_decompress, &data)?;
                            Ok(bytes_to_array(&decompressed))
                        }),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );
}

/// Compress data using the given format.
fn compress(format: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    match format {
        "gzip" => {
            let mut encoder = flate2::write::GzEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            );
            encoder
                .write_all(data)
                .map_err(|e| format!("gzip compress: {}", e))?;
            encoder
                .finish()
                .map_err(|e| format!("gzip finish: {}", e))
        }
        "deflate" => {
            let mut encoder = flate2::write::DeflateEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            );
            encoder
                .write_all(data)
                .map_err(|e| format!("deflate compress: {}", e))?;
            encoder
                .finish()
                .map_err(|e| format!("deflate finish: {}", e))
        }
        "deflate-raw" => {
            let mut encoder = flate2::write::ZlibEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            );
            encoder
                .write_all(data)
                .map_err(|e| format!("deflate-raw compress: {}", e))?;
            encoder
                .finish()
                .map_err(|e| format!("deflate-raw finish: {}", e))
        }
        "brotli" => {
            let mut compressed = Vec::new();
            let params = brotli::enc::BrotliEncoderParams::default();
            brotli::BrotliCompress(&mut &data[..], &mut compressed, &params)
                .map_err(|e| format!("brotli compress: {}", e))?;
            Ok(compressed)
        }
        _ => Err(format!("unsupported compression format: {}", format)),
    }
}

/// Decompress data using the given format.
fn decompress(format: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    match format {
        "gzip" => {
            let mut decoder = flate2::read::GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| format!("gzip decompress: {}", e))?;
            Ok(decompressed)
        }
        "deflate" => {
            let mut decoder = flate2::read::DeflateDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| format!("deflate decompress: {}", e))?;
            Ok(decompressed)
        }
        "deflate-raw" => {
            let mut decoder = flate2::read::ZlibDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| format!("deflate-raw decompress: {}", e))?;
            Ok(decompressed)
        }
        "brotli" => {
            let mut decoder = brotli::Decompressor::new(&data[..], 4096);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| format!("brotli decompress: {}", e))?;
            Ok(decompressed)
        }
        _ => Err(format!("unsupported decompression format: {}", format)),
    }
}

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

fn bytes_to_array(bytes: &[u8]) -> Value {
    Value::Array(Rc::new(RefCell::new(
        bytes.iter().map(|b| Value::Number(*b as f64)).collect(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_round_trip() {
        let original = b"hello world, this is a test of gzip compression!";
        let compressed = compress("gzip", original).unwrap();
        assert!(compressed.len() > 0);
        let decompressed = decompress("gzip", &compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn deflate_round_trip() {
        let original = b"hello world";
        let compressed = compress("deflate", original).unwrap();
        let decompressed = decompress("deflate", &compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn brotli_round_trip() {
        let original = b"hello world, brotli is awesome!";
        let compressed = compress("brotli", original).unwrap();
        let decompressed = decompress("brotli", &compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn compression_stream_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("CompressionStream").is_some());
        assert!(scope.get("DecompressionStream").is_some());
    }
}
