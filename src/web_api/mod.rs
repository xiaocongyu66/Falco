//! Additional Web Platform APIs for Falco.
//!
//! This module implements the Web APIs that are commonly used by modern
//! web applications but were missing from Falco's TJS runtime:
//!
//! - **TextEncoder / TextDecoder** — UTF-8, UTF-16LE encoding/decoding
//! - **Crypto API** — `crypto.getRandomValues()`, `crypto.subtle.digest()`
//! - **Web Workers** — `new Worker(url)`, `postMessage`, `onmessage`
//! - **MessageChannel / MessagePort** — bidirectional message passing
//! - **BroadcastChannel** — cross-context messaging
//! - **IndexedDB** — async key-value object store
//! - **Compression Streams** — `CompressionStream`, `DecompressionStream`
//! - **URLSearchParams** — URL query string parsing
//! - **AbortController / AbortSignal** — cancelable async operations
//! - **queueMicrotask** — schedule microtask
//! - **Intl** — DateTimeFormat, NumberFormat, Collator, PluralRules, ListFormat, RelativeTimeFormat, Segmenter
//! - **WebAudio** — AudioContext, OscillatorNode, GainNode, AnalyserNode, BiquadFilterNode
//! - **Service Workers** — registration, Cache API, fetch interception
//! - **WOFF/WOFF2** — font decoding (decompresses to sfnt for ab_glyph)
//! - **Proxy** — meta-object with get/set/has/deleteProperty/ownKeys traps
//! - **HSTS** — HTTP Strict-Transport-Security policy enforcement
//! - **DNS** — from-scratch DNS resolver (A, AAAA, CNAME, MX, TXT, NS, SRV, PTR)

pub mod text_codec;
pub mod crypto;
pub mod workers;
pub mod messaging;
pub mod storage;
pub mod compression;
pub mod url;
pub mod abort;
pub mod microtask;
pub mod intl;
pub mod audio;
pub mod service_worker;
pub mod font;
pub mod proxy;
pub mod hsts;
pub mod dns;
pub mod temporal;
pub mod mathml;
pub mod css_typed_om;
pub mod houdini;
pub mod webrtc;
pub mod notifications;
pub mod gamepad;
pub mod speech;
pub mod webusb;
pub mod webserial;
pub mod webbluetooth;
pub mod webcodecs;
pub mod webgpu;
pub mod webtransport;
pub mod screen_capture;
pub mod webauthn;
pub mod local_event_loop;
pub mod es_modules;
pub mod shared_buffer;
pub mod integration_tests;

use crate::tjs::interpreter::Scope;
use crate::tjs::value::Value;

/// Register all Web APIs on the given TJS scope.
///
/// Call this once during context initialization (after `builtins::register`).
pub fn register_web_apis(scope: &mut Scope) {
    text_codec::register(scope);
    crypto::register(scope);
    workers::register(scope);
    messaging::register(scope);
    storage::register(scope);
    compression::register(scope);
    url::register(scope);
    abort::register(scope);
    microtask::register(scope);
    intl::register(scope);
    audio::register(scope);
    service_worker::register(scope);
    proxy::register(scope);
    temporal::register(scope);
    css_typed_om::register(scope);
    houdini::register(scope);
    webrtc::register(scope);
    notifications::register(scope);
    gamepad::register(scope);
    speech::register(scope);
    webusb::register(scope);
    webserial::register(scope);
    webbluetooth::register(scope);
    webcodecs::register(scope);
    webgpu::register(scope);
    webtransport::register(scope);
    screen_capture::register(scope);
    webauthn::register(scope);
    shared_buffer::register(scope);
    es_modules::register_module_runtime(scope);
    // font, hsts, dns, mathml are not JS-exposed (used internally by the renderer).
}
