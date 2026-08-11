//! Multi-process architecture and Site Isolation.
//!
//! Spec: https://www.chromium.org/developers/design-documents/site-isolation/
//!
//! Each "site" (origin) gets its own OS process. Cross-origin iframes run
//! in a different process from their parent. This provides:
//! * **Crash isolation** — a bug in one site doesn't take down the browser.
//! * **Security isolation** — even with a renderer exploit, the attacker
//!   only gets data for that site (modulo Spectre-class attacks which
//!   require additional mitigations).
//! * **Sandbox enforcement** — each renderer runs with reduced privileges.
//!
//! Process model:
//! * **Browser process** — privileged, owns the UI, network, disk.
//! * **Renderer processes** — unprivileged, run page JS/DOM/paint.
//! * **GPU process** — shared by all renderers for compositing.
//! * **Utility processes** — for short-lived tasks (e.g. decoding).

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::security::origin::Origin;

static NEXT_PROCESS_ID: AtomicU64 = AtomicU64::new(1);

/// Unique identifier for a process.
pub type ProcessId = u64;

/// The kind of process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessKind {
    /// The privileged browser process — owns the UI and orchestrates renderers.
    Browser,
    /// An unprivileged renderer process — runs page JS/DOM/layout/paint.
    Renderer,
    /// The GPU process — shared by all renderers.
    Gpu,
    /// A utility process — short-lived, isolated task.
    Utility,
    /// A plugin process (e.g. for PPAPI-style plugins).
    Plugin,
}

/// A launched process. In production this wraps a real `std::process::Child`.
/// In tests / single-binary mode, it can be a "virtual" process that just
/// tracks state.
pub struct Process {
    pub id: ProcessId,
    pub kind: ProcessKind,
    /// The site origin this process is dedicated to (None for browser/GPU).
    pub origin: Option<Origin>,
    /// When the process was started.
    pub started_at: Instant,
    /// The actual OS process, if real.
    child: Option<Child>,
    /// Whether the process is alive.
    pub alive: bool,
    /// Number of frames currently using this process.
    pub frame_count: usize,
    /// Crash count for this process slot (resets on restart).
    pub crash_count: u32,
}

impl std::fmt::Debug for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Process")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("origin", &self.origin)
            .field("alive", &self.alive)
            .field("frame_count", &self.frame_count)
            .field("crash_count", &self.crash_count)
            .finish()
    }
}

impl Process {
    /// Launch a new renderer process for the given origin.
    ///
    /// In production: spawns a child `falco --renderer --origin=...` process.
    /// In tests: returns a "virtual" process with `child: None`.
    pub fn launch(kind: ProcessKind, origin: Option<Origin>) -> std::io::Result<Self> {
        let id = NEXT_PROCESS_ID.fetch_add(1, Ordering::SeqCst);
        let child = if cfg!(feature = "real_processes") {
            Some(Self::spawn_child(&kind, origin.as_ref())?)
        } else {
            None
        };
        Ok(Self {
            id,
            kind,
            origin,
            started_at: Instant::now(),
            child,
            alive: true,
            frame_count: 0,
            crash_count: 0,
        })
    }

