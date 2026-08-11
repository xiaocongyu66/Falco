//! WASM linear memory — a contiguous byte buffer with grow/load/store.
//!
//! Per the WASM spec, a linear memory:
//! - Is a contiguous array of bytes
//! - Has a minimum and optional maximum size (in pages)
//! - Each page is 64 KiB (65,536 bytes)
//! - Can be grown dynamically (but not past the maximum)
//! - All load/store operations are bounds-checked

use crate::wasm::{parser::Limits, WasmError};
use std::cell::RefCell;
use std::rc::Rc;

/// Size of a WASM memory page in bytes (64 KiB).
pub const PAGE_SIZE: usize = 64 * 1024; // 65,536

/// Maximum number of pages allowed in a WASM memory (per spec: 65,536).
pub const MAX_PAGES: u32 = 65_536;

/// A linear memory instance.
#[derive(Debug)]
pub struct LinearMemory {
    /// The backing byte buffer.
    data: Vec<u8>,
    /// Current size in pages.
    current_pages: u32,
    /// Maximum size in pages (if specified).
    max_pages: Option<u32>,
}

impl LinearMemory {
    /// Create a new linear memory with the given limits.
    pub fn new(limits: Limits) -> Result<Self, WasmError> {
        let min = limits.min;
        let max = limits.max;
        if min > MAX_PAGES {
            return Err(WasmError::Validate(format!(
                "memory min {} exceeds max allowed {}",
                min, MAX_PAGES
            )));
        }
        if let Some(m) = max {
            if m < min {
                return Err(WasmError::Validate(format!(
                    "memory max {} < min {}",
                    m, min
                )));
            }
            if m > MAX_PAGES {
                return Err(WasmError::Validate(format!(
                    "memory max {} exceeds spec max {}",
                    m, MAX_PAGES
                )));
            }
        }
        let data = vec![0u8; min as usize * PAGE_SIZE];
        Ok(Self {
            data,
            current_pages: min,
            max_pages: max,
        })
    }

    /// Current size in pages.
    pub fn size_pages(&self) -> u32 {
        self.current_pages
    }

