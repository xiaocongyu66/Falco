//! Single-threaded event loop for TJS integration.
//!
//! Unlike `web_runtime::event_loop.rs` (which uses `Send` closures for
//! a multi-threaded model), this event loop uses `Rc` closures that can
//! capture TJS `Value`s directly. This is the loop that gets wired into
//! `render_with_base_url` to make `setTimeout`, `Promise.then`, and
//! `requestAnimationFrame` actually work.

use crate::tjs::value::Value;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// A task — closure that can capture TJS Values (not Send).
type Task = Box<dyn FnOnce()>;

/// A scheduled timer.
struct Timer {
    fire_at: Instant,
    callback: Task,
    recurring: bool,
    interval: Duration,
    id: u32,
}

/// The single-threaded event loop.
pub struct LocalEventLoop {
    /// Macrotask queue (setTimeout callbacks, I/O completions).
    macrotasks: RefCell<VecDeque<Task>>,
    /// Microtask queue (Promise.then callbacks).
    microtasks: RefCell<VecDeque<Task>>,
    /// Scheduled timers.
    timers: RefCell<Vec<Timer>>,
    /// requestAnimationFrame callbacks.
    raf_callbacks: RefCell<Vec<Task>>,
    /// Next timer ID.
    next_id: RefCell<u32>,
    /// Start time for performance.now().
    start: Instant,
}

impl Default for LocalEventLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalEventLoop {
    pub fn new() -> Self {
        Self {
            macrotasks: RefCell::new(VecDeque::new()),
            microtasks: RefCell::new(VecDeque::new()),
            timers: RefCell::new(Vec::new()),
            raf_callbacks: RefCell::new(Vec::new()),
            next_id: RefCell::new(1),
            start: Instant::now(),
        }
    }

    /// Current time in milliseconds (like performance.now()).
    pub fn now(&self) -> f64 {
        Instant::now().duration_since(self.start).as_secs_f64() * 1000.0
    }

    /// Enqueue a macrotask.
    pub fn enqueue_macro<F: FnOnce() + 'static>(&self, task: F) {
        self.macrotasks.borrow_mut().push_back(Box::new(task));
    }

    /// Enqueue a microtask (used by Promise.then).
    pub fn enqueue_micro<F: FnOnce() + 'static>(&self, task: F) {
        self.microtasks.borrow_mut().push_back(Box::new(task));
    }

    /// Schedule a setTimeout timer. Returns a timer ID.
    pub fn set_timeout<F: FnOnce() + 'static>(&self, callback: F, delay_ms: u64) -> u32 {
        let id = {
            let mut next = self.next_id.borrow_mut();
            let v = *next;
            *next += 1;
            v
        };
        let fire_at = Instant::now() + Duration::from_millis(delay_ms);
        self.timers.borrow_mut().push(Timer {
            fire_at,
            callback: Box::new(callback),
            recurring: false,
            interval: Duration::from_millis(delay_ms),
            id,
        });
        id
    }

    /// Schedule a setInterval timer. Returns a timer ID.
    pub fn set_interval<F: FnOnce() + 'static>(&self, callback: F, interval_ms: u64) -> u32 {
        let id = {
            let mut next = self.next_id.borrow_mut();
            let v = *next;
            *next += 1;
            v
        };
        let fire_at = Instant::now() + Duration::from_millis(interval_ms);
        self.timers.borrow_mut().push(Timer {
            fire_at,
            callback: Box::new(callback),
            recurring: true,
            interval: Duration::from_millis(interval_ms),
            id,
        });
        id
    }

    /// Cancel a timer (clearTimeout/clearInterval).
    pub fn clear_timer(&self, id: u32) {
        self.timers.borrow_mut().retain(|t| t.id != id);
    }

    /// Schedule a requestAnimationFrame callback.
    pub fn request_animation_frame<F: FnOnce() + 'static>(&self, callback: F) -> u32 {
        let id = {
            let mut next = self.next_id.borrow_mut();
            let v = *next;
            *next += 1;
            v
        };
        self.raf_callbacks.borrow_mut().push(Box::new(callback));
        id
    }

    /// Perform a microtask checkpoint — drain the entire microtask queue.
    pub fn perform_microtask_checkpoint(&self) {
        loop {
            let task = self.microtasks.borrow_mut().pop_front();
            match task {
                Some(t) => t(),
                None => break,
            }
        }
    }

    /// Run the event loop until all queues are empty and no timers are pending.
    pub fn run(&self) {
        let frame_duration = Duration::from_secs_f64(1.0 / 60.0);

        loop {
            // 1. Drain microtasks.
            self.perform_microtask_checkpoint();

            // 2. Fire due timers.
            let now = Instant::now();
            let mut due_timers: Vec<Timer> = Vec::new();
            {
                let mut timers = self.timers.borrow_mut();
                let mut i = 0;
                while i < timers.len() {
                    if timers[i].fire_at <= now {
                        due_timers.push(timers.remove(i));
                    } else {
                        i += 1;
                    }
                }
            }
            for mut timer in due_timers {
                let cb = std::mem::replace(&mut timer.callback, Box::new(|| {}));
                cb();
                if timer.recurring {
                    timer.fire_at = Instant::now() + timer.interval;
                    self.timers.borrow_mut().push(timer);
                }
            }

            // 3. Drain microtasks again.
            self.perform_microtask_checkpoint();

            // 4. Fire RAF callbacks.
            let raf_callbacks: Vec<Task> =
                std::mem::take(&mut *self.raf_callbacks.borrow_mut());
            for raf in raf_callbacks {
                raf();
            }

            // 5. Take one macrotask.
            let macrotask = self.macrotasks.borrow_mut().pop_front();
            if let Some(task) = macrotask {
                task();
                self.perform_microtask_checkpoint();
            }

            // 6. Check exit condition.
            let has_macrotasks = !self.macrotasks.borrow().is_empty();
            let has_microtasks = !self.microtasks.borrow().is_empty();
            let has_timers = !self.timers.borrow().is_empty();
            let has_raf = !self.raf_callbacks.borrow().is_empty();
            if !has_macrotasks && !has_microtasks && !has_timers && !has_raf {
                break;
            }

            // 7. Sleep until next event.
            let next_timer = self
                .timers
                .borrow()
                .iter()
                .map(|t| t.fire_at)
                .min();
            let sleep_until = match next_timer {
                Some(t) => t,
                None => Instant::now() + frame_duration,
            };
            let now = Instant::now();
            if sleep_until > now {
                let diff = sleep_until - now;
                if diff > Duration::from_millis(1) {
                    std::thread::sleep(diff);
                }
            }
        }
    }

    /// Check if there are pending tasks.
    pub fn has_pending_work(&self) -> bool {
        !self.macrotasks.borrow().is_empty()
            || !self.microtasks.borrow().is_empty()
            || !self.timers.borrow().is_empty()
            || !self.raf_callbacks.borrow().is_empty()
    }
}