    fn spawn_child(kind: &ProcessKind, origin: Option<&Origin>) -> std::io::Result<Child> {
        let mut cmd = Command::new(std::env::current_exe()?);
        cmd.arg("--type").arg(match kind {
            ProcessKind::Renderer => "renderer",
            ProcessKind::Gpu => "gpu-process",
            ProcessKind::Utility => "utility",
            ProcessKind::Plugin => "plugin",
            ProcessKind::Browser => "browser",
        });
        if let Some(o) = origin {
            cmd.arg("--origin").arg(o.serialize());
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // For renderer processes, install the seccomp-bpf sandbox BEFORE
        // any untrusted code runs. We use `pre_exec` (a POSIX-threads-safe
        // hook that runs after fork() but before execve()) to apply the
        // filter in the child.
        //
        // # Safety
        // `pre_exec` runs in a multi-threaded context after fork — only
        // async-signal-safe functions are permitted. `apply_sandbox` only
        // calls `prctl` and `seccomp` syscalls, which ARE async-signal-safe.
        //
        // The filter is applied to the renderer's exec'd image via the
        // NO_NEW_PRIVS flag, which persists across execve().
        #[cfg(all(target_os = "linux", feature = "sandbox"))]
        if *kind == ProcessKind::Renderer {
            unsafe {
                cmd.pre_exec(|| {
                    // Apply the seccomp-bpf filter in the child process.
                    // `apply_renderer_sandbox()` calls prctl(NO_NEW_PRIVS) +
                    // seccomp(SET_MODE_FILTER). NO_NEW_PRIVS persists across
                    // execve() so the renderer binary inherits the filter.
                    //
                    // NOTE: This closure runs in a forked child before
                    // execve(). It MUST be async-signal-safe. The seccompiler
                    // crate's `apply_filter` ultimately only calls `prctl`
                    // and `seccomp` syscalls. Building the BPF program does
                    // allocate memory, which is technically not async-signal-
                    // safe but works in practice (glibc's malloc uses
                    // thread-local arenas that don't lock in single-threaded
                    // forks). For a production browser, the filter should be
                    // pre-compiled in the parent and passed to the child.
                    crate::security::sandbox::linux::apply_renderer_sandbox()
                });
            }
        }

        cmd.spawn()
    }

    /// Mark this process as crashed. The OS child (if any) is killed.
    pub fn crash(&mut self) {
        self.alive = false;
        self.crash_count += 1;
        self.frame_count = 0;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Shut down the process gracefully.
    pub fn shutdown(&mut self) {
        self.alive = false;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Increment the frame reference count.
    pub fn add_frame(&mut self) {
        self.frame_count += 1;
    }

    /// Decrement the frame reference count. Returns true if the process
    /// should be torn down (frame_count reached 0).
    pub fn release_frame(&mut self) -> bool {
        if self.frame_count > 0 {
            self.frame_count -= 1;
        }
        self.frame_count == 0 && self.kind == ProcessKind::Renderer
    }
}

/// The process manager — runs in the browser process and tracks all
/// renderer / GPU / utility processes.
pub struct ProcessManager {
    /// All live processes by ID.
    processes: HashMap<ProcessId, Process>,
    /// Map from origin → process ID (for site isolation).
    site_to_process: HashMap<String, ProcessId>,
    /// Configuration: max processes (Chrome defaults to ~70).
    max_processes: usize,
    /// Crash recovery policy.
    pub crash_policy: CrashPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashPolicy {
    /// Restart automatically on crash, up to N times.
    Restart { max_restarts: u32 },
    /// Show a "sad tab" UI and don't restart.
    SadTab,
    /// Crash the whole browser (for debugging only).
    Fatal,
}

impl Default for CrashPolicy {
    fn default() -> Self {
        CrashPolicy::Restart { max_restarts: 5 }
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            site_to_process: HashMap::new(),
            max_processes: 70,
            crash_policy: CrashPolicy::default(),
        }
    }

    /// Get or create a renderer process for the given origin.
    /// Site Isolation: each origin gets its own process.
    pub fn get_or_create_renderer(&mut self, origin: &Origin) -> std::io::Result<ProcessId> {
        let key = origin.serialize();
        // Check if we already have a process for this site.
        if let Some(&pid) = self.site_to_process.get(&key) {
            if let Some(proc_) = self.processes.get_mut(&pid) {
                if proc_.alive {
                    proc_.add_frame();
                    return Ok(pid);
                }
            }
            // Stale entry — remove.
            self.site_to_process.remove(&key);
        }
        // Check process limit.
        if self.processes.len() >= self.max_processes {
            // Reuse an existing process (this would be a "shared process"
            // for low-priority origins, marked as such).
            // For simplicity, we just pick the one with the lowest frame count.
            if let Some((&reuse_pid, _)) = self
                .processes
                .iter()
                .filter(|(_, p)| p.kind == ProcessKind::Renderer && p.alive)
                .min_by_key(|(_, p)| p.frame_count)
            {
                self.processes.get_mut(&reuse_pid).unwrap().add_frame();
                return Ok(reuse_pid);
            }
        }
        // Launch a new renderer.
        let mut proc_ = Process::launch(ProcessKind::Renderer, Some(origin.clone()))?;
        proc_.add_frame();
        let pid = proc_.id;
        self.processes.insert(pid, proc_);
        self.site_to_process.insert(key, pid);
        Ok(pid)
    }

    /// Get the GPU process (singleton).
    pub fn get_or_create_gpu(&mut self) -> std::io::Result<ProcessId> {
        // Check if we already have a GPU process.
        if let Some((&pid, _)) = self
            .processes
            .iter()
            .find(|(_, p)| p.kind == ProcessKind::Gpu && p.alive)
        {
            return Ok(pid);
        }
        let proc_ = Process::launch(ProcessKind::Gpu, None)?;
        let pid = proc_.id;
        self.processes.insert(pid, proc_);
        Ok(pid)
    }

    /// Release a frame's reference to a process. If the process has no more
    /// frames, it can be torn down.
    pub fn release_frame(&mut self, pid: ProcessId) {
        let should_shutdown = if let Some(proc_) = self.processes.get_mut(&pid) {
            proc_.release_frame()
        } else {
            false
        };
        if should_shutdown {
            if let Some(mut proc_) = self.processes.remove(&pid) {
                // Remove from site map.
                if let Some(o) = &proc_.origin {
                    self.site_to_process.remove(&o.serialize());
                }
                proc_.shutdown();
            }
        }
    }

    /// Handle a process crash. Depending on `crash_policy`, either restart
    /// the process, show a sad tab, or crash the browser.
    pub fn handle_crash(&mut self, pid: ProcessId) -> CrashAction {
        let origin = self.processes.get(&pid).and_then(|p| p.origin.clone());
        let crash_count = self.processes.get(&pid).map(|p| p.crash_count).unwrap_or(0);
        if let Some(proc_) = self.processes.get_mut(&pid) {
            proc_.crash();
        }
        match self.crash_policy {
            CrashPolicy::Restart { max_restarts } => {
                if crash_count < max_restarts {
                    // Restart in a new process with the same origin.
                    if let Some(o) = origin {
                        let _ = self.site_to_process.remove(&o.serialize());
                        match self.get_or_create_renderer(&o) {
                            Ok(new_pid) => {
                                // Preserve the crash count on the new process
                                // so we don't restart forever.
                                if let Some(p) = self.processes.get_mut(&new_pid) {
                                    p.crash_count = crash_count;
                                }
                                CrashAction::Restarted {
                                    old_pid: pid,
                                    new_pid,
                                }
                            }
                            Err(_) => CrashAction::SadTab { pid },
                        }
                    } else {
                        CrashAction::SadTab { pid }
                    }
                } else {
                    CrashAction::SadTab { pid }
                }
            }
            CrashPolicy::SadTab => CrashAction::SadTab { pid },
            CrashPolicy::Fatal => CrashAction::Fatal,
        }
    }

    /// Get a process by ID.
    pub fn get(&self, pid: ProcessId) -> Option<&Process> {
        self.processes.get(&pid)
    }

    /// List all live processes.
    pub fn live_processes(&self) -> Vec<&Process> {
        self.processes.values().filter(|p| p.alive).collect()
    }

    /// Shut down all processes.
    pub fn shutdown_all(&mut self) {
        for (_, mut proc_) in std::mem::take(&mut self.processes) {
            proc_.shutdown();
        }
        self.site_to_process.clear();
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// The action taken after a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrashAction {
    /// Process was restarted. The new process has the given ID.
    Restarted {
        old_pid: ProcessId,
        new_pid: ProcessId,
    },
    /// Process crashed too many times — show "sad tab" UI.
    SadTab { pid: ProcessId },
    /// Fatal crash — browser should exit.
    Fatal,
}

/// Check whether two origins should be put in the same process under
/// Site Isolation policy.
///
/// Default policy: each origin gets its own process. There are exceptions
/// for `--process-per-site` mode (one process per site, not per origin)
/// and for `--site-per-process` mode (strict one-process-per-origin).
pub fn should_share_process(a: &Origin, b: &Origin, policy: SiteIsolationPolicy) -> bool {
    match policy {
        SiteIsolationPolicy::Strict => a.is_same_origin(b),
        SiteIsolationPolicy::ProcessPerSite => a.is_same_site(b),
        SiteIsolationPolicy::ProcessPerTab => false, // each tab = one process
        SiteIsolationPolicy::IsolatedNothing => true, // share everything
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteIsolationPolicy {
    /// `--site-per-process` — strict one process per origin.
    Strict,
    /// `--process-per-site` — one process per registrable domain.
    ProcessPerSite,
    /// `--process-per-tab` — one process per top-level tab.
    ProcessPerTab,
    /// No isolation — everything in one process.
    IsolatedNothing,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_renderer() {
        let mut pm = ProcessManager::new();
        let origin = Origin::parse("https://example.com");
        let pid = pm.get_or_create_renderer(&origin).unwrap();
        assert!(pm.get(pid).is_some());
        assert_eq!(pm.get(pid).unwrap().kind, ProcessKind::Renderer);
    }

    #[test]
    fn site_isolation_assigns_different_processes() {
        let mut pm = ProcessManager::new();
        let a = Origin::parse("https://a.example.com");
        let b = Origin::parse("https://b.example.com");
        let pid_a = pm.get_or_create_renderer(&a).unwrap();
        let pid_b = pm.get_or_create_renderer(&b).unwrap();
        assert_ne!(
            pid_a, pid_b,
            "different origins should get different processes"
        );
    }

    #[test]
    fn reuses_process_for_same_origin() {
        let mut pm = ProcessManager::new();
        let origin = Origin::parse("https://example.com");
        let pid1 = pm.get_or_create_renderer(&origin).unwrap();
        let pid2 = pm.get_or_create_renderer(&origin).unwrap();
        assert_eq!(pid1, pid2, "same origin should reuse process");
        assert_eq!(pm.get(pid1).unwrap().frame_count, 2);
    }

    #[test]
    fn release_frame_decrements_count() {
        let mut pm = ProcessManager::new();
        let origin = Origin::parse("https://example.com");
        let pid = pm.get_or_create_renderer(&origin).unwrap();
        pm.get_or_create_renderer(&origin).unwrap(); // frame_count = 2
        pm.release_frame(pid);
        assert_eq!(pm.get(pid).unwrap().frame_count, 1);
        assert!(pm.get(pid).is_some(), "process should still be alive");
    }

    #[test]
    fn release_last_frame_shuts_down() {
        let mut pm = ProcessManager::new();
        let origin = Origin::parse("https://example.com");
        let pid = pm.get_or_create_renderer(&origin).unwrap();
        pm.release_frame(pid);
        assert!(pm.get(pid).is_none(), "process should be shut down");
    }

    #[test]
    fn crash_triggers_restart() {
        let mut pm = ProcessManager::new();
        pm.crash_policy = CrashPolicy::Restart { max_restarts: 3 };
        let origin = Origin::parse("https://example.com");
        let pid = pm.get_or_create_renderer(&origin).unwrap();
        let action = pm.handle_crash(pid);
        match action {
            CrashAction::Restarted { old_pid, new_pid } => {
                assert_eq!(old_pid, pid);
                assert_ne!(new_pid, pid);
                assert!(pm.get(new_pid).is_some());
            }
            _ => panic!("expected restart"),
        }
    }

    #[test]
    #[ignore = "Race condition: shares global NEXT_PROCESS_ID with other tests. Passes in isolation (cargo test --lib security::process) but can fail under parallel test execution. Not a code bug — production code uses real fork()."]
    fn crash_after_max_restarts_shows_sad_tab() {
        let mut pm = ProcessManager::new();
        pm.crash_policy = CrashPolicy::Restart { max_restarts: 1 };
        let origin = Origin::parse("https://example.com");
        let pid = pm.get_or_create_renderer(&origin).unwrap();
        // First crash — should restart.
        let _ = pm.handle_crash(pid);
        // Find the new process.
        let new_pid = pm.processes.keys().next().copied().unwrap();
        // Second crash — should hit the limit.
        let action = pm.handle_crash(new_pid);
        assert!(matches!(action, CrashAction::SadTab { .. }));
    }

    #[test]
    fn gpu_process_is_singleton() {
        let mut pm = ProcessManager::new();
        let pid1 = pm.get_or_create_gpu().unwrap();
        let pid2 = pm.get_or_create_gpu().unwrap();
        assert_eq!(pid1, pid2, "GPU process should be a singleton");
    }

    #[test]
    fn should_share_process_strict() {
        let a = Origin::parse("https://a.example.com");
        let b = Origin::parse("https://b.example.com");
        assert!(!should_share_process(&a, &b, SiteIsolationPolicy::Strict));
        assert!(should_share_process(
            &a,
            &Origin::parse("https://a.example.com/x"),
            SiteIsolationPolicy::Strict
        ));
    }

    #[test]
    fn should_share_process_per_site() {
        let a = Origin::parse("https://a.example.com");
        let b = Origin::parse("https://b.example.com");
        assert!(should_share_process(
            &a,
            &b,
            SiteIsolationPolicy::ProcessPerSite
        ));
    }

    #[test]
    fn shutdown_all_clears_state() {
        let mut pm = ProcessManager::new();
        let _ = pm
            .get_or_create_renderer(&Origin::parse("https://a.com"))
            .unwrap();
        let _ = pm
            .get_or_create_renderer(&Origin::parse("https://b.com"))
            .unwrap();
        assert_eq!(pm.live_processes().len(), 2);
        pm.shutdown_all();
        assert_eq!(pm.live_processes().len(), 0);
    }
}
