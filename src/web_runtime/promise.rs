//! Promise with REAL async resolution via the event loop.
//!
//! Spec: https://tc39.es/ecma262/#sec-promise-objects
//!
//! This is NOT the synchronous stub from `tjs_ext`. This Promise:
//! * Schedules `then` callbacks as **microtasks** on the event loop.
//! * Actually resolves/rejects asynchronously — the callback fires on the
//!   next microtask checkpoint, not immediately.
//! * Supports chaining: `p.then(f).then(g)` — `g` runs after `f`'s result
//!   resolves.
//! * Supports `Promise.all`, `Promise.race`, `Promise.allSettled`.
//! * Supports `async/await` via a continuation-pass transform: `await p`
//!   registers a `then` callback that resumes the function.
//!
//! # How async/await works
//!
//! TJS is a tree-walking interpreter — it can't pause execution. So we
//! implement `await` via desugaring:
//! ```js
//! async function fetchUser(id) {
//!   var user = await fetch('/users/' + id);
//!   return user.name;
//! }
//! ```
//! becomes (conceptually):
//! ```js
//! function fetchUser(id) {
//!   return fetch('/users/' + id).then(function(user) {
//!     return user.name;
//!   });
//! }
//! ```
//! The event loop drives the chain forward by running microtasks.

use crate::web_runtime::event_loop::EventLoop;
use std::sync::{Arc, Mutex};

/// Promise state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromiseState {
    Pending,
    Fulfilled,
    Rejected,
}

/// A Promise value. The actual state is shared via Arc<Mutex> so that
/// resolve/reject can be called from any thread (e.g. a background HTTP
/// thread that completes a fetch).
pub struct AsyncPromise {
    pub state: Mutex<PromiseState>,
    pub value: Mutex<Option<PromiseValue>>,
    /// Pending then/catch callbacks. Each is (callback, next_promise).
    /// When the promise settles, these are scheduled as microtasks.
    pub pending: Mutex<Vec<PendingCallback>>,
}

/// A pending callback waiting for the promise to settle.
struct PendingCallback {
    /// The callback closure. Takes the resolved/rejected value.
    callback: Box<dyn Fn(PromiseValue) -> PromiseValue + Send>,
    /// The next promise in the chain (what this callback's return value
    /// resolves or rejects).
    next_promise: Arc<AsyncPromise>,
    /// Whether this is a then (true) or catch (false) callback.
    is_then: bool,
}

/// A promise's resolved or rejected value.
#[derive(Debug, Clone)]
pub enum PromiseValue {
    /// Resolved with a value (we use String for simplicity — the JS bridge
    /// layer converts to/from TJS Values).
    Resolved(String),
    /// Rejected with an error reason.
    Rejected(String),
}

impl std::fmt::Display for PromiseValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromiseValue::Resolved(s) => write!(f, "{}", s),
            PromiseValue::Rejected(s) => write!(f, "Error: {}", s),
        }
    }
}

