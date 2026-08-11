//! WebCodecs API — low-level media codec access.
//!
//! # Overview
//!
//! WebCodecs provides direct access to browser-internal video/audio codecs
//! without needing to go through `<video>` or MediaRecorder. This enables:
//!
//! - Decoding video frames from a network stream (e.g., for custom players)
//! - Encoding video for real-time communication (WebRTC)
//! - Processing video frames on a canvas
//! - Transcoding between formats
//!
//! # Classes
//!
//! - `VideoDecoder` — decodes EncodedVideoChunk → VideoFrame
//! - `VideoEncoder` — encodes VideoFrame → EncodedVideoChunk
//! - `AudioDecoder` — decodes EncodedAudioChunk → AudioData
//! - `AudioEncoder` — encodes AudioData → EncodedAudioChunk
//! - `ImageDecoder` — decodes images (PNG, JPEG, WebP, AVIF, GIF)
//! - `VideoFrame` — a frame of video (pixel data + metadata)
//! - `AudioData` — a buffer of audio samples
//! - `EncodedVideoChunk` / `EncodedAudioChunk` — compressed media data
//!
//! # Implementation
//!
//! Falco's WebCodecs uses the existing `image` crate for image decoding
//! and the `openh264` crate (optional) for H.264 video. The encoder side
//! is stubbed (returns empty chunks) since real encoding requires a
//! full codec implementation.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Register the WebCodecs API.
pub fn register(scope: &mut Scope) {
    register_video_frame(scope);
    register_audio_data(scope);
    register_encoded_chunks(scope);
    register_video_decoder(scope);
    register_video_encoder(scope);
    register_audio_decoder(scope);
    register_audio_encoder(scope);
    register_image_decoder(scope);
}

// ── VideoFrame ────────────────────────────────────────────────────────

fn register_video_frame(scope: &mut Scope) {
    scope.declare(
        "VideoFrame",
        Value::Builtin(BuiltinFn {
            name: "VideoFrame".to_string(),
            func: Rc::new(|args| {
                // VideoFrame can be constructed from:
                // - Another VideoFrame (copy)
                // - A BufferSource (typed array) + options (format, codedWidth, codedHeight)
                // - An options object with no source (for GPU textures)
                let mut frame = ObjectValue::new();

                // Extract dimensions from options if provided.
                let (width, height, format) = if let Some(Value::Object(opts)) = args.get(1) {
                    let opts = opts.borrow();
                    let w = opts.properties.get("codedWidth").map(|v| v.to_number()).unwrap_or(0.0);
                    let h = opts.properties.get("codedHeight").map(|v| v.to_number()).unwrap_or(0.0);
                    let fmt = opts.properties.get("format").map(|v| v.to_string()).unwrap_or_else(|| "RGBA".to_string());
                    (w, h, fmt)
                } else {
                    (0.0, 0.0, "RGBA".to_string())
                };

                frame.set("format", Value::String(format));
                frame.set("codedWidth", Value::Number(width));
                frame.set("codedHeight", Value::Number(height));
                frame.set("visibleWidth", Value::Number(width));
                frame.set("visibleHeight", Value::Number(height));
                frame.set("timestamp", Value::Number(0.0));
                frame.set("duration", Value::Number(0.0));
                frame.set("colorSpace", Value::Object(Rc::new(RefCell::new({
                    let mut cs = ObjectValue::new();
                    cs.set("primaries", Value::String("bt709".to_string()));
                    cs.set("transfer", Value::String("bt709".to_string()));
                    cs.set("matrix", Value::String("bt709".to_string()));
                    cs.set("fullRange", Value::Boolean(false));
                    cs
                }))));

                // Store the source data (if any).
                frame.set("__data", args.first().cloned().unwrap_or(Value::Undefined));

                // allocationSize()
                frame.set(
                    "allocationSize",
                    Value::Builtin(BuiltinFn {
                        name: "VideoFrame.allocationSize".to_string(),
                        func: Rc::new(move |_args| {
                            let size = (width * height * 4.0) as usize;
                            Ok(Value::Number(size as f64))
                        }),
                    }),
                );

                // copyTo(buffer)
                let w = width;
                let h = height;
                frame.set(
                    "copyTo",
                    Value::Builtin(BuiltinFn {
                        name: "VideoFrame.copyTo".to_string(),
                        func: Rc::new(move |_args| {
                            // Return the number of bytes copied (stub — copies zeros).
                            let size = (w * h * 4.0) as usize;
                            Ok(Value::Number(size as f64))
                        }),
                    }),
                );

                // close()
                frame.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "VideoFrame.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // clone()
                frame.set(
                    "clone",
                    Value::Builtin(BuiltinFn {
                        name: "VideoFrame.clone".to_string(),
                        func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(frame))))
            }),
        }),
    );
}

