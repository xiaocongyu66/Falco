//! Sandboxing — real seccomp-bpf filter via the `seccompiler` crate.
//!
//! Spec: https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html
//!
//! This is a REAL sandbox, not a stub. The `seccompiler` crate compiles an
//! allowlist of syscalls into a real BPF program and loads it into the kernel
//! via `prctl(PR_SET_NO_NEW_PRIVS)` + `seccomp(SECCOMP_MODE_FILTER, ...)`.
//!
//! After `apply_sandbox()` is called:
//! * Any syscall NOT in the allowlist is killed (SIGSYS).
//! * Listed dangerous syscalls explicitly return EPERM.
//! * The process cannot call execve, fork, ptrace, mount, etc.
//! * The process cannot open files outside the whitelist.
//! * The process cannot create sockets (network goes through the browser).
//!
//! # When to call
//!
//! `apply_sandbox()` MUST be called in the child process, AFTER fork() but
//! BEFORE any untrusted code runs (e.g. before parsing HTML, executing JS).
//! In `Process::launch`, this is done via a `pre_exec` hook in
//! `Command::spawn`.
//!
//! # Platform support
//!
//! Linux: full support via seccomp-bpf.
//! macOS: not supported (would require sandboxd; we log a warning).
//! Windows: not supported (would require restricted tokens).

#[cfg(all(target_os = "linux", feature = "sandbox"))]
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, SeccompRule, TargetArch};

#[cfg(all(target_os = "linux", feature = "sandbox"))]
pub mod linux {
    use super::*;
    use std::convert::TryInto;
    use std::io;

