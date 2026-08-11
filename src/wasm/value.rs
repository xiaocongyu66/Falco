//! WASM value types and runtime values.
//!
//! # Value Types
//!
//! WASM MVP supports four value types:
//! - `i32` (0x7F) — 32-bit signed integer
//! - `i64` (0x7E) — 64-bit signed integer
//! - `f32` (0x7D) — 32-bit IEEE 754 float
//! - `f64` (0x7C) — 64-bit IEEE 754 float
//!
//! # Runtime Values
//!
//! `WasmValue` is a tagged enum holding one of the four types. The
//! interpreter pushes/pops these on its operand stack.

use std::fmt;

/// A WASM value type encoding (the byte that appears in the binary format).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ValType {
    /// 32-bit signed integer. Binary encoding: 0x7F.
    I32 = 0x7F,
    /// 64-bit signed integer. Binary encoding: 0x7E.
    I64 = 0x7E,
    /// 32-bit IEEE 754 float. Binary encoding: 0x7D.
    F32 = 0x7D,
    /// 64-bit IEEE 754 float. Binary encoding: 0x7C.
    F64 = 0x7C,
    /// 128-bit vector (SIMD). Binary encoding: 0x7B.
    V128 = 0x7B,
    /// Function reference (reference types proposal). Binary encoding: 0x70.
    FuncRef = 0x70,
    /// External reference (reference types proposal). Binary encoding: 0x6F.
    ExternRef = 0x6F,
}

impl ValType {
    /// Decode a value type from its binary encoding byte.
    ///
    /// Returns `None` if the byte doesn't encode a valid value type.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x7F => Some(ValType::I32),
            0x7E => Some(ValType::I64),
            0x7D => Some(ValType::F32),
            0x7C => Some(ValType::F64),
            0x7B => Some(ValType::V128),
            0x70 => Some(ValType::FuncRef),
            0x6F => Some(ValType::ExternRef),
            _ => None,
        }
    }

    /// The size of this value type in bytes (when stored in linear memory).
    pub fn byte_size(&self) -> usize {
        match self {
            ValType::I32 | ValType::F32 => 4,
            ValType::I64 | ValType::F64 => 8,
            ValType::V128 => 16,
            ValType::FuncRef | ValType::ExternRef => 8, // pointer-sized
        }
    }

    /// The default zero value for this type.
    pub fn default_value(&self) -> WasmValue {
        match self {
            ValType::I32 => WasmValue::I32(0),
            ValType::I64 => WasmValue::I64(0),
            ValType::F32 => WasmValue::F32(0.0),
            ValType::F64 => WasmValue::F64(0.0),
            ValType::V128 => WasmValue::V128([0; 16]),
            ValType::FuncRef | ValType::ExternRef => WasmValue::NullRef,
        }
    }
}

impl fmt::Display for ValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValType::I32 => write!(f, "i32"),
            ValType::I64 => write!(f, "i64"),
            ValType::F32 => write!(f, "f32"),
            ValType::F64 => write!(f, "f64"),
            ValType::V128 => write!(f, "v128"),
            ValType::FuncRef => write!(f, "funcref"),
            ValType::ExternRef => write!(f, "externref"),
        }
    }
}

/// A runtime WASM value — one of the supported types.
#[derive(Debug, Clone, Copy)]
pub enum WasmValue {
    /// 32-bit signed integer.
    I32(i32),
    /// 64-bit signed integer.
    I64(i64),
    /// 32-bit float (stored as Rust `f32`).
    F32(f32),
    /// 64-bit float (stored as Rust `f64`).
    F64(f64),
    /// 128-bit vector (SIMD). Stored as 16 raw bytes.
    V128([u8; 16]),
    /// Null reference (for funcref/externref).
    NullRef,
    /// A function reference (index into the function table).
    FuncRef(u32),
    /// An external reference (opaque host value).
    ExternRef(u64),
}

impl WasmValue {
    /// Get the type of this value.
    pub fn val_type(&self) -> ValType {
        match self {
            WasmValue::I32(_) => ValType::I32,
            WasmValue::I64(_) => ValType::I64,
            WasmValue::F32(_) => ValType::F32,
            WasmValue::F64(_) => ValType::F64,
            WasmValue::V128(_) => ValType::V128,
            WasmValue::NullRef | WasmValue::FuncRef(_) => ValType::FuncRef,
            WasmValue::ExternRef(_) => ValType::ExternRef,
        }
    }