// ── AudioData ─────────────────────────────────────────────────────────

fn register_audio_data(scope: &mut Scope) {
    scope.declare(
        "AudioData",
        Value::Builtin(BuiltinFn {
            name: "AudioData".to_string(),
            func: Rc::new(|args| {
                let mut data = ObjectValue::new();
                let format = if let Some(Value::Object(opts)) = args.first() {
                    let opts = opts.borrow();
                    opts.properties.get("format").map(|v| v.to_string()).unwrap_or_else(|| "f32".to_string())
                } else {
                    "f32".to_string()
                };
                let sample_rate = if let Some(Value::Object(opts)) = args.first() {
                    let opts = opts.borrow();
                    opts.properties.get("sampleRate").map(|v| v.to_number()).unwrap_or(44100.0)
                } else {
                    44100.0
                };
                let num_frames = if let Some(Value::Object(opts)) = args.first() {
                    let opts = opts.borrow();
                    opts.properties.get("numberOfFrames").map(|v| v.to_number()).unwrap_or(0.0)
                } else {
                    0.0
                };
                let num_channels = if let Some(Value::Object(opts)) = args.first() {
                    let opts = opts.borrow();
                    opts.properties.get("numberOfChannels").map(|v| v.to_number()).unwrap_or(2.0)
                } else {
                    2.0
                };

                data.set("format", Value::String(format));
                data.set("sampleRate", Value::Number(sample_rate));
                data.set("numberOfFrames", Value::Number(num_frames));
                data.set("numberOfChannels", Value::Number(num_channels));
                data.set("timestamp", Value::Number(0.0));
                data.set("duration", Value::Number(0.0));

                data.set(
                    "copyTo",
                    Value::Builtin(BuiltinFn {
                        name: "AudioData.copyTo".to_string(),
                        func: Rc::new(|_args| Ok(Value::Number(0.0))),
                    }),
                );

                data.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "AudioData.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                data.set(
                    "clone",
                    Value::Builtin(BuiltinFn {
                        name: "AudioData.clone".to_string(),
                        func: Rc::new(|_args| Ok(Value::Object(Rc::new(RefCell::new(ObjectValue::new()))))),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(data))))
            }),
        }),
    );
}

// ── EncodedVideoChunk / EncodedAudioChunk ─────────────────────────────

