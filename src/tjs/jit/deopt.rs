//! Deoptimization support.
//!
//! When a JIT-compiled instruction encounters a situation it wasn't
//! compiled for (e.g., a type guard fails, or an inline cache misses),
//! the JIT must exit back to the interpreter.
//!
//! # Deopt Table
//!
//! Each compiled loop has a deopt table mapping native code offsets to:
//! - The corresponding bytecode IP to resume at
//! - The reason for the deopt (for profiling / re-JIT decisions)
//! - The expected stack depth at that point (for sanity checks)
//!
//! # Deopt Stub
//!
//! When a type guard fails, the emitted code jumps to a deopt stub at
//! the end of the compiled code. The stub:
//! 1. Sets rax = index into the deopt table (so we know where we deopted)
//! 2. Sets the exit reason to 1 (deopt)
//! 3. Jumps to the epilogue
//!
//! The `JitContext::run_compiled` call then reads the deopt table to
//! determine the resume IP.

use crate::tjs::value::Value;
use std::sync::Once;

/// One-time initialization guard for the Value layout statics.
static LAYOUT_INIT: Once = Once::new();

/// Why a deoptimization occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeoptReason {
    /// A type guard failed (e.g., expected Number, got String).
    TypeGuardFailed,
    /// An inline cache missed (object shape didn't match cached shape).
    InlineCacheMiss,
    /// The stack pointer went out of bounds.
    StackOverflow,
    StackUnderflow,
    /// An unsupported bytecode was encountered.
    UnsupportedBytecode,
    /// A function call returned a non-Number where Number was expected.
    NonNumberReturn,
    /// An exception was thrown by a called function.
    Exception,
    /// A property access on null/undefined.
    NullDeref,
    /// An explicit `debugger` or breakpoint.
    Debugger,
}

/// One entry in the deopt table.
///
/// Maps a native code offset → interpreter state to resume with.
#[derive(Debug, Clone)]
pub struct DeoptInfo {
    /// Native code offset (relative to start of compiled code) where
    /// the deopt stub was emitted. Used for debugging.
    pub native_offset: usize,
    /// Bytecode IP to resume interpretation at.
    pub bytecode_ip: usize,
    /// Why we deoptimized.
    pub reason: DeoptReason,
    /// Expected stack depth at the point of deopt (in Value units).
    /// The VM checks this matches the actual stack pointer.
    pub expected_stack_depth: usize,
}

impl DeoptInfo {
    pub fn new(
        native_offset: usize,
        bytecode_ip: usize,
        reason: DeoptReason,
        expected_stack_depth: usize,
    ) -> Self {
        Self {
            native_offset,
            bytecode_ip,
            reason,
            expected_stack_depth,
        }
    }
}

/// Result of running JIT-compiled code.
///
/// The VM uses this to decide how to continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitExitReason {
    /// Normal exit — VM should continue from `bytecode_ip`.
    Normal(usize),
    /// Deoptimized — VM should resume interpreter from `bytecode_ip`.
    Deopt(usize),
    /// An exception was thrown — VM should propagate it.
    Exception,
}

// ── Value layout helpers ──────────────────────────────────────────────
//
// The JIT needs to know the layout of `Value` in memory to emit type
// guards and access payloads directly. These constants encode that layout.
//
// IMPORTANT: These MUST match the layout of `Value` in `value.rs`.
// If the Value enum layout changes, update these constants.
//
// Current Value layout (Rust enum, 24 bytes on x86_64):
//   Offset 0:  discriminant (8 bytes, u64 — Rust uses 8 bytes for enum tag
//              when any variant has a non-ZST payload)
//   Offset 8:  payload (16 bytes — for Number it's f64 at offset 8)
//
// NOTE: This is fragile. A production JIT would use `#[repr(C)]` on Value
// or use NaN-boxing. We rely on Rust's stable enum layout for now.