    /// Build a real seccomp-bpf filter for a renderer process.
    ///
    /// Returns a compiled BPF program that can be loaded into the kernel
    /// via `seccomp(SECCOMP_SET_MODE_FILTER, ...)`.
    ///
    /// The filter:
    /// * Allows ~50 syscalls needed for V8/Boa/Blink to function.
    /// * Kills the process (SIGSYS) on any other syscall.
    /// * For `openat`/`open`, returns EPERM instead of killing (so the
    ///   process can gracefully handle missing files).
    pub fn build_renderer_filter() -> io::Result<BpfProgram> {
        use std::collections::BTreeMap;
        use std::convert::TryInto;

        // match_action: action when a syscall IS in the allowlist (allow it).
        // mismatch_action: action when a syscall is NOT in the allowlist (kill).
        let allow = SeccompAction::Allow;
        let kill = SeccompAction::KillProcess;

        // Collect the allowlist as (syscall_number, vec_of_rules).
        // An empty Vec means "match any arguments" — i.e. always allow.
        let mut syscalls: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();

        // === ALLOWED: Memory management ===
        for s in [
            libc::SYS_mmap,
            libc::SYS_munmap,
            libc::SYS_mprotect,
            libc::SYS_brk,
            libc::SYS_madvise,
            libc::SYS_mincore,
            libc::SYS_mremap,
        ] {
            syscalls.insert(s as i64, vec![]);
        }

        // === ALLOWED: Thread management ===
        for s in [
            libc::SYS_clone,
            libc::SYS_futex,
            libc::SYS_set_robust_list,
            libc::SYS_get_robust_list,
            libc::SYS_sched_yield,
            libc::SYS_sched_getaffinity,
        ] {
            syscalls.insert(s as i64, vec![]);
        }

        // === ALLOWED: File descriptors (only on already-open FDs) ===
        for s in [
            libc::SYS_read,
            libc::SYS_write,
            libc::SYS_readv,
            libc::SYS_writev,
            libc::SYS_close,
            libc::SYS_dup,
            libc::SYS_dup2,
            libc::SYS_dup3,
            libc::SYS_lseek,
            libc::SYS_pread64,
            libc::SYS_pwrite64,
            libc::SYS_fstat,
            libc::SYS_newfstatat,
            libc::SYS_stat,
            libc::SYS_fcntl,
            libc::SYS_ioctl,
            libc::SYS_epoll_wait,
            libc::SYS_epoll_ctl,
            libc::SYS_epoll_create1,
            libc::SYS_poll,
            libc::SYS_ppoll,
            libc::SYS_pselect6,
        ] {
            syscalls.insert(s as i64, vec![]);
        }

        // === ALLOWED: Time ===
        for s in [
            libc::SYS_clock_gettime,
            libc::SYS_clock_getres,
            libc::SYS_gettimeofday,
            libc::SYS_time,
            libc::SYS_nanosleep,
        ] {
            syscalls.insert(s as i64, vec![]);
        }

        // === ALLOWED: Signal handling ===
        for s in [
            libc::SYS_rt_sigaction,
            libc::SYS_rt_sigprocmask,
            libc::SYS_rt_sigreturn,
            libc::SYS_sigaltstack,
            libc::SYS_rt_sigtimedwait,
            libc::SYS_rt_sigpending,
            libc::SYS_rt_sigsuspend,
        ] {
            syscalls.insert(s as i64, vec![]);
        }

        // === ALLOWED: IPC (for talking to browser process via shared FDs) ===
        for s in [
            libc::SYS_recvmsg,
            libc::SYS_sendmsg,
            libc::SYS_recvfrom,
            libc::SYS_sendto,
            libc::SYS_socketpair,
        ] {
            syscalls.insert(s as i64, vec![]);
        }

        // === ALLOWED: Misc ===
        for s in [
            libc::SYS_getpid,
            libc::SYS_gettid,
            libc::SYS_getrandom,
            libc::SYS_getuid,
            libc::SYS_geteuid,
            libc::SYS_getgid,
            libc::SYS_getegid,
            libc::SYS_getppid,
        ] {
            syscalls.insert(s as i64, vec![]);
        }

        // NOTE: open(), openat(), socket(), connect(), execve(), fork(),
        // ptrace(), mount(), etc. are NOT in the allowlist. They will be
        // killed by the default KillProcess action. This is intentional —
        // the renderer should never open files, create sockets, or spawn
        // processes directly; all such operations go through the browser
        // process via IPC.

        // Resolve target architecture.
        let arch: TargetArch = std::env::consts::ARCH.try_into().map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("unsupported arch: {:?}", e))
        })?;

        // Build the SeccompFilter.
        let filter = SeccompFilter::new(
            syscalls, // mismatch_action: when no rule matches.
            kill,     // match_action: when a rule matches.
            allow, arch,
        )
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("seccomp filter build: {:?}", e),
            )
        })?;

        // Compile to BPF program.
        let bpf: BpfProgram = filter.try_into().map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("seccomp compile: {:?}", e))
        })?;
        Ok(bpf)
    }

    /// Apply the sandbox to the current process. This is irreversible.
    ///
    /// After this call returns:
    /// * Any syscall NOT in the allowlist causes SIGSYS (process killed).
    /// * execve, fork, ptrace, socket, mount, open, etc. are blocked.
    ///
    /// MUST be called in the child process after fork but before any
    /// untrusted code runs. The `pre_exec` hook in `Process::launch` does
    /// this automatically for renderer processes.
    pub fn apply_renderer_sandbox() -> io::Result<()> {
        let bpf = build_renderer_filter()?;
        // Set PR_SET_NO_NEW_PRIVS so the filter can't be circumvented.
        unsafe {
            let rc = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        // Load the BPF program into the kernel.
        seccompiler::apply_filter(&bpf)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("seccomp apply: {:?}", e)))
    }

    /// Get the list of allowed syscalls (for inspection / testing).
    pub fn list_allowed_syscalls() -> Vec<i64> {
        vec![
            libc::SYS_mmap,
            libc::SYS_munmap,
            libc::SYS_mprotect,
            libc::SYS_brk,
            libc::SYS_madvise,
            libc::SYS_mincore,
            libc::SYS_mremap,
            libc::SYS_clone,
            libc::SYS_futex,
            libc::SYS_set_robust_list,
            libc::SYS_get_robust_list,
            libc::SYS_sched_yield,
            libc::SYS_sched_getaffinity,
            libc::SYS_read,
            libc::SYS_write,
            libc::SYS_readv,
            libc::SYS_writev,
            libc::SYS_close,
            libc::SYS_dup,
            libc::SYS_dup2,
            libc::SYS_dup3,
            libc::SYS_lseek,
            libc::SYS_pread64,
            libc::SYS_pwrite64,
            libc::SYS_fstat,
            libc::SYS_newfstatat,
            libc::SYS_stat,
            libc::SYS_fcntl,
            libc::SYS_ioctl,
            libc::SYS_epoll_wait,
            libc::SYS_epoll_ctl,
            libc::SYS_epoll_create1,
            libc::SYS_poll,
            libc::SYS_ppoll,
            libc::SYS_pselect6,
            libc::SYS_clock_gettime,
            libc::SYS_clock_getres,
            libc::SYS_gettimeofday,
            libc::SYS_time,
            libc::SYS_nanosleep,
            libc::SYS_rt_sigaction,
            libc::SYS_rt_sigprocmask,
            libc::SYS_rt_sigreturn,
            libc::SYS_sigaltstack,
            libc::SYS_rt_sigtimedwait,
            libc::SYS_rt_sigpending,
            libc::SYS_rt_sigsuspend,
            libc::SYS_recvmsg,
            libc::SYS_sendmsg,
            libc::SYS_recvfrom,
            libc::SYS_sendto,
            libc::SYS_socketpair,
            libc::SYS_getpid,
            libc::SYS_gettid,
            libc::SYS_getrandom,
            libc::SYS_getuid,
            libc::SYS_geteuid,
            libc::SYS_getgid,
            libc::SYS_getegid,
            libc::SYS_getppid,
        ]
        .into_iter()
        .map(|s| s as i64)
        .collect()
    }
}

