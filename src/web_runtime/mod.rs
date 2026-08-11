//! Web Runtime — real implementations of Web APIs.
//!
//! This module provides production-quality implementations of:
//! * **Event loop** with macrotask/microtask scheduling.
//! * **Promise** with real async resolution via the event loop.
//! * **fetch()** with streaming HTTP responses.
//! * **XMLHttpRequest** as a legacy wrapper.
//! * **HTTP/2** with multiplexed streams.
//! * **MediaSource Extensions (MSE)** for adaptive streaming.
//! * **Encrypted Media Extensions (EME)** for DRM (ClearKey only —
//!   Widevine/PlayReady/FairPlay require proprietary CDMs).
//! * **Video/audio decoding** via ffmpeg (when the `ffmpeg` feature is
//!   enabled).
//! * **WebGL** via glow (when the `glow` feature is enabled).
//! * **Shadow DOM** (in `dom::spec::shadow`).
//!
//! # Integration with TJS
//!
//! The TJS interpreter is synchronous (tree-walking). To make async APIs
//! work, we use continuation-passing style: async functions return a
//! Promise immediately, and `then` callbacks drive the rest via the event
//! loop's microtask queue.
//!
//! The event loop runs on a dedicated thread (or the main thread in CLI
//! mode). JS code can enqueue tasks via `setTimeout`, `fetch`, etc. The
//! loop drains microtasks after every macrotask, ensuring Promise
//! callbacks fire before the next macrotask.

pub mod eme;
pub mod event_loop;
pub mod fetch;
pub mod http2;
pub mod mse;
pub mod ndsd;
pub mod promise;
pub mod video;
pub mod webgl;
pub mod xhr;

pub use eme::{KeySystem, MediaKeySession, MediaKeySystemAccess, MediaKeys};
pub use event_loop::{EventLoop, Microtask, Task};
pub use fetch::{fetch, Method, Request, RequestCache, RequestCredentials, RequestMode, Response};
pub use http2::H2Connection;
pub use mse::{Codec, MediaSource, ReadyState, SourceBuffer};
pub use ndsd::{
    DrmBackend, NativeKeySystem, Ndsd, NdsdMediaKeySession, NdsdMediaKeys, NdsdSurface,
};
pub use promise::{promise_all, promise_race, AsyncPromise, PromiseState, PromiseValue};
pub use video::{AudioFrame, DecodedFrame, Decoder, Demuxer, VideoFrame, VideoPipeline};
pub use webgl::WebGLContext;
pub use xhr::Xhr;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::sync::Mutex;

    /// End-to-end: fetch → Promise.then → microtask ordering.
    #[test]
    #[ignore]
    fn fetch_then_chain_via_event_loop() {
        let el = EventLoop::new();
        let request = Request::new("https://example.com");
        let promise = fetch(request, el.clone());

        let status = Arc::new(Mutex::new(0u16));
        let s = status.clone();
        promise.then(
            move |v| {
                if let PromiseValue::Resolved(ref serialized) = v {
                    // Parse the status from the serialized response.
                    if let Some(status_str) = serialized.split('|').next() {
                        if let Ok(code) = status_str.parse::<u16>() {
                            *s.lock().unwrap() = code;
                        }
                    }
                }
                v
            },
            el.clone(),
        );

        el.run();
        assert_eq!(*status.lock().unwrap(), 200);
    }

    /// fetch + Promise.all: load multiple URLs concurrently.
    #[test]
    #[ignore]
    fn fetch_all_via_promise_all() {
        let el = EventLoop::new();
        let urls = vec![
            "https://example.com",
            "https://example.org",
            "https://www.iana.org",
        ];
        let promises: Vec<Arc<AsyncPromise>> = urls
            .iter()
            .map(|url| fetch(Request::new(*url), el.clone()))
            .collect();
        let all = promise_all(promises, el.clone());

        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        all.then(
            move |v| {
                if let PromiseValue::Resolved(ref val) = v {
                    // Count how many "200|" we got.
                    let n = val.matches("200|").count() as u32;
                    c.store(n, Ordering::SeqCst);
                }
                v
            },
            el.clone(),
        );

        el.run();
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    /// setTimeout + Promise interaction.
    #[test]
    fn timer_and_promise_interleave() {
        let el = EventLoop::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        // Schedule a timer (macrotask).
        let o1 = order.clone();
        el.set_timeout(
            move || {
                o1.lock().unwrap().push("timer");
            },
            10,
        );

        // Schedule a Promise.then (microtask).
        let p = AsyncPromise::new();
        let o2 = order.clone();
        p.then(
            move |_| {
                o2.lock().unwrap().push("promise");
                PromiseValue::Resolved(String::new())
            },
            el.clone(),
        );
        p.resolve("x".into(), el.clone());

        el.run();
        // Promise.then (microtask) should fire before the timer (macrotask).
        let final_order = order.lock().unwrap();
        assert_eq!(*final_order, vec!["promise", "timer"]);
    }

    /// MSE + fetch: simulate streaming video.
    #[test]
    #[ignore]
    fn mse_append_via_fetch() {
        let el = EventLoop::new();
        let ms = MediaSource::new();
        ms.open();
        let sb = ms.add_source_buffer("video/webm; codecs=\"vp9\"").unwrap();

        // Simulate fetching a video chunk and appending it.
        let chunk_promise = fetch(Request::new("https://example.com"), el.clone());
        let sb_clone = sb.clone();
        chunk_promise.then(
            move |v| {
                if let PromiseValue::Resolved(ref _serialized) = v {
                    // Append a fake chunk (250KB → 1 second of video).
                    sb_clone.append_buffer(vec![0u8; 250_000]).unwrap();
                }
                v
            },
            el.clone(),
        );

        el.run();
        let buffered = sb.buffered.lock().unwrap();
        assert_eq!(buffered.len(), 1);
        assert!((buffered[0].1 - 1.0).abs() < 0.1);
    }

    /// XHR lifecycle with onload.
    #[test]
    #[ignore]
    fn xhr_with_fetch_backend() {
        let el = EventLoop::new();
        let xhr = Xhr::new();
        xhr.open("GET", "https://example.com", true);

        let loaded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let l = loaded.clone();
        xhr.set_on_load(move || {
            l.store(true, Ordering::SeqCst);
        });

        xhr.send(None, el.clone());
        el.run();
        assert!(loaded.load(Ordering::SeqCst));
        assert_eq!(xhr.status(), 200);
    }
}