    /// Convert to i32 (for use as a memory address or condition).
    pub fn as_i32(&self) -> i32 {
        match self {
            WasmValue::I32(v) => *v,
            WasmValue::I64(v) => *v as i32, // truncate
            WasmValue::F32(v) => *v as i32, // saturating cast per spec
            WasmValue::F64(v) => *v as i32,
            _ => 0,
        }
    }

    /// Convert to i64.
    pub fn as_i64(&self) -> i64 {
        match self {
            WasmValue::I32(v) => *v as i64,
            WasmValue::I64(v) => *v,
            WasmValue::F32(v) => *v as i64,
            WasmValue::F64(v) => *v as i64,
            _ => 0,
        }
    }

    /// Convert to f32.
    pub fn as_f32(&self) -> f32 {
        match self {
            WasmValue::I32(v) => *v as f32,
            WasmValue::I64(v) => *v as f32,
            WasmValue::F32(v) => *v,
            WasmValue::F64(v) => *v as f32,
            _ => 0.0,
        }
    }

    /// Convert to f64.
    pub fn as_f64(&self) -> f64 {
        match self {
            WasmValue::I32(v) => *v as f64,
            WasmValue::I64(v) => *v as f64,
            WasmValue::F32(v) => *v as f64,
            WasmValue::F64(v) => *v,
            _ => 0.0,
        }
    }

    /// Convert to u32 (for unsigned interpretations).
    pub fn as_u32(&self) -> u32 {
        match self {
            WasmValue::I32(v) => *v as u32,
            WasmValue::I64(v) => *v as u32,
            _ => 0,
        }
    }

    /// Convert to u64 (for unsigned interpretations).
    pub fn as_u64(&self) -> u64 {
        match self {
            WasmValue::I32(v) => *v as u64,
            WasmValue::I64(v) => *v as u64,
            _ => 0,
        }
    }

    /// Is this value truthy (non-zero)?
    pub fn is_truthy(&self) -> bool {
        match self {
            WasmValue::I32(v) => *v != 0,
            WasmValue::I64(v) => *v != 0,
            WasmValue::F32(v) => *v != 0.0,
            WasmValue::F64(v) => *v != 0.0,
            WasmValue::V128(v) => v.iter().any(|&b| b != 0),
            WasmValue::NullRef => false,
            WasmValue::FuncRef(_) => true,
            WasmValue::ExternRef(_) => true,
        }
    }

    /// Default (zero) value for the given type.
    pub fn default_for(t: ValType) -> Self {
        t.default_value()
    }

    /// Get the V128 bytes (for SIMD operations).
    pub fn as_v128(&self) -> [u8; 16] {
        match self {
            WasmValue::V128(v) => *v,
            _ => [0; 16],
        }
    }

    /// Interpret the V128 as 4×i32.
    pub fn as_i32x4(&self) -> [i32; 4] {
        let bytes = self.as_v128();
        [
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            i32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        ]
    }

    /// Interpret the V128 as 4×f32.
    pub fn as_f32x4(&self) -> [f32; 4] {
        let bytes = self.as_v128();
        [
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        ]
    }

    /// Create a V128 from 4×i32.
    pub fn from_i32x4(lanes: [i32; 4]) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&lanes[0].to_le_bytes());
        bytes[4..8].copy_from_slice(&lanes[1].to_le_bytes());
        bytes[8..12].copy_from_slice(&lanes[2].to_le_bytes());
        bytes[12..16].copy_from_slice(&lanes[3].to_le_bytes());
        WasmValue::V128(bytes)
    }

    /// Create a V128 from 4×f32.
    pub fn from_f32x4(lanes: [f32; 4]) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&lanes[0].to_le_bytes());
        bytes[4..8].copy_from_slice(&lanes[1].to_le_bytes());
        bytes[8..12].copy_from_slice(&lanes[2].to_le_bytes());
        bytes[12..16].copy_from_slice(&lanes[3].to_le_bytes());
        WasmValue::V128(bytes)
    }
}

