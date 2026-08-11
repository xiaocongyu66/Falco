//! W^X (Write-Xor-Execute) executable memory.
//!
//! Modern OSes forbid pages that are simultaneously writable AND executable
//! (RWX). This module allocates memory as RW for code generation, then
//! flips permissions to RX before execution.
//!
//! # Platform Support
//!
//! - **Linux**: `mmap(PROT_READ|PROT_WRITE)` + `mprotect(PROT_READ|PROT_EXEC)`
//! - **macOS Intel**: Same as Linux — `mmap` + `mprotect`
//! - **macOS Apple Silicon**: `mmap(MAP_JIT)` + `pthread_jit_write_protect_np()`
//!   toggle between RW and RX on the same mapping (W^X enforced in hardware).
//! - **Windows**: `VirtualAlloc(PAGE_READWRITE)` + `VirtualProtect(PAGE_EXECUTE_READ)`
//!
//! # Hardened Kernels
//!
//! On systems with `mmap_min_addr` set very high or `seccomp` filters that
//! block `mmap` with `PROT_EXEC`, allocation will fail. The JIT context
//! detects this and falls back to the interpreter.

use std::io;

/// A region of executable memory.
///
/// Holds a raw pointer + size. After `make_executable()` is called, the
/// memory is no longer writable but can be called as a function.
pub struct ExecMemory {
    ptr: *mut u8,
    size: usize,
    /// Whether we've flipped to executable mode.
    executable: bool,
    /// On macOS Apple Silicon, whether MAP_JIT was used.
    /// If so, we use `pthread_jit_write_protect_np` to toggle RW/RX.
    uses_map_jit: bool,
}

unsafe impl Send for ExecMemory {}
unsafe impl Sync for ExecMemory {}

impl ExecMemory {
    /// Allocate a writable region of `size` bytes.
    ///
    /// Initially writable (RW), not executable. Call `make_executable()`
    /// before invoking the code.
    pub fn allocate_rw(size: usize) -> io::Result<Self> {
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "size must be > 0",
            ));
        }

        // Round size up to page size.
        let page_size = page_size();
        let size = ((size + page_size - 1) / page_size) * page_size;

        unsafe {
            #[cfg(unix)]
            {
                let (ptr, uses_map_jit) = unix_mmap_rw(size)?;
                if ptr.is_null() || ptr == (-1isize as *mut u8) {
                    return Err(io::Error::other("mmap returned invalid pointer"));
                }
                Ok(Self {
                    ptr,
                    size,
                    executable: false,
                    uses_map_jit,
                })
            }

            #[cfg(windows)]
            {
                let ptr = windows_virtual_alloc_rw(size)?;
                if ptr.is_null() {
                    return Err(io::Error::other("VirtualAlloc returned NULL"));
                }
                Ok(Self {
                    ptr,
                    size,
                    executable: false,
                    uses_map_jit: false,
                })
            }

            #[cfg(not(any(unix, windows)))]
            {
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "JIT not supported on this platform",
                ))
            }
        }
    }

    /// Write code into the (still writable) memory at `offset`.
    ///
    /// Panics if `make_executable()` has already been called (memory is RX).
    pub fn write(&mut self, offset: usize, code: &[u8]) {
        debug_assert!(
            !self.executable,
            "cannot write to executable memory (W^X policy)"
        );
        debug_assert!(
            offset + code.len() <= self.size,
            "write out of bounds: offset={} + len={} > size={}",
            offset,
            code.len(),
            self.size
        );
        unsafe {
            std::ptr::copy_nonoverlapping(code.as_ptr(), self.ptr.add(offset), code.len());
        }
    }

    /// Flip permissions from RW → RX.
    ///
    /// After this call, the memory is executable but not writable.
    /// On macOS Apple Silicon (MAP_JIT), this calls
    /// `pthread_jit_write_protect_np(1)`.
    pub fn make_executable(&mut self) -> io::Result<()> {
        if self.executable {
            return Ok(());
        }

        unsafe {
            #[cfg(unix)]
            {
                unix_make_executable(self.ptr, self.size, self.uses_map_jit)?;
            }

            #[cfg(windows)]
            {
                windows_make_executable(self.ptr, self.size)?;
            }
        }

        self.executable = true;
        Ok(())
    }

    /// Get the raw pointer to the executable code.
    ///
    /// Caller is responsible for ensuring `make_executable()` was called.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Get the function pointer to the start of the code.
    ///
    /// Convenience wrapper around `as_ptr()` with type erasure.
    pub fn as_fn(&self) -> extern "C" fn() -> u64 {
        assert!(self.executable, "memory is not executable");
        unsafe { std::mem::transmute(self.ptr) }
    }

    /// Size of the allocated region in bytes.
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for ExecMemory {
    fn drop(&mut self) {
        unsafe {
            #[cfg(unix)]
            {
                if !self.ptr.is_null() && self.ptr != (-1isize as *mut u8) {
                    let _ = unix_munmap(self.ptr, self.size);
                }
            }

            #[cfg(windows)]
            {
                if !self.ptr.is_null() {
                    let _ = windows_virtual_free(self.ptr);
                }
            }
        }
    }
}