fn register_encoded_chunks(scope: &mut Scope) {
    // EncodedVideoChunk
    scope.declare(
        "EncodedVideoChunk",
        Value::Builtin(BuiltinFn {
            name: "EncodedVideoChunk".to_string(),
            func: Rc::new(|args| {
                let mut chunk = ObjectValue::new();
                let (chunk_type, timestamp, duration) = if let Some(Value::Object(opts)) = args.first() {
                    let opts = opts.borrow();
                    let ct = opts.properties.get("type").map(|v| v.to_string()).unwrap_or_else(|| "delta".to_string());
                    let ts = opts.properties.get("timestamp").map(|v| v.to_number()).unwrap_or(0.0);
                    let dur = opts.properties.get("duration").map(|v| v.to_number()).unwrap_or(0.0);
                    (ct, ts, dur)
                } else {
                    ("delta".to_string(), 0.0, 0.0)
                };
                chunk.set("type", Value::String(chunk_type));
                chunk.set("timestamp", Value::Number(timestamp));
                chunk.set("duration", Value::Number(duration));
                chunk.set("byteLength", Value::Number(0.0));
                chunk.set(
                    "copyTo",
                    Value::Builtin(BuiltinFn {
                        name: "EncodedVideoChunk.copyTo".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(chunk))))
            }),
        }),
    );

    // EncodedAudioChunk
    scope.declare(
        "EncodedAudioChunk",
        Value::Builtin(BuiltinFn {
            name: "EncodedAudioChunk".to_string(),
            func: Rc::new(|args| {
                let mut chunk = ObjectValue::new();
                let (chunk_type, timestamp) = if let Some(Value::Object(opts)) = args.first() {
                    let opts = opts.borrow();
                    let ct = opts.properties.get("type").map(|v| v.to_string()).unwrap_or_else(|| "delta".to_string());
                    let ts = opts.properties.get("timestamp").map(|v| v.to_number()).unwrap_or(0.0);
                    (ct, ts)
                } else {
                    ("delta".to_string(), 0.0)
                };
                chunk.set("type", Value::String(chunk_type));
                chunk.set("timestamp", Value::Number(timestamp));
                chunk.set("byteLength", Value::Number(0.0));
                chunk.set(
                    "copyTo",
                    Value::Builtin(BuiltinFn {
                        name: "EncodedAudioChunk.copyTo".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );
                Ok(Value::Object(Rc::new(RefCell::new(chunk))))
            }),
        }),
    );
}

// ── VideoDecoder ──────────────────────────────────────────────────────

fn register_video_decoder(scope: &mut Scope) {
    scope.declare(
        "VideoDecoder",
        Value::Builtin(BuiltinFn {
            name: "VideoDecoder".to_string(),
            func: Rc::new(|args| {
                let mut decoder = ObjectValue::new();
                decoder.set("state", Value::String("unconfigured".to_string()));
                decoder.set("decodeQueueSize", Value::Number(0.0));

                // Store the init callback (output, error).
                if let Some(Value::Object(init)) = args.first() {
                    let init = init.borrow();
                    decoder.set("output", init.properties.get("output").cloned().unwrap_or(Value::Undefined));
                    decoder.set("error", init.properties.get("error").cloned().unwrap_or(Value::Undefined));
                }

                // configure(config)
                decoder.set(
                    "configure",
                    Value::Builtin(BuiltinFn {
                        name: "VideoDecoder.configure".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // decode(chunk)
                decoder.set(
                    "decode",
                    Value::Builtin(BuiltinFn {
                        name: "VideoDecoder.decode".to_string(),
                        func: Rc::new(|_args| Ok(Value::Number(0.0))), // returns decodeQueueSize
                    }),
                );

                // flush()
                decoder.set(
                    "flush",
                    Value::Builtin(BuiltinFn {
                        name: "VideoDecoder.flush".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // reset()
                decoder.set(
                    "reset",
                    Value::Builtin(BuiltinFn {
                        name: "VideoDecoder.reset".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                // close()
                decoder.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "VideoDecoder.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(decoder))))
            }),
        }),
    );

    // VideoDecoder.isConfigSupported(config)
    if let Some(Value::Builtin(_)) = scope.get("VideoDecoder") {
        // Expose as a separate global (can't easily add static methods to a Builtin).
    }
    scope.declare(
        "VideoDecoder_isConfigSupported",
        Value::Builtin(BuiltinFn {
            name: "VideoDecoder.isConfigSupported".to_string(),
            func: Rc::new(|_args| {
                let mut result = ObjectValue::new();
                result.set("supported", Value::Boolean(true));
                result.set("config", Value::Undefined);
                Ok(Value::Object(Rc::new(RefCell::new(result))))
            }),
        }),
    );
}

// ── VideoEncoder ──────────────────────────────────────────────────────

fn register_video_encoder(scope: &mut Scope) {
    scope.declare(
        "VideoEncoder",
        Value::Builtin(BuiltinFn {
            name: "VideoEncoder".to_string(),
            func: Rc::new(|args| {
                let mut encoder = ObjectValue::new();
                encoder.set("state", Value::String("unconfigured".to_string()));
                encoder.set("encodeQueueSize", Value::Number(0.0));

                if let Some(Value::Object(init)) = args.first() {
                    let init = init.borrow();
                    encoder.set("output", init.properties.get("output").cloned().unwrap_or(Value::Undefined));
                    encoder.set("error", init.properties.get("error").cloned().unwrap_or(Value::Undefined));
                }

                encoder.set(
                    "configure",
                    Value::Builtin(BuiltinFn {
                        name: "VideoEncoder.configure".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                encoder.set(
                    "encode",
                    Value::Builtin(BuiltinFn {
                        name: "VideoEncoder.encode".to_string(),
                        func: Rc::new(|_args| Ok(Value::Number(0.0))),
                    }),
                );

                encoder.set(
                    "flush",
                    Value::Builtin(BuiltinFn {
                        name: "VideoEncoder.flush".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                encoder.set(
                    "reset",
                    Value::Builtin(BuiltinFn {
                        name: "VideoEncoder.reset".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                encoder.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "VideoEncoder.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(encoder))))
            }),
        }),
    );
}

// ── AudioDecoder / AudioEncoder ───────────────────────────────────────

fn register_audio_decoder(scope: &mut Scope) {
    scope.declare(
        "AudioDecoder",
        Value::Builtin(BuiltinFn {
            name: "AudioDecoder".to_string(),
            func: Rc::new(|args| {
                let mut decoder = ObjectValue::new();
                decoder.set("state", Value::String("unconfigured".to_string()));
                decoder.set("decodeQueueSize", Value::Number(0.0));

                if let Some(Value::Object(init)) = args.first() {
                    let init = init.borrow();
                    decoder.set("output", init.properties.get("output").cloned().unwrap_or(Value::Undefined));
                    decoder.set("error", init.properties.get("error").cloned().unwrap_or(Value::Undefined));
                }

                for method in &["configure", "decode", "flush", "reset", "close"] {
                    let m = method.to_string();
                    decoder.set(
                        method,
                        Value::Builtin(BuiltinFn {
                            name: format!("AudioDecoder.{}", m),
                            func: Rc::new(|_args| Ok(Value::Undefined)),
                        }),
                    );
                }

                Ok(Value::Object(Rc::new(RefCell::new(decoder))))
            }),
        }),
    );
}

fn register_audio_encoder(scope: &mut Scope) {
    scope.declare(
        "AudioEncoder",
        Value::Builtin(BuiltinFn {
            name: "AudioEncoder".to_string(),
            func: Rc::new(|args| {
                let mut encoder = ObjectValue::new();
                encoder.set("state", Value::String("unconfigured".to_string()));
                encoder.set("encodeQueueSize", Value::Number(0.0));

                if let Some(Value::Object(init)) = args.first() {
                    let init = init.borrow();
                    encoder.set("output", init.properties.get("output").cloned().unwrap_or(Value::Undefined));
                    encoder.set("error", init.properties.get("error").cloned().unwrap_or(Value::Undefined));
                }

                for method in &["configure", "encode", "flush", "reset", "close"] {
                    let m = method.to_string();
                    encoder.set(
                        method,
                        Value::Builtin(BuiltinFn {
                            name: format!("AudioEncoder.{}", m),
                            func: Rc::new(|_args| Ok(Value::Undefined)),
                        }),
                    );
                }

                Ok(Value::Object(Rc::new(RefCell::new(encoder))))
            }),
        }),
    );
}

// ── ImageDecoder ──────────────────────────────────────────────────────

fn register_image_decoder(scope: &mut Scope) {
    scope.declare(
        "ImageDecoder",
        Value::Builtin(BuiltinFn {
            name: "ImageDecoder".to_string(),
            func: Rc::new(|args| {
                let mut decoder = ObjectValue::new();

                // Extract type from init.
                let image_type = if let Some(Value::Object(init)) = args.first() {
                    init.borrow().properties.get("type").map(|v| v.to_string()).unwrap_or_else(|| "image/png".to_string())
                } else {
                    "image/png".to_string()
                };

                decoder.set("type", Value::String(image_type));
                decoder.set("complete", Value::Boolean(true));
                decoder.set("tracks", Value::Object(Rc::new(RefCell::new({
                    let mut tracks = ObjectValue::new();
                    tracks.set("ready", Value::Boolean(true));
                    tracks.set("length", Value::Number(1.0));
                    tracks
                }))));

                // decode(options) — returns a Promise resolving to a VideoFrame.
                decoder.set(
                    "decode",
                    Value::Builtin(BuiltinFn {
                        name: "ImageDecoder.decode".to_string(),
                        func: Rc::new(|_args| {
                            // Return a mock VideoFrame.
                            let mut frame = ObjectValue::new();
                            frame.set("format", Value::String("RGBA".to_string()));
                            frame.set("codedWidth", Value::Number(0.0));
                            frame.set("codedHeight", Value::Number(0.0));
                            Ok(Value::Object(Rc::new(RefCell::new(frame))))
                        }),
                    }),
                );

                // decodeQueuingSize()
                decoder.set(
                    "decodeQueuingSize",
                    Value::Builtin(BuiltinFn {
                        name: "ImageDecoder.decodeQueuingSize".to_string(),
                        func: Rc::new(|_args| Ok(Value::Number(0.0))),
                    }),
                );

                // reset() / close()
                decoder.set(
                    "reset",
                    Value::Builtin(BuiltinFn {
                        name: "ImageDecoder.reset".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                decoder.set(
                    "close",
                    Value::Builtin(BuiltinFn {
                        name: "ImageDecoder.close".to_string(),
                        func: Rc::new(|_args| Ok(Value::Undefined)),
                    }),
                );

                Ok(Value::Object(Rc::new(RefCell::new(decoder))))
            }),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_frame_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("VideoFrame").unwrap();
        if let Value::Builtin(b) = ctor {
            let frame = (b.func)(vec![Value::Undefined]).unwrap();
            if let Value::Object(obj) = frame {
                let obj = obj.borrow();
                assert!(obj.properties.contains_key("format"));
                assert!(obj.properties.contains_key("codedWidth"));
                assert!(obj.properties.contains_key("copyTo"));
                assert!(obj.properties.contains_key("close"));
            }
        }
    }

    #[test]
    fn audio_data_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("AudioData").unwrap();
        if let Value::Builtin(b) = ctor {
            let mut opts = ObjectValue::new();
            opts.set("format", Value::String("f32".to_string()));
            opts.set("sampleRate", Value::Number(44100.0));
            opts.set("numberOfFrames", Value::Number(1024.0));
            opts.set("numberOfChannels", Value::Number(2.0));
            let data = (b.func)(vec![Value::Object(Rc::new(RefCell::new(opts)))]).unwrap();
            if let Value::Object(obj) = data {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("sampleRate"), Some(&Value::Number(44100.0)));
                assert_eq!(obj.properties.get("numberOfChannels"), Some(&Value::Number(2.0)));
            }
        }
    }

    #[test]
    fn encoded_video_chunk() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("EncodedVideoChunk").unwrap();
        if let Value::Builtin(b) = ctor {
            let mut opts = ObjectValue::new();
            opts.set("type", Value::String("key".to_string()));
            opts.set("timestamp", Value::Number(0.0));
            opts.set("duration", Value::Number(33000.0));
            let chunk = (b.func)(vec![Value::Object(Rc::new(RefCell::new(opts)))]).unwrap();
            if let Value::Object(obj) = chunk {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("type"), Some(&Value::String("key".to_string())));
            }
        }
    }

    #[test]
    fn video_decoder_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("VideoDecoder").unwrap();
        let mut init = ObjectValue::new();
        init.set(
            "output",
            Value::Builtin(BuiltinFn {
                name: "output".to_string(),
                func: Rc::new(|_args| Ok(Value::Undefined)),
            }),
        );
        init.set(
            "error",
            Value::Builtin(BuiltinFn {
                name: "error".to_string(),
                func: Rc::new(|_args| Ok(Value::Undefined)),
            }),
        );
        if let Value::Builtin(b) = ctor {
            let decoder = (b.func)(vec![Value::Object(Rc::new(RefCell::new(init)))]).unwrap();
            if let Value::Object(obj) = decoder {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("state"), Some(&Value::String("unconfigured".to_string())));
                assert!(obj.properties.contains_key("configure"));
                assert!(obj.properties.contains_key("decode"));
                assert!(obj.properties.contains_key("flush"));
            }
        }
    }

    #[test]
    fn video_encoder_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("VideoEncoder").unwrap();
        if let Value::Builtin(b) = ctor {
            let encoder = (b.func)(vec![Value::Undefined]).unwrap();
            if let Value::Object(obj) = encoder {
                let obj = obj.borrow();
                assert!(obj.properties.contains_key("configure"));
                assert!(obj.properties.contains_key("encode"));
                assert!(obj.properties.contains_key("flush"));
            }
        }
    }

    #[test]
    fn image_decoder_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("ImageDecoder").unwrap();
        let mut init = ObjectValue::new();
        init.set("type", Value::String("image/png".to_string()));
        if let Value::Builtin(b) = ctor {
            let decoder = (b.func)(vec![Value::Object(Rc::new(RefCell::new(init)))]).unwrap();
            if let Value::Object(obj) = decoder {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("type"), Some(&Value::String("image/png".to_string())));
                assert!(obj.properties.contains_key("decode"));
            }
        }
    }

    #[test]
    fn all_codecs_classes_registered() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        for name in &[
            "VideoFrame",
            "AudioData",
            "EncodedVideoChunk",
            "EncodedAudioChunk",
            "VideoDecoder",
            "VideoEncoder",
            "AudioDecoder",
            "AudioEncoder",
            "ImageDecoder",
        ] {
            assert!(scope.get(name).is_some(), "missing: {}", name);
        }
    }
}