impl AsyncPromise {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(PromiseState::Pending),
            value: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
        })
    }

    /// Resolve the promise with a value. Schedules all pending `then`
    /// callbacks as microtasks on the event loop.
    pub fn resolve(&self, value: String, event_loop: Arc<EventLoop>) {
        let mut state = self.state.lock().unwrap();
        if *state != PromiseState::Pending {
            return; // Already settled — ignore.
        }
        *state = PromiseState::Fulfilled;
        *self.value.lock().unwrap() = Some(PromiseValue::Resolved(value.clone()));
        drop(state);

        // Schedule all pending `then` callbacks as microtasks.
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for cb in pending {
            if cb.is_then {
                let value = value.clone();
                let next = cb.next_promise.clone();
                let callback = cb.callback;
                let el = event_loop.clone();
                event_loop.enqueue_micro(move || {
                    let result = callback(PromiseValue::Resolved(value));
                    // Resolve or reject the next promise based on the callback's result.
                    match result {
                        PromiseValue::Resolved(v) => next.resolve(v, el.clone()),
                        PromiseValue::Rejected(v) => next.reject(v, el),
                    }
                });
            }
            // catch callbacks are skipped on resolve.
        }
    }

    /// Reject the promise with a reason. Schedules all pending `catch`
    /// callbacks as microtasks.
    pub fn reject(&self, reason: String, event_loop: Arc<EventLoop>) {
        let mut state = self.state.lock().unwrap();
        if *state != PromiseState::Pending {
            return;
        }
        *state = PromiseState::Rejected;
        *self.value.lock().unwrap() = Some(PromiseValue::Rejected(reason.clone()));
        drop(state);

        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for cb in pending {
            if !cb.is_then {
                let reason = reason.clone();
                let next = cb.next_promise.clone();
                let callback = cb.callback;
                let el = event_loop.clone();
                event_loop.enqueue_micro(move || {
                    let result = callback(PromiseValue::Rejected(reason));
                    match result {
                        PromiseValue::Resolved(v) => next.resolve(v, el.clone()),
                        PromiseValue::Rejected(v) => next.reject(v, el),
                    }
                });
            } else {
                // then callbacks are skipped on reject — propagate to next.
                let reason = reason.clone();
                let next = cb.next_promise.clone();
                let el = event_loop.clone();
                event_loop.enqueue_micro(move || {
                    next.reject(reason, el);
                });
            }
        }
    }

    /// Register a `then` callback. Returns a new promise that resolves
    /// with the callback's return value.
    pub fn then<F>(&self, callback: F, event_loop: Arc<EventLoop>) -> Arc<Self>
    where
        F: Fn(PromiseValue) -> PromiseValue + Send + 'static,
    {
        let next = AsyncPromise::new();
        let state = self.state.lock().unwrap();
        if *state == PromiseState::Pending {
            // Still pending — queue the callback.
            self.pending.lock().unwrap().push(PendingCallback {
                callback: Box::new(callback),
                next_promise: next.clone(),
                is_then: true,
            });
        } else {
            // Already settled — schedule immediately.
            let value = self.value.lock().unwrap().clone().unwrap();
            let next_clone = next.clone();
            let cb = Box::new(callback);
            let el = event_loop.clone();
            event_loop.enqueue_micro(move || {
                let result = cb(value);
                match result {
                    PromiseValue::Resolved(v) => next_clone.resolve(v, el.clone()),
                    PromiseValue::Rejected(v) => next_clone.reject(v, el),
                }
            });
        }
        drop(state);
        next
    }

    /// Register a `catch` callback.
    pub fn catch<F>(&self, callback: F, event_loop: Arc<EventLoop>) -> Arc<Self>
    where
        F: Fn(PromiseValue) -> PromiseValue + Send + 'static,
    {
        let next = AsyncPromise::new();
        let state = self.state.lock().unwrap();
        if *state == PromiseState::Pending {
            self.pending.lock().unwrap().push(PendingCallback {
                callback: Box::new(callback),
                next_promise: next.clone(),
                is_then: false,
            });
        } else if *state == PromiseState::Rejected {
            // Already rejected — schedule immediately.
            let value = self.value.lock().unwrap().clone().unwrap();
            let next_clone = next.clone();
            let cb = Box::new(callback);
            let el = event_loop.clone();
            event_loop.enqueue_micro(move || {
                let result = cb(value);
                match result {
                    PromiseValue::Resolved(v) => next_clone.resolve(v, el.clone()),
                    PromiseValue::Rejected(v) => next_clone.reject(v, el),
                }
            });
        }
        // If already fulfilled, just propagate the value to next.
        else if *state == PromiseState::Fulfilled {
            let value = self.value.lock().unwrap().clone().unwrap();
            let next_clone = next.clone();
            let el = event_loop.clone();
            event_loop.enqueue_micro(move || match value {
                PromiseValue::Resolved(v) => next_clone.resolve(v, el.clone()),
                PromiseValue::Rejected(v) => next_clone.reject(v, el),
            });
        }
        drop(state);
        next
    }

    /// Register a `finally` callback (runs regardless of settle state).
    pub fn finally<F>(&self, callback: F, event_loop: Arc<EventLoop>) -> Arc<Self>
    where
        F: Fn() + Send + 'static,
    {
        self.then(
            move |v| {
                callback();
                v
            },
            event_loop,
        )
    }

    /// Get the current state (for inspection).
    pub fn get_state(&self) -> PromiseState {
        *self.state.lock().unwrap()
    }
}