    /// Current size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.data.len()
    }

    /// Grow the memory by `delta` pages. Returns the old size (in pages) on
    /// success, or -1 (as u32::MAX in some conventions) on failure.
    pub fn grow(&mut self, delta: u32) -> Result<u32, WasmError> {
        let old_pages = self.current_pages;
        let new_pages = self.current_pages.checked_add(delta);
        match new_pages {
            None => Err(WasmError::Trap("memory.grow overflow".to_string())),
            Some(new) if new > MAX_PAGES => Err(WasmError::Trap("memory.grow exceeds limit".to_string())),
            Some(new) => {
                if let Some(max) = self.max_pages {
                    if new > max {
                        return Err(WasmError::Trap(format!(
                            "memory.grow would exceed max {} (current {}, requested {})",
                            max, self.current_pages, delta
                        )));
                    }
                }
                self.data.resize(new as usize * PAGE_SIZE, 0);
                self.current_pages = new;
                Ok(old_pages)
            }
        }
    }

    /// Get a slice of `n` bytes starting at `offset`. Bounds-checked.
    fn get_slice(&self, offset: u32, n: usize) -> Result<&[u8], WasmError> {
        let offset = offset as usize;
        if offset.checked_add(n).map_or(true, |end| end > self.data.len()) {
            return Err(WasmError::Trap(format!(
                "out-of-bounds memory access: offset={} len={} memory_size={}",
                offset,
                n,
                self.data.len()
            )));
        }
        Ok(&self.data[offset..offset + n])
    }

    /// Get a mutable slice of `n` bytes starting at `offset`. Bounds-checked.
    fn get_slice_mut(&mut self, offset: u32, n: usize) -> Result<&mut [u8], WasmError> {
        let offset = offset as usize;
        if offset.checked_add(n).map_or(true, |end| end > self.data.len()) {
            return Err(WasmError::Trap(format!(
                "out-of-bounds memory access: offset={} len={} memory_size={}",
                offset,
                n,
                self.data.len()
            )));
        }
        Ok(&mut self.data[offset..offset + n])
    }

    // ── Load operations ──────────────────────────────────────────────

    pub fn load_i32(&self, offset: u32) -> Result<i32, WasmError> {
        let bytes = self.get_slice(offset, 4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn load_i64(&self, offset: u32) -> Result<i64, WasmError> {
        let bytes = self.get_slice(offset, 8)?;
        Ok(i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn load_f32(&self, offset: u32) -> Result<f32, WasmError> {
        let bytes = self.get_slice(offset, 4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn load_f64(&self, offset: u32) -> Result<f64, WasmError> {
        let bytes = self.get_slice(offset, 8)?;
        Ok(f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn load_u8(&self, offset: u32) -> Result<u8, WasmError> {
        let bytes = self.get_slice(offset, 1)?;
        Ok(bytes[0])
    }

    pub fn load_u16(&self, offset: u32) -> Result<u16, WasmError> {
        let bytes = self.get_slice(offset, 2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn load_u32(&self, offset: u32) -> Result<u32, WasmError> {
        let bytes = self.get_slice(offset, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Load a byte and sign-extend to i32 (for load8_s).
    pub fn load_i8(&self, offset: u32) -> Result<i32, WasmError> {
        let bytes = self.get_slice(offset, 1)?;
        Ok(bytes[0] as i8 as i32)
    }

    /// Load a 16-bit value and sign-extend to i32 (for load16_s).
    pub fn load_i16(&self, offset: u32) -> Result<i32, WasmError> {
        let bytes = self.get_slice(offset, 2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]) as i16 as i32)
    }

    // ── Store operations ─────────────────────────────────────────────

    pub fn store_i32(&mut self, offset: u32, value: i32) -> Result<(), WasmError> {
        let bytes = value.to_le_bytes();
        self.get_slice_mut(offset, 4)?.copy_from_slice(&bytes);
        Ok(())
    }

    pub fn store_i64(&mut self, offset: u32, value: i64) -> Result<(), WasmError> {
        let bytes = value.to_le_bytes();
        self.get_slice_mut(offset, 8)?.copy_from_slice(&bytes);
        Ok(())
    }

    pub fn store_f32(&mut self, offset: u32, value: f32) -> Result<(), WasmError> {
        let bytes = value.to_le_bytes();
        self.get_slice_mut(offset, 4)?.copy_from_slice(&bytes);
        Ok(())
    }

    pub fn store_f64(&mut self, offset: u32, value: f64) -> Result<(), WasmError> {
        let bytes = value.to_le_bytes();
        self.get_slice_mut(offset, 8)?.copy_from_slice(&bytes);
        Ok(())
    }

    pub fn store_u8(&mut self, offset: u32, value: u8) -> Result<(), WasmError> {
        self.get_slice_mut(offset, 1)?[0] = value;
        Ok(())
    }

    pub fn store_u16(&mut self, offset: u32, value: u16) -> Result<(), WasmError> {
        let bytes = value.to_le_bytes();
        self.get_slice_mut(offset, 2)?.copy_from_slice(&bytes);
        Ok(())
    }

    /// Write raw bytes (for data segment initialization).
    pub fn store_bytes(&mut self, offset: u32, data: &[u8]) -> Result<(), WasmError> {
        let slice = self.get_slice_mut(offset, data.len())?;
        slice.copy_from_slice(data);
        Ok(())
    }

    /// Get the entire memory as a byte slice (for debugging / snapshots).
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// A shared linear memory (Rc<RefCell> for shared ownership in imports).
pub type SharedMemory = Rc<RefCell<LinearMemory>>;

/// Create a shared linear memory.
pub fn new_shared(limits: Limits) -> Result<SharedMemory, WasmError> {
    Ok(Rc::new(RefCell::new(LinearMemory::new(limits)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_creation() {
        let mem = LinearMemory::new(Limits {
            min: 1,
            max: None,
        })
        .unwrap();
        assert_eq!(mem.size_pages(), 1);
        assert_eq!(mem.size_bytes(), PAGE_SIZE);
    }

    #[test]
    fn memory_with_max() {
        let mem = LinearMemory::new(Limits {
            min: 1,
            max: Some(10),
        })
        .unwrap();
        assert_eq!(mem.size_pages(), 1);
    }

    #[test]
    fn memory_invalid_min() {
        let result = LinearMemory::new(Limits {
            min: MAX_PAGES + 1,
            max: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn memory_grow() {
        let mut mem = LinearMemory::new(Limits {
            min: 1,
            max: Some(10),
        })
        .unwrap();
        let old = mem.grow(2).unwrap();
        assert_eq!(old, 1);
        assert_eq!(mem.size_pages(), 3);
    }

    #[test]
    fn memory_grow_exceeds_max() {
        let mut mem = LinearMemory::new(Limits {
            min: 1,
            max: Some(2),
        })
        .unwrap();
        let result = mem.grow(5);
        assert!(result.is_err());
    }

    #[test]
    fn memory_load_store_i32() {
        let mut mem = LinearMemory::new(Limits {
            min: 1,
            max: None,
        })
        .unwrap();
        mem.store_i32(0, 42).unwrap();
        assert_eq!(mem.load_i32(0).unwrap(), 42);

        mem.store_i32(100, -123).unwrap();
        assert_eq!(mem.load_i32(100).unwrap(), -123);
    }

    #[test]
    fn memory_load_store_i64() {
        let mut mem = LinearMemory::new(Limits {
            min: 1,
            max: None,
        })
        .unwrap();
        mem.store_i64(8, 0x1234_5678_9ABC_DEF0_i64).unwrap();
        assert_eq!(mem.load_i64(8).unwrap(), 0x1234_5678_9ABC_DEF0_i64);
    }

    #[test]
    fn memory_load_store_f64() {
        let mut mem = LinearMemory::new(Limits {
            min: 1,
            max: None,
        })
        .unwrap();
        mem.store_f64(0, 3.141592653589793).unwrap();
        let loaded = mem.load_f64(0).unwrap();
        assert!((loaded - 3.141592653589793).abs() < 1e-15);
    }

    #[test]
    fn memory_out_of_bounds() {
        let mem = LinearMemory::new(Limits {
            min: 1,
            max: None,
        })
        .unwrap();
        // Try to read past the end.
        let result = mem.load_i32(mem.size_bytes() as u32 - 2);
        assert!(result.is_err());
    }

    #[test]
    fn memory_byte_operations() {
        let mut mem = LinearMemory::new(Limits {
            min: 1,
            max: None,
        })
        .unwrap();
        mem.store_u8(0, 0xFF).unwrap();
        // load8_s: should be -1 (sign-extended)
        assert_eq!(mem.load_i8(0).unwrap(), -1);
        // load8_u: should be 255
        let val = mem.load_u8(0).unwrap();
        assert_eq!(val, 0xFF);
    }

    #[test]
    fn memory_store_bytes() {
        let mut mem = LinearMemory::new(Limits {
            min: 1,
            max: None,
        })
        .unwrap();
        let data = b"hello world";
        mem.store_bytes(100, data).unwrap();
        assert_eq!(&mem.as_bytes()[100..100 + data.len()], data);
    }
}