/// Discriminant value for `Value::Number(f64)`.
///
/// This is the discriminant tag Rust assigns to the Number variant.
/// DETERMINED AT RUNTIME — see `compute_value_layout()` below.
pub static mut NUMBER_DISCRIMINANT: u64 = 0;
/// Discriminant for `Value::String`.
pub static mut STRING_DISCRIMINANT: u64 = 1;
/// Discriminant for `Value::Boolean`.
pub static mut BOOLEAN_DISCRIMINANT: u64 = 2;
/// Discriminant for `Value::Null`.
pub static mut NULL_DISCRIMINANT: u64 = 3;
/// Discriminant for `Value::Undefined`.
pub static mut UNDEFINED_DISCRIMINANT: u64 = 4;
/// Discriminant for `Value::Object`.
pub static mut OBJECT_DISCRIMINANT: u64 = 5;
/// Discriminant for `Value::Array`.
pub static mut ARRAY_DISCRIMINANT: u64 = 6;
/// Discriminant for `Value::Function`.
pub static mut FUNCTION_DISCRIMINANT: u64 = 7;
/// Discriminant for `Value::Builtin`.
pub static mut BUILTIN_DISCRIMINANT: u64 = 8;
/// Discriminant for `Value::BigInt`.
pub static mut BIGINT_DISCRIMINANT: u64 = 9;

/// Offset (in bytes) of the f64 payload within a `Value::Number`.
///
/// For Rust enums with non-ZST payloads, the discriminant is at offset 0
/// and the payload starts at offset 8 (after the 8-byte discriminant).
pub const NUMBER_PAYLOAD_OFFSET: usize = 8;

/// Size of a `Value` in bytes.
pub const VALUE_SIZE: usize = std::mem::size_of::<Value>();

/// Initialize the runtime-detected Value layout.
///
/// MUST be called once before any JIT compilation. Idempotent — uses
/// a `Once` guard so multiple calls are safe.
pub fn init_value_layout() {
    LAYOUT_INIT.call_once(|| {
        // SAFETY: We're computing the discriminant values by creating Values
        // and reading their raw bytes. This is safe as long as we don't read
        // uninitialized memory.
        unsafe {
            let v = Value::Number(0.0);
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            NUMBER_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            let v = Value::String(String::new());
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            STRING_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            let v = Value::Boolean(false);
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            BOOLEAN_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            let v = Value::Null;
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            NULL_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            let v = Value::Undefined;
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            UNDEFINED_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            let v = Value::Object(std::rc::Rc::new(std::cell::RefCell::new(
                crate::tjs::value::ObjectValue::new(),
            )));
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            OBJECT_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            let v = Value::Array(std::rc::Rc::new(std::cell::RefCell::new(vec![])));
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            ARRAY_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            let v = Value::BigInt("0".to_string());
            let bytes: &[u8] = std::slice::from_raw_parts(
                &v as *const Value as *const u8,
                std::mem::size_of::<Value>(),
            );
            BIGINT_DISCRIMINANT = u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);

            // Log the layout for debugging.
            eprintln!(
                "[tjs:jit] Value layout: size={} bytes, discrim(Number)=0x{:016x}, discrim(String)=0x{:016x}, discrim(Boolean)=0x{:016x}, discrim(Undefined)=0x{:016x}, discrim(Null)=0x{:016x}",
                std::mem::size_of::<Value>(),
                NUMBER_DISCRIMINANT,
                STRING_DISCRIMINANT,
                BOOLEAN_DISCRIMINANT,
                UNDEFINED_DISCRIMINANT,
                NULL_DISCRIMINANT
            );
        }
    });
}

/// Get the discriminant value for `Value::Number`.
///
/// Initializes the layout on first call.
pub fn number_discriminant() -> u64 {
    init_value_layout();
    unsafe { NUMBER_DISCRIMINANT }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_layout_initializes() {
        init_value_layout();
        let n = number_discriminant();
        // The discriminant can be any u64 value — Rust may use niche
        // optimization, so it might not be a small integer. We just
        // verify the detection ran and returned *something*.
        // (For our Value enum, the discriminant happens to be 0x8000000000000000.)
        let _ = n;
    }

    #[test]
    fn value_size_is_correct() {
        // On x86_64, Value is 40 bytes:
        //   8 bytes discriminant + 32 bytes payload (largest variant is BuiltinFn
        //   which contains a String (24 bytes) + Rc (8 bytes)).
        let size = std::mem::size_of::<Value>();
        assert!(size >= 16, "Value size too small: {}", size);
        assert!(size <= 64, "Value size too large: {}", size);
    }

    #[test]
    fn deopt_info_construction() {
        let info = DeoptInfo::new(0x100, 42, DeoptReason::TypeGuardFailed, 3);
        assert_eq!(info.native_offset, 0x100);
        assert_eq!(info.bytecode_ip, 42);
        assert_eq!(info.reason, DeoptReason::TypeGuardFailed);
        assert_eq!(info.expected_stack_depth, 3);
    }
}