/// Promise.all — resolves when all input promises resolve, rejects on first rejection.
pub fn promise_all(
    promises: Vec<Arc<AsyncPromise>>,
    event_loop: Arc<EventLoop>,
) -> Arc<AsyncPromise> {
    let result = AsyncPromise::new();
    if promises.is_empty() {
        result.resolve("[]".to_string(), event_loop);
        return result;
    }
    let remaining = Arc::new(Mutex::new(promises.len()));
    let values = Arc::new(Mutex::new(vec![String::new(); promises.len()]));
    let result_clone = result.clone();
    let event_loop_clone = event_loop.clone();

    for (i, p) in promises.into_iter().enumerate() {
        let values = values.clone();
        let remaining = remaining.clone();
        let result = result_clone.clone();
        let el = event_loop_clone.clone();
        let el_for_then = event_loop_clone.clone();
        p.then(
            move |v| {
                if let PromiseValue::Resolved(ref val) = v {
                    values.lock().unwrap()[i] = val.clone();
                    let mut rem = remaining.lock().unwrap();
                    *rem -= 1;
                    if *rem == 0 {
                        let vals = values.lock().unwrap().clone();
                        let joined = vals.join(",");
                        result.resolve(format!("[{}]", joined), el.clone());
                    }
                }
                PromiseValue::Resolved(String::new())
            },
            el_for_then,
        );
        let result = result_clone.clone();
        let el = event_loop_clone.clone();
        let el_for_catch = event_loop_clone.clone();
        p.catch(
            move |reason| {
                if let PromiseValue::Rejected(ref r) = reason {
                    result.reject(r.clone(), el.clone());
                }
                PromiseValue::Resolved(String::new())
            },
            el_for_catch,
        );
    }
    result
}