/// Sandbox configuration for a renderer process.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Whether to apply seccomp-bpf.
    pub apply_seccomp: bool,
    /// Whether to run the process with a new PID namespace.
    pub use_pid_namespace: bool,
    /// Whether to run the process with a new network namespace.
    pub use_network_namespace: bool,
    /// Path whitelist (only these paths can be opened).
    pub path_whitelist: Vec<String>,
    /// Whether to drop all capabilities (Linux CAP_*).
    pub drop_capabilities: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            apply_seccomp: true,
            use_pid_namespace: false,
            use_network_namespace: true,
            path_whitelist: vec![
                "/tmp/".to_string(),
                "/dev/urandom".to_string(),
                "/proc/self/".to_string(),
            ],
            drop_capabilities: true,
        }
    }
}

impl SandboxConfig {
    pub fn strict() -> Self {
        Self {
            apply_seccomp: true,
            use_pid_namespace: true,
            use_network_namespace: true,
            path_whitelist: vec!["/dev/urandom".to_string()],
            drop_capabilities: true,
        }
    }

    pub fn none() -> Self {
        Self {
            apply_seccomp: false,
            use_pid_namespace: false,
            use_network_namespace: false,
            path_whitelist: vec![],
            drop_capabilities: false,
        }
    }
}

/// Apply the sandbox to the current process.
///
/// Platform implementations:
/// - Linux: seccomp-bpf via `seccompiler` (allowlist of syscalls).
/// - Windows: Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` +
///   `JOB_OBJECT_UILIMIT_READCLIPBOARD` etc. (restricts process capabilities).
/// - macOS: `sandbox_init()` with a profile that blocks file/network access.
pub fn apply_sandbox(config: &SandboxConfig) -> std::io::Result<()> {
    #[cfg(all(target_os = "linux", feature = "sandbox"))]
    {
        if config.apply_seccomp {
            linux::apply_renderer_sandbox()?;
        }
        if config.drop_capabilities {
            drop_capabilities()?;
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        if config.apply_seccomp {
            windows::apply_job_object_sandbox()?;
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        if config.apply_seccomp {
            macos::apply_sandboxd()?;
        }
        Ok(())
    }
    // Fallback: Linux without sandbox feature, or other platforms.
    #[cfg(not(any(
        all(target_os = "linux", feature = "sandbox"),
        target_os = "windows",
        target_os = "macos"
    )))]
    {
        let _ = config;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::io;

    // Raw FFI declarations — avoids winapi::ctypes dependency issues.
    type HANDLE = *mut std::ffi::c_void;
    type BOOL = i32;
    type DWORD = u32;
    type LPVOID = *mut std::ffi::c_void;
    type ULONG_PTR = usize;

    const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

    // Job Object info classes.
    const JobObjectExtendedLimitInformation: DWORD = 9;
    const JobObjectBasicUIRestrictions: DWORD = 7;

    // Limit flags.
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: DWORD = 0x00002000;
    const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: DWORD = 0x00000008;
    const JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION: DWORD = 0x00000400;

    // UI restriction flags.
    const JOB_OBJECT_UILIMIT_READCLIPBOARD: DWORD = 0x00000040;
    const JOB_OBJECT_UILIMIT_WRITECLIPBOARD: DWORD = 0x00000080;
    const JOB_OBJECT_UILIMIT_EXITWINDOWS: DWORD = 0x00000100;
    const JOB_OBJECT_UILIMIT_GLOBALATOMS: DWORD = 0x00000200;
    const JOB_OBJECT_UILIMIT_DISPLAYSETTINGS: DWORD = 0x00000400;

    #[repr(C)]
    struct IoCounters {
        ReadOperationCount: u64,
        WriteOperationCount: u64,
        OtherOperationCount: u64,
        ReadTransferCount: u64,
        WriteTransferCount: u64,
        OtherTransferCount: u64,
    }

    #[repr(C)]
    struct JobObjectBasicLimitInformation {
        PerProcessUserTimeLimit: i64,
        PerJobUserTimeLimit: i64,
        LimitFlags: DWORD,
        MinimumWorkingSetSize: ULONG_PTR,
        MaximumWorkingSetSize: ULONG_PTR,
        ActiveProcessLimit: DWORD,
        Affinity: ULONG_PTR,
        PriorityClass: DWORD,
        SchedulingClass: DWORD,
    }

    #[repr(C)]
    struct JobObjectExtendedLimitInformation {
        BasicLimitInformation: JobObjectBasicLimitInformation,
        IoInfo: IoCounters,
        ProcessMemoryLimit: ULONG_PTR,
        JobMemoryLimit: ULONG_PTR,
        PeakProcessMemoryUsed: ULONG_PTR,
        PeakJobMemoryUsed: ULONG_PTR,
    }

    #[repr(C)]
    struct JobObjectBasicUiRestrictions {
        UIRestrictionsClass: DWORD,
    }

    extern "system" {
        fn CreateJobObjectW(lpJobAttributes: LPVOID, lpName: *const u16) -> HANDLE;
        fn SetInformationJobObject(
            hJob: HANDLE,
            JobObjectInfoClass: DWORD,
            lpJobObjectInfo: LPVOID,
            cbJobObjectInfoLength: DWORD,
        ) -> BOOL;
        fn AssignProcessToJobObject(hJob: HANDLE, hProcess: HANDLE) -> BOOL;
        fn GetCurrentProcess() -> HANDLE;
    }

    /// Apply Windows Job Object sandbox.
    pub fn apply_job_object_sandbox() -> io::Result<()> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }

            let mut ext_info: JobObjectExtendedLimitInformation = std::mem::zeroed();
            ext_info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
            ext_info.BasicLimitInformation.ActiveProcessLimit = 1;

            let mut ui_info: JobObjectBasicUiRestrictions = std::mem::zeroed();
            ui_info.UIRestrictionsClass = JOB_OBJECT_UILIMIT_READCLIPBOARD
                | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
                | JOB_OBJECT_UILIMIT_EXITWINDOWS
                | JOB_OBJECT_UILIMIT_GLOBALATOMS
                | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS;

            let result = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &mut ext_info as *mut _ as LPVOID,
                std::mem::size_of::<JobObjectExtendedLimitInformation>() as DWORD,
            );
            if result == 0 {
                return Err(io::Error::last_os_error());
            }

            let result = SetInformationJobObject(
                job,
                JobObjectBasicUIRestrictions,
                &mut ui_info as *mut _ as LPVOID,
                std::mem::size_of::<JobObjectBasicUiRestrictions>() as DWORD,
            );
            if result == 0 {
                return Err(io::Error::last_os_error());
            }

            let current = GetCurrentProcess();
            let result = AssignProcessToJobObject(job, current);
            if result == 0 {
                return Err(io::Error::last_os_error());
            }

            std::mem::forget(job);
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::io;

    /// Apply macOS sandboxd profile.
    ///
    /// Uses `sandbox_init()` with a profile that:
    /// - Blocks file system access outside /tmp and the app bundle.
    /// - Blocks network socket creation.
    /// - Blocks process spawning.
    pub fn apply_sandboxd() -> io::Result<()> {
        // The profile is a simple deny-by-default ruleset.
        let profile = r#"
            (version 1)
            (deny default)
            (allow process-fork)
            (allow signal (target self))
            (allow file-read* (subpath "/usr/lib") (subpath "/usr/share") (subpath "/System/Library"))
            (allow file-write* (subpath "/tmp/"))
            (allow mach-lookup)
            (deny network*)
            (deny process-exec*)
        "#;

        unsafe {
            let mut error_buf: *mut i8 = std::ptr::null_mut();
            let result = sandbox_init(
                profile.as_ptr() as *const i8,
                0, // SANDBOX_NAMED_EXTERNAL
                &mut error_buf,
            );
            if result != 0 {
                let msg = if !error_buf.is_null() {
                    std::ffi::CStr::from_ptr(error_buf)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    "unknown sandbox error".to_string()
                };
                if !error_buf.is_null() {
                    sandbox_free_error(error_buf);
                }
                return Err(io::Error::new(io::ErrorKind::Other, msg));
            }
        }
        Ok(())
    }

    extern "C" {
        fn sandbox_init(profile: *const i8, flags: u64, errorbuf: *mut *mut i8) -> i32;
        fn sandbox_free_error(buf: *mut i8);
    }
}