impl PartialEq for WasmValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (WasmValue::I32(a), WasmValue::I32(b)) => a == b,
            (WasmValue::I64(a), WasmValue::I64(b)) => a == b,
            // Per IEEE 754: NaN != NaN, even when bit patterns match.
            // f32::eq follows this rule.
            (WasmValue::F32(a), WasmValue::F32(b)) => a == b,
            (WasmValue::F64(a), WasmValue::F64(b)) => a == b,
            (WasmValue::V128(a), WasmValue::V128(b)) => a == b,
            (WasmValue::NullRef, WasmValue::NullRef) => true,
            (WasmValue::FuncRef(a), WasmValue::FuncRef(b)) => a == b,
            (WasmValue::ExternRef(a), WasmValue::ExternRef(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for WasmValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WasmValue::I32(v) => write!(f, "i32:{}", v),
            WasmValue::I64(v) => write!(f, "i64:{}", v),
            WasmValue::F32(v) => write!(f, "f32:{}", v),
            WasmValue::F64(v) => write!(f, "f64:{}", v),
            WasmValue::V128(v) => write!(f, "v128:{:02x?}", v),
            WasmValue::NullRef => write!(f, "nullref"),
            WasmValue::FuncRef(i) => write!(f, "funcref:{}", i),
            WasmValue::ExternRef(i) => write!(f, "externref:{}", i),
        }
    }
}

// ── Saturating float→int conversions (per WASM spec) ──────────────────
//
// Unlike Rust's default `as` cast (which wraps), WASM's `trunc_f32_s`
// etc. *saturate*: out-of-range values become INT_MIN/INT_MAX, and NaN
// becomes 0. These helpers implement the spec-correct behavior.

/// Saturating conversion: f32 → i32 (signed).
pub fn f32_to_i32_s(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else if v >= 2147483520.0_f32 {
        // 2^31 - 128, the largest f32 ≤ i32::MAX
        i32::MAX
    } else if v <= -2147483648.0_f32 {
        i32::MIN
    } else {
        v.trunc() as i32
    }
}

/// Saturating conversion: f32 → i32 (unsigned).
pub fn f32_to_i32_u(v: f32) -> u32 {
    if v.is_nan() {
        0
    } else if v >= 4294967040.0_f32 {
        // 2^32 - 256
        u32::MAX
    } else if v <= 0.0_f32 {
        0
    } else {
        v.trunc() as u32
    }
}

/// Saturating conversion: f32 → i64 (signed).
pub fn f32_to_i64_s(v: f32) -> i64 {
    if v.is_nan() {
        0
    } else if v >= 9223371487098961920.0_f32 {
        // 2^63 - 2^40 (largest f32 ≤ i64::MAX exactly)
        i64::MAX
    } else if v <= -9223372036854775808.0_f32 {
        i64::MIN
    } else {
        v.trunc() as i64
    }
}

/// Saturating conversion: f32 → i64 (unsigned).
pub fn f32_to_i64_u(v: f32) -> u64 {
    if v.is_nan() {
        0
    } else if v >= 18446742974197923840.0_f32 {
        u64::MAX
    } else if v <= 0.0_f32 {
        0
    } else {
        v.trunc() as u64
    }
}

/// Saturating conversion: f64 → i32 (signed).
pub fn f64_to_i32_s(v: f64) -> i32 {
    if v.is_nan() {
        0
    } else if v >= 2147483647.0_f64 {
        i32::MAX
    } else if v <= -2147483648.0_f64 {
        i32::MIN
    } else {
        v.trunc() as i32
    }
}

/// Saturating conversion: f64 → i32 (unsigned).
pub fn f64_to_i32_u(v: f64) -> u32 {
    if v.is_nan() {
        0
    } else if v >= 4294967295.0_f64 {
        u32::MAX
    } else if v <= 0.0_f64 {
        0
    } else {
        v.trunc() as u32
    }
}

/// Saturating conversion: f64 → i64 (signed).
pub fn f64_to_i64_s(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else if v >= 9223372036854774784.0_f64 {
        i64::MAX
    } else if v <= -9223372036854775808.0_f64 {
        i64::MIN
    } else {
        v.trunc() as i64
    }
}