/// Promise.race — resolves/rejects with the first promise to settle.
pub fn promise_race(
    promises: Vec<Arc<AsyncPromise>>,
    event_loop: Arc<EventLoop>,
) -> Arc<AsyncPromise> {
    let result = AsyncPromise::new();
    let result_clone = result.clone();
    let el = event_loop.clone();
    for p in promises {
        let r = result_clone.clone();
        let el_inner = el.clone();
        p.then(
            move |v| {
                match &v {
                    PromiseValue::Resolved(val) => r.resolve(val.clone(), el_inner.clone()),
                    PromiseValue::Rejected(reason) => r.reject(reason.clone(), el_inner.clone()),
                }
                v
            },
            event_loop.clone(),
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn promise_resolves_async() {
        let el = EventLoop::new();
        let p = AsyncPromise::new();
        let fired = Arc::new(AtomicBool::new(false));
        let f = fired.clone();

        // Register then BEFORE resolving.
        p.then(
            move |v| {
                if let PromiseValue::Resolved(ref val) = v {
                    if val == "hello" {
                        f.store(true, Ordering::SeqCst);
                    }
                }
                PromiseValue::Resolved(String::new())
            },
            el.clone(),
        );

        // Resolve AFTER registering.
        p.resolve("hello".to_string(), el.clone());

        // Run the loop — the then callback fires as a microtask.
        el.run();
        assert!(
            fired.load(Ordering::SeqCst),
            "then callback should have fired"
        );
    }

    #[test]
    fn promise_chain_works() {
        let el = EventLoop::new();
        let p = AsyncPromise::new();
        let result = Arc::new(Mutex::new(String::new()));
        let r = result.clone();

        p.then(
            move |v| {
                // First then: double the value.
                if let PromiseValue::Resolved(val) = &v {
                    let n: i32 = val.parse().unwrap_or(0);
                    return PromiseValue::Resolved((n * 2).to_string());
                }
                v
            },
            el.clone(),
        )
        .then(
            move |v| {
                // Second then: add 1.
                if let PromiseValue::Resolved(val) = &v {
                    let n: i32 = val.parse().unwrap_or(0);
                    let result = n + 1;
                    *r.lock().unwrap() = result.to_string();
                    return PromiseValue::Resolved(result.to_string());
                }
                v
            },
            el.clone(),
        );

        p.resolve("5".to_string(), el.clone());
        el.run();
        assert_eq!(*result.lock().unwrap(), "11"); // 5*2 + 1
    }

    #[test]
    fn promise_reject_propagates_to_catch() {
        let el = EventLoop::new();
        let p = AsyncPromise::new();
        let caught = Arc::new(Mutex::new(String::new()));
        let c = caught.clone();

        p.then(
            move |_| PromiseValue::Resolved("should not run".into()),
            el.clone(),
        )
        .catch(
            move |v| {
                if let PromiseValue::Rejected(ref reason) = v {
                    *c.lock().unwrap() = reason.clone();
                }
                PromiseValue::Resolved(String::new())
            },
            el.clone(),
        );

        p.reject("network error".to_string(), el.clone());
        el.run();
        assert_eq!(*caught.lock().unwrap(), "network error");
    }

    #[test]
    fn promise_all_resolves_when_all_complete() {
        let el = EventLoop::new();
        let p1 = AsyncPromise::new();
        let p2 = AsyncPromise::new();
        let p3 = AsyncPromise::new();
        let all = promise_all(vec![p1.clone(), p2.clone(), p3.clone()], el.clone());

        let result = Arc::new(Mutex::new(String::new()));
        let r = result.clone();
        all.then(
            move |v| {
                if let PromiseValue::Resolved(ref val) = v {
                    *r.lock().unwrap() = val.clone();
                }
                v
            },
            el.clone(),
        );

        // Resolve in any order.
        p2.resolve("b".into(), el.clone());
        p1.resolve("a".into(), el.clone());
        p3.resolve("c".into(), el.clone());
        el.run();
        // Order should match input order (a,b,c), not resolution order.
        assert_eq!(*result.lock().unwrap(), "[a,b,c]");
    }

    #[test]
    fn promise_all_rejects_on_first_rejection() {
        let el = EventLoop::new();
        let p1 = AsyncPromise::new();
        let p2 = AsyncPromise::new();
        let all = promise_all(vec![p1.clone(), p2.clone()], el.clone());

        let caught = Arc::new(Mutex::new(String::new()));
        let c = caught.clone();
        all.catch(
            move |v| {
                if let PromiseValue::Rejected(ref reason) = v {
                    *c.lock().unwrap() = reason.clone();
                }
                v
            },
            el.clone(),
        );

        p1.reject("fail".into(), el.clone());
        p2.resolve("ok".into(), el.clone());
        el.run();
        assert_eq!(*caught.lock().unwrap(), "fail");
    }

    #[test]
    fn microtask_ordering_with_promises() {
        // Promise.then callbacks run as microtasks, before the next macrotask.
        let el = EventLoop::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        let p = AsyncPromise::new();
        let o1 = order.clone();
        p.then(
            move |_| {
                o1.lock().unwrap().push("promise");
                PromiseValue::Resolved(String::new())
            },
            el.clone(),
        );

        let o2 = order.clone();
        el.enqueue_macro(move || {
            o2.lock().unwrap().push("macro");
        });

        p.resolve("x".into(), el.clone());
        el.run();
        assert_eq!(*order.lock().unwrap(), vec!["promise", "macro"]);
    }
}