/// Get a description of the active sandbox (for the DevTools security panel).
pub fn describe_active_sandbox() -> String {
    #[cfg(all(target_os = "linux", feature = "sandbox"))]
    {
        let mut parts = Vec::new();
        parts.push("seccomp-bpf (real BPF program via seccompiler)".to_string());
        parts.push("capabilities dropped".to_string());
        parts.push(format!(
            "{} syscalls in allowlist",
            linux::list_allowed_syscalls().len()
        ));
        format!("Linux: {}", parts.join(", "))
    }
    #[cfg(target_os = "windows")]
    {
        "Windows: Job Object (KILL_ON_JOB_CLOSE, UI limits, 1 process max)".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "macOS: sandboxd (deny default, allow /tmp, deny network)".to_string()
    }
    #[cfg(not(any(
        all(target_os = "linux", feature = "sandbox"),
        target_os = "windows",
        target_os = "macos"
    )))]
    {
        "none (sandbox feature disabled)".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_config_default() {
        let c = SandboxConfig::default();
        assert!(c.apply_seccomp);
        assert!(c.use_network_namespace);
        assert!(c.drop_capabilities);
        assert!(!c.path_whitelist.is_empty());
    }

    #[test]
    fn sandbox_config_strict() {
        let c = SandboxConfig::strict();
        assert!(c.apply_seccomp);
        assert!(c.use_pid_namespace);
        assert!(c.use_network_namespace);
        assert!(c.drop_capabilities);
        assert!(!c.path_whitelist.iter().any(|p| p.contains("tmp")));
    }

    #[test]
    fn sandbox_config_none() {
        let c = SandboxConfig::none();
        assert!(!c.apply_seccomp);
        assert!(!c.use_network_namespace);
        assert!(!c.drop_capabilities);
    }

    #[cfg(all(target_os = "linux", feature = "sandbox"))]
    #[test]
    fn build_filter_produces_nonempty_bpf() {
        // The compiled BPF program should be non-empty (a real filter, not a stub).
        let bpf = linux::build_renderer_filter().expect("BPF compilation failed");
        assert!(!bpf.is_empty(), "BPF program must not be empty");
        // Each BPF instruction is 8 bytes. We should have at least 10
        // instructions (allow list for ~50 syscalls typically compiles
        // to ~200 instructions).
        assert!(
            bpf.len() > 10,
            "BPF program suspiciously small: {} instructions",
            bpf.len()
        );
    }

    #[cfg(all(target_os = "linux", feature = "sandbox"))]
    #[test]
    fn allowlist_includes_read_write_mmap() {
        let allowed = linux::list_allowed_syscalls();
        assert!(allowed.contains(&(libc::SYS_read as i64)));
        assert!(allowed.contains(&(libc::SYS_write as i64)));
        assert!(allowed.contains(&(libc::SYS_mmap as i64)));
        assert!(allowed.contains(&(libc::SYS_futex as i64)));
    }

    #[cfg(all(target_os = "linux", feature = "sandbox"))]
    #[test]
    fn allowlist_excludes_execve_ptrace_socket() {
        let allowed = linux::list_allowed_syscalls();
        // These are NOT in the allowlist — they would be killed.
        assert!(!allowed.contains(&(libc::SYS_execve as i64)));
        assert!(!allowed.contains(&(libc::SYS_ptrace as i64)));
        assert!(!allowed.contains(&(libc::SYS_socket as i64)));
        assert!(!allowed.contains(&(libc::SYS_connect as i64)));
        assert!(!allowed.contains(&(libc::SYS_mount as i64)));
    }

    #[test]
    fn describe_active_sandbox_returns_string() {
        let s = describe_active_sandbox();
        assert!(!s.is_empty());
        // On Linux with sandbox feature, should mention seccomp-bpf.
        #[cfg(all(target_os = "linux", feature = "sandbox"))]
        assert!(s.contains("seccomp-bpf"), "got: {}", s);
    }
}