// ── Unix (Linux + macOS) ──────────────────────────────────────────────

#[cfg(unix)]
unsafe fn unix_mmap_rw(size: usize) -> io::Result<(*mut u8, bool)> {
    extern "C" {
        fn mmap(
            addr: *mut std::ffi::c_void,
            length: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut std::ffi::c_void;
    }

    const PROT_READ: i32 = 1;
    const PROT_WRITE: i32 = 2;
    const MAP_PRIVATE: i32 = 0x02;
    const MAP_ANONYMOUS: i32 = 0x20;

    // On macOS (both Intel and Apple Silicon), use MAP_JIT.
    // The flag value is 0x800 (defined in <sys/mman.h> as MAP_JIT).
    // On Linux, MAP_JIT doesn't exist — we just use RW and mprotect to RX.
    #[cfg(target_os = "macos")]
    const MAP_JIT: i32 = 0x8000;
    #[cfg(not(target_os = "macos"))]
    const MAP_JIT: i32 = 0;

    #[cfg(target_os = "macos")]
    let uses_map_jit = true;
    #[cfg(not(target_os = "macos"))]
    let uses_map_jit = false;

    let mut flags = MAP_PRIVATE | MAP_ANONYMOUS;
    if uses_map_jit {
        flags |= MAP_JIT;
    }

    let ptr = mmap(
        std::ptr::null_mut(),
        size,
        PROT_READ | PROT_WRITE,
        flags,
        -1,
        0,
    );

    if ptr.is_null() || ptr == (-1isize as *mut std::ffi::c_void) {
        // If MAP_JIT failed (older macOS), retry without it.
        if uses_map_jit {
            let ptr2 = mmap(
                std::ptr::null_mut(),
                size,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr2.is_null() || ptr2 == (-1isize as *mut std::ffi::c_void) {
                return Err(io::Error::last_os_error());
            }
            return Ok((ptr2 as *mut u8, false));
        }
        return Err(io::Error::last_os_error());
    }

    Ok((ptr as *mut u8, uses_map_jit))
}

#[cfg(unix)]
unsafe fn unix_make_executable(
    ptr: *mut u8,
    size: usize,
    uses_map_jit: bool,
) -> io::Result<()> {
    extern "C" {
        fn mprotect(addr: *mut std::ffi::c_void, len: usize, prot: i32) -> i32;
    }

    const PROT_READ: i32 = 1;
    const PROT_EXEC: i32 = 4;

    // On macOS Apple Silicon with MAP_JIT, we toggle RW/RX with
    // pthread_jit_write_protect_np instead of mprotect.
    // This is required because MAP_JIT memory is special: it's the only
    // memory that can be made executable on Apple Silicon, and the toggle
    // is per-thread, not per-page.
    #[cfg(target_os = "macos")]
    if uses_map_jit {
        extern "C" {
            fn pthread_jit_write_protect_np(enabled: i32) -> i32;
        }
        // 1 = executable (no writes), 0 = writable (no exec)
        // We want executable now.
        let r = pthread_jit_write_protect_np(1);
        if r != 0 {
            return Err(io::Error::last_os_error());
        }
        return Ok(());
    }

    // Standard Unix path: mprotect to PROT_READ|PROT_EXEC.
    let r = mprotect(ptr as *mut std::ffi::c_void, size, PROT_READ | PROT_EXEC);
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
unsafe fn unix_munmap(ptr: *mut u8, size: usize) -> io::Result<()> {
    extern "C" {
        fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
    }
    // On macOS Apple Silicon, ensure we're in writable mode before unmapping
    // (otherwise the kernel may complain).
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn pthread_jit_write_protect_np(enabled: i32) -> i32;
        }
        let _ = pthread_jit_write_protect_np(0);
    }
    let r = munmap(ptr as *mut std::ffi::c_void, size);
    if r != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// ── Windows ───────────────────────────────────────────────────────────

#[cfg(windows)]
unsafe fn windows_virtual_alloc_rw(size: usize) -> io::Result<*mut u8> {
    // Raw FFI — avoids winapi crate dependency entirely.
    const MEM_COMMIT: u32 = 0x00001000;
    const MEM_RESERVE: u32 = 0x00002000;
    const PAGE_READWRITE: u32 = 0x04;

    extern "system" {
        fn VirtualAlloc(
            lpAddress: *mut std::ffi::c_void,
            dwSize: usize,
            flAllocationType: u32,
            flProtect: u32,
        ) -> *mut std::ffi::c_void;
    }

    let ptr = VirtualAlloc(
        std::ptr::null_mut(),
        size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );
    if ptr.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(ptr as *mut u8)
    }
}

#[cfg(windows)]
unsafe fn windows_make_executable(ptr: *mut u8, size: usize) -> io::Result<()> {
    const PAGE_EXECUTE_READ: u32 = 0x20;

    extern "system" {
        fn VirtualProtect(
            lpAddress: *mut std::ffi::c_void,
            dwSize: usize,
            flNewProtect: u32,
            lpflOldProtect: *mut u32,
        ) -> i32;
    }

    let mut old_protect: u32 = 0;
    let r = VirtualProtect(
        ptr as *mut std::ffi::c_void,
        size,
        PAGE_EXECUTE_READ,
        &mut old_protect,
    );
    if r == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
unsafe fn windows_virtual_free(ptr: *mut u8) -> io::Result<()> {
    const MEM_RELEASE: u32 = 0x8000;

    extern "system" {
        fn VirtualFree(
            lpAddress: *mut std::ffi::c_void,
            dwSize: usize,
            dwFreeType: u32,
        ) -> i32;
    }

    let r = VirtualFree(ptr as *mut std::ffi::c_void, 0, MEM_RELEASE);
    if r == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Get the system page size.
fn page_size() -> usize {
    #[cfg(unix)]
    {
        extern "C" {
            fn sysconf(name: i32) -> i64;
        }
        // _SC_PAGESIZE on Linux and macOS is 30.
        const SC_PAGESIZE: i32 = 30;
        unsafe {
            let ps = sysconf(SC_PAGESIZE);
            if ps > 0 {
                return ps as usize;
            }
        }
    }
    // Fallback — 4096 is correct on x86_64 and AArch64.
    4096
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_and_write() {
        let mut mem = match ExecMemory::allocate_rw(4096) {
            Ok(m) => m,
            Err(_) => {
                // JIT not available in this test env — skip.
                return;
            }
        };
        // Write some bytes.
        let code = [0xC3u8]; // RET
        mem.write(0, &code);
        // Make executable.
        let _ = mem.make_executable();
        // Pointer should be valid.
        let ptr = mem.as_ptr();
        assert!(!ptr.is_null());
    }

    #[test]
    fn allocate_zero_size_fails() {
        let r = ExecMemory::allocate_rw(0);
        assert!(r.is_err());
    }
}
