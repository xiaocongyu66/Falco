//! WebAssembly runtime — full from-scratch implementation.
//!
//! # Overview
//!
//! This module implements the WebAssembly (WASM) MVP specification:
//!
//! - Binary format parser (decoder for `.wasm` files)
//! - Validator (basic structural / type checking)
//! - Stack-based interpreter (executes WASM instructions)
//! - Linear memory (grow/load/store for i32/i64/f32/f64)
//! - Function tables (call_indirect)
//! - Globals (mutable and immutable)
//! - All control-flow constructs (block/loop/if/br/br_table/return/call)
//! - JS API: `WebAssembly.Module`, `WebAssembly.Instance`,
//!   `WebAssembly.Memory`, `WebAssembly.Table`, `WebAssembly.instantiate`,
//!   `WebAssembly.compile`, `WebAssembly.validate`
//!
//! # Architecture
//!
//! ```text
//! .wasm bytes
//!     │
//!     ▼
//! ┌───────────────────┐
//! │  parser.rs        │  decode LEB128, sections, function bodies
//! └─────────┬─────────┘
//!           │ Module { types, funcs, memories, tables, globals, exports, ... }
//!           ▼
//! ┌───────────────────┐
//! │  validator.rs     │  type-check function bodies, validate imports
//! └─────────┬─────────┘
//!           │ validated Module
//!           ▼
//! ┌───────────────────┐
//! │  interp.rs        │  stack-based execution of validated Module
//! └─────────┬─────────┘
//!           │ Vec<WasmValue> results
//!           ▼
//! ┌───────────────────┐
//! │  js_api.rs        │  expose to TJS as WebAssembly.* builtins
//! └───────────────────┘
//! ```
//!
//! # Supported Value Types
//!
//! - `i32` — 32-bit signed integer (stored as `i32`)
//! - `i64` — 64-bit signed integer (stored as `i64`)
//! - `f32` — 32-bit float (stored as `f32`)
//! - `f64` — 64-bit float (stored as `f64`)
//!
//! # Supported Instructions
//!
//! ## Constants
//!   `i32.const`, `i64.const`, `f32.const`, `f64.const`
//!
//! ## Arithmetic
//!   i32: `add`, `sub`, `mul`, `div_s`, `div_u`, `rem_s`, `rem_u`,
//!        `and`, `or`, `xor`, `shl`, `shr_s`, `shr_u`, `rotl`, `rotr`,
//!        `clz`, `ctz`, `popcnt`, `eqz`
//!   i64: same as i32
//!   f32/f64: `add`, `sub`, `mul`, `div`, `min`, `max`, `copysign`,
//!            `abs`, `neg`, `ceil`, `floor`, `trunc`, `nearest`, `sqrt`,
//!            `eq`, `ne`, `lt`, `gt`, `le`, `ge`
//!
//! ## Conversions
//!   `i32.wrap_i64`, `i64.extend_i32_s`, `i64.extend_i32_u`,
//!   `f32.convert_i32_s`, `f32.convert_i32_u`, `f32.convert_i64_s/u`,
//!   `f32.demote_f64`, `f64.promote_f32`, `f64.convert_i32_s/u`,
//!   `f64.convert_i64_s/u`, `i32.trunc_f32_s/u`, `i32.trunc_f64_s/u`,
//!   `i64.trunc_f32_s/u`, `i64.trunc_f64_s/u`,
//!   `i32.reinterpret_f32`, `i64.reinterpret_f64`,
//!   `f32.reinterpret_i32`, `f64.reinterpret_i64`
//!
//! ## Comparisons
//!   i32/i64: `eq`, `ne`, `lt_s`, `lt_u`, `gt_s`, `gt_u`, `le_s`, `le_u`,
//!            `ge_s`, `ge_u`
//!
//! ## Memory
//!   `memory.size`, `memory.grow`,
//!   `i32.load`, `i64.load`, `f32.load`, `f64.load`,
//!   `i32.store`, `i64.store`, `f32.store`, `f64.store`,
//!   and all the `.8/.16` variants (load8_s, load8_u, etc.)
//!
//! ## Control Flow
//!   `unreachable`, `nop`, `block`, `loop`, `if`, `else`, `end`,
//!   `br`, `br_if`, `br_table`, `return`, `call`, `call_indirect`,
//!   `drop`, `select`
//!
//! ## Variables
//!   `local.get`, `local.set`, `local.tee`, `global.get`, `global.set`

pub mod parser;
pub mod validator;
pub mod interp;
pub mod value;
pub mod memory;
pub mod table;
pub mod js_api;

pub use interp::{Instance, InstanceOptions};
pub use js_api::register_webassembly;
pub use memory::LinearMemory;
pub use parser::{Module, parse_module};
pub use table::Table;
pub use value::{WasmValue, ValType};

/// Errors that can occur during WASM compilation or execution.
#[derive(Debug, Clone)]
pub enum WasmError {
    /// Invalid binary format (e.g., wrong magic number, truncated section).
    Parse(String),
    /// Validation error (e.g., type mismatch, unknown function index).
    Validate(String),
    /// Runtime trap (e.g., divide by zero, out-of-bounds memory access).
    Trap(String),
    /// A host function (import) returned an error.
    HostError(String),
    /// An import was not satisfied (no matching export in the import module).
    LinkError(String),
}

impl std::fmt::Display for WasmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmError::Parse(s) => write!(f, "wasm parse error: {}", s),
            WasmError::Validate(s) => write!(f, "wasm validate error: {}", s),
            WasmError::Trap(s) => write!(f, "wasm trap: {}", s),
            WasmError::HostError(s) => write!(f, "wasm host error: {}", s),
            WasmError::LinkError(s) => write!(f, "wasm link error: {}", s),
        }
    }
}

impl std::error::Error for WasmError {}

/// WASM module magic number — `\0asm`.
pub const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];

/// WASM binary version (currently 1).
pub const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse a tiny valid WASM module (just the header).
    #[test]
    fn parse_empty_module() {
        let bytes = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        let module = parse_module(&bytes).expect("parse should succeed");
        assert!(module.types.is_empty());
        assert!(module.function_indices.is_empty());
        assert!(module.exports.is_empty());
    }

    #[test]
    fn parse_invalid_magic() {
        let bytes = [0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
        let result = parse_module(&bytes);
        assert!(result.is_err());
    }
}