/// Saturating conversion: f64 → i64 (unsigned).
pub fn f64_to_i64_u(v: f64) -> u64 {
    if v.is_nan() {
        0
    } else if v >= 18446744073709549568.0_f64 {
        u64::MAX
    } else if v <= 0.0_f64 {
        0
    } else {
        v.trunc() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn val_type_from_byte() {
        assert_eq!(ValType::from_byte(0x7F), Some(ValType::I32));
        assert_eq!(ValType::from_byte(0x7E), Some(ValType::I64));
        assert_eq!(ValType::from_byte(0x7D), Some(ValType::F32));
        assert_eq!(ValType::from_byte(0x7C), Some(ValType::F64));
        assert_eq!(ValType::from_byte(0x7B), Some(ValType::V128));
        assert_eq!(ValType::from_byte(0x70), Some(ValType::FuncRef));
        assert_eq!(ValType::from_byte(0x6F), Some(ValType::ExternRef));
        assert_eq!(ValType::from_byte(0x00), None);
    }

    #[test]
    fn val_type_byte_size() {
        assert_eq!(ValType::I32.byte_size(), 4);
        assert_eq!(ValType::I64.byte_size(), 8);
        assert_eq!(ValType::F32.byte_size(), 4);
        assert_eq!(ValType::F64.byte_size(), 8);
        assert_eq!(ValType::V128.byte_size(), 16);
        assert_eq!(ValType::FuncRef.byte_size(), 8);
    }

    #[test]
    fn v128_lane_operations() {
        let v = WasmValue::from_i32x4([1, 2, 3, 4]);
        let lanes = v.as_i32x4();
        assert_eq!(lanes, [1, 2, 3, 4]);
    }

    #[test]
    fn v128_f32x4_operations() {
        let v = WasmValue::from_f32x4([1.0, 2.0, 3.0, 4.0]);
        let lanes = v.as_f32x4();
        assert_eq!(lanes, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn v128_default_zero() {
        let v = ValType::V128.default_value();
        match v {
            WasmValue::V128(bytes) => assert!(bytes.iter().all(|&b| b == 0)),
            _ => panic!("expected V128"),
        }
    }

    #[test]
    fn null_ref_truthy() {
        assert!(!WasmValue::NullRef.is_truthy());
        assert!(WasmValue::FuncRef(0).is_truthy());
    }

    #[test]
    fn ref_type_display() {
        assert_eq!(format!("{}", ValType::V128), "v128");
        assert_eq!(format!("{}", ValType::FuncRef), "funcref");
        assert_eq!(format!("{}", ValType::ExternRef), "externref");
    }

    #[test]
    fn wasm_value_as_i32() {
        assert_eq!(WasmValue::I32(42).as_i32(), 42);
        assert_eq!(WasmValue::I64(100).as_i32(), 100);
        assert_eq!(WasmValue::F32(3.14).as_i32(), 3);
        assert_eq!(WasmValue::F64(9.99).as_i32(), 9);
    }

    #[test]
    fn saturating_conversions_normal() {
        assert_eq!(f32_to_i32_s(3.7), 3);
        assert_eq!(f32_to_i32_s(-3.7), -3);
        assert_eq!(f64_to_i64_s(1e10), 10_000_000_000_i64);
    }

    #[test]
    fn saturating_conversions_nan() {
        assert_eq!(f32_to_i32_s(f32::NAN), 0);
        assert_eq!(f64_to_i64_s(f64::NAN), 0);
    }

    #[test]
    fn saturating_conversions_overflow() {
        assert_eq!(f32_to_i32_s(1e20_f32), i32::MAX);
        assert_eq!(f32_to_i32_s(-1e20_f32), i32::MIN);
        assert_eq!(f64_to_i64_s(1e308), i64::MAX);
        assert_eq!(f64_to_i64_s(-1e308), i64::MIN);
    }

    #[test]
    fn saturating_unsigned_negative() {
        assert_eq!(f32_to_i32_u(-1.0), 0);
        assert_eq!(f64_to_i64_u(-1.0), 0);
    }

    #[test]
    fn wasm_value_truthy() {
        assert!(WasmValue::I32(1).is_truthy());
        assert!(!WasmValue::I32(0).is_truthy());
        assert!(WasmValue::F64(3.14).is_truthy());
        assert!(!WasmValue::F64(0.0).is_truthy());
    }

    #[test]
    fn wasm_value_equality() {
        assert_eq!(WasmValue::I32(1), WasmValue::I32(1));
        assert_ne!(WasmValue::I32(1), WasmValue::I64(1));
        // NaN != NaN for floats (per IEEE 754)
        assert_ne!(
            WasmValue::F32(f32::NAN),
            WasmValue::F32(f32::NAN)
        );
    }
}