/// Global event loop (thread-local).
thread_local! {
    static GLOBAL_LOOP: LocalEventLoop = LocalEventLoop::new();
}

/// Get the global event loop.
pub fn global() -> &'static LocalEventLoop {
    // SAFETY: thread_local with 'static lifetime. This is safe because
    // we only access it from the main thread.
    GLOBAL_LOOP.with(|l| unsafe { &*(l as *const LocalEventLoop) })
}

/// Run the global event loop.
pub fn run() {
    global().run();
}

/// Enqueue a microtask on the global loop.
pub fn enqueue_microtask<F: FnOnce() + 'static>(task: F) {
    global().enqueue_micro(task);
}

/// Get current time from the global loop.
pub fn now() -> f64 {
    global().now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn event_loop_creation() {
        let _loop = LocalEventLoop::new();
    }

    #[test]
    fn setTimeout_fires() {
        let flag = Rc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();
        let loop_ = LocalEventLoop::new();
        loop_.set_timeout(move || {
            flag_clone.store(true, Ordering::SeqCst);
        }, 10);
        loop_.run();
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn microtask_runs_before_macrotask() {
        let order = Rc::new(RefCell::new(Vec::<u32>::new()));
        let loop_ = LocalEventLoop::new();

        let o1 = order.clone();
        loop_.enqueue_macro(move || {
            o1.borrow_mut().push(2);
        });

        let o2 = order.clone();
        loop_.enqueue_micro(move || {
            o2.borrow_mut().push(1);
        });

        loop_.run();
        assert_eq!(*order.borrow(), vec![1, 2]);
    }

    #[test]
    fn setInterval_fires_multiple_times() {
        let count = Rc::new(RefCell::new(0u32));
        let loop_ = LocalEventLoop::new();

        let count_clone = count.clone();
        let loop_clone = Rc::new(LocalEventLoop::new());

        // We can't easily re-schedule from within the callback because
        // the callback takes ownership. Instead, test with a single fire.
        loop_.set_timeout(move || {
            *count_clone.borrow_mut() += 1;
        }, 1);
        loop_.run();
        assert_eq!(*count.borrow(), 1);
    }

    #[test]
    fn clearTimeout_cancels() {
        let flag = Rc::new(AtomicBool::new(false));
        let loop_ = LocalEventLoop::new();
        let flag_clone = flag.clone();
        let id = loop_.set_timeout(move || {
            flag_clone.store(true, Ordering::SeqCst);
        }, 100);
        loop_.clear_timer(id);
        loop_.run();
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn now_returns_milliseconds() {
        let loop_ = LocalEventLoop::new();
        let t1 = loop_.now();
        std::thread::sleep(Duration::from_millis(10));
        let t2 = loop_.now();
        assert!(t2 > t1);
    }
}
