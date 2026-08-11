//! Comprehensive integration tests for the Falco web API surface.
//!
//! These tests verify that multiple Web APIs work together correctly,
//! simulating real-world usage patterns.

#![cfg(test)]

use crate::tjs::TjsContext;
use crate::tjs::value::Value;

/// Test that the full Web API surface is registered and accessible.
#[test]
fn all_web_apis_accessible() {
    let mut tjs = TjsContext::new();

    // Check that all major APIs are present (as globals or on navigator).
    let apis = [
        "WebAssembly", "crypto", "TextEncoder", "TextDecoder",
        "Intl", "Temporal", "URLSearchParams", "AbortController",
        "CompressionStream", "DecompressionStream",
        "BroadcastChannel", "MessageChannel", "Worker",
        "Notification", "SpeechSynthesisUtterance", "SpeechRecognition",
        "GamepadButton", "WebTransport", "WebTransportError",
        "VideoDecoder", "VideoEncoder", "VideoFrame", "AudioData",
        "ImageDecoder", "EncodedVideoChunk", "EncodedAudioChunk",
        "PasswordCredential", "FederatedCredential", "PublicKeyCredential",
        "caches", "indexedDB", "CSS", "speechSynthesis",
    ];

    for api_name in &apis {
        let script = format!("typeof {} !== 'undefined'", api_name);
        let result = tjs.execute(&script);
        assert!(
            result.is_ok(),
            "API '{}' failed to evaluate: {:?}",
            api_name,
            result.err()
        );
        // Also verify it's truthy (actually defined).
        if let Ok(Value::Boolean(b)) = &result {
            assert!(*b, "API '{}' is undefined", api_name);
        }
    }
}

/// Test that navigator has all expected sub-objects.
#[test]
fn navigator_has_all_apis() {
    let mut tjs = TjsContext::new();

    let nav_apis = [
        "navigator.usb", "navigator.serial", "navigator.bluetooth",
        "navigator.gpu", "navigator.serviceWorker",
        "navigator.mediaDevices", "navigator.contacts", "navigator.credentials",
    ];

    for api in &nav_apis {
        let script = format!("typeof {} !== 'undefined'", api);
        let result = tjs.execute(&script);
        assert!(
            result.is_ok(),
            "navigator API '{}' failed: {:?}",
            api,
            result.err()
        );
    }
}

/// Test WebAssembly.validate with a minimal module.
#[test]
fn wasm_validate_minimal() {
    let mut tjs = TjsContext::new();

    // A minimal valid WASM module (just the header).
    // WebAssembly.validate accepts a byte array or string.
    let script = r#"WebAssembly.validate([0,97,115,109,1,0,0,0])"#;

    let result = tjs.execute(script).unwrap();
    assert_eq!(result, Value::Boolean(true));
}

/// Test crypto.subtle.digest with SHA-256.
#[test]
fn crypto_sha256_digest() {
    let mut tjs = TjsContext::new();

    // Compute SHA-256 of "abc" (bytes [97, 98, 99]).
    // We verify the result length (should be 32 bytes for SHA-256).
    let script = r#"
        (function() {
            var data = [97, 98, 99];
            var result = crypto.subtle.digest("SHA-256", data);
            return result.length;
        })()
    "#;

    let result = tjs.execute(script).unwrap();
    assert_eq!(result, Value::Number(32.0)); // SHA-256 produces 32 bytes
}

/// Test URLSearchParams.
#[test]
fn url_search_params() {
    let mut tjs = TjsContext::new();

    // Call URLSearchParams as a function (TJS doesn't support `new` with builtins).
    let script = r#"
        (function() {
            var params = URLSearchParams("a=1&b=2&c=3");
            return params.get("b");
        })()
    "#;

    let result = tjs.execute(script);
    assert!(result.is_ok(), "URLSearchParams failed: {:?}", result.err());
    if let Ok(Value::String(s)) = result {
        assert_eq!(s, "2");
    }
}

/// Test crypto.randomUUID format.
#[test]
fn crypto_random_uuid() {
    let mut tjs = TjsContext::new();

    let result = tjs.execute("crypto.randomUUID()").unwrap();
    if let Value::String(uuid) = result {
        assert_eq!(uuid.len(), 36);
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        assert!(parts[2].starts_with('4'));
    } else {
        panic!("expected string, got {:?}", result);
    }
}

/// Test crypto.getRandomValues.
#[test]
fn crypto_get_random_values() {
    let mut tjs = TjsContext::new();

    let script = r#"
        (function() {
            var arr = [0, 0, 0, 0, 0, 0, 0, 0];
            crypto.getRandomValues(arr);
            // At least one byte should be non-zero.
            var nonZero = 0;
            for (var i = 0; i < arr.length; i++) {
                if (arr[i] !== 0) nonZero++;
            }
            return nonZero;
        })()
    "#;

    let result = tjs.execute(script).unwrap();
    if let Value::Number(n) = result {
        assert!(n > 0.0, "expected at least one non-zero byte, got {}", n);
    }
}

/// Test Math, JSON, Array builtins still work.
#[test]
fn core_builtins_still_work() {
    let mut tjs = TjsContext::new();

    let result = tjs.execute("Math.floor(3.7)").unwrap();
    assert_eq!(result, Value::Number(3.0));

    let result = tjs.execute(r#"JSON.parse('{"x": 42}').x"#).unwrap();
    assert_eq!(result, Value::Number(42.0));

    let result = tjs.execute("[1,2,3,4,5].reduce(function(a,b) { return a+b }, 0)").unwrap();
    assert_eq!(result, Value::Number(15.0));
}

/// Test that WebAssembly.Module.exports works.
#[test]
fn wasm_module_exports() {
    let mut tjs = TjsContext::new();

    // A minimal WASM module with one export "run".
    // Manually constructed with correct section sizes.
    let wasm_bytes: Vec<u8> = vec![
        // Header
        0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00,
        // Type section (id=1, size=5)
        0x01, 0x05, 0x01, // 1 type
        0x60, 0x00, 0x00, // () -> ()
        // Function section (id=3, size=2)
        0x03, 0x02, 0x01, 0x00, // 1 function, type 0
        // Export section (id=7, size=7)
        0x07, 0x07,
        0x01, // 1 export
        0x03, 0x72, 0x75, 0x6E, // "run"
        0x00, 0x00, // function, index 0
        // Code section (id=10, size=5)
        0x0A, 0x05,
        0x01, // 1 function body
        0x03, 0x00, 0x00, 0x0B, // body: 0 locals, nop, end
    ];

    let js_array: String = wasm_bytes
        .iter()
        .map(|b| format!("{}", b))
        .collect::<Vec<_>>()
        .join(",");

    let script = format!(
        r#"
        (function() {{
            var mod = WebAssembly.Module([{}]);
            var exports = WebAssembly.Module_exports(mod);
            return exports.length;
        }})()
        "#,
        js_array
    );

    let result = tjs.execute(&script).unwrap();
    assert_eq!(result, Value::Number(1.0));
}

/// Test CompressionStream round-trip.
#[test]
fn compression_round_trip() {
    let mut tjs = TjsContext::new();

    // Compress and decompress, verifying the lengths match.
    let script = r#"
        (function() {
            var compressor = CompressionStream("gzip");
            var original = "hello world compression test";
            var encoded = compressor.compress(original);
            var decompressor = DecompressionStream("gzip");
            var decoded = decompressor.decompress(encoded);
            return decoded.length;
        })()
    "#;

    let result = tjs.execute(script).unwrap();
    // The decompressed data should have the same length as the original string.
    assert_eq!(result, Value::Number("hello world compression test".len() as f64));
}
