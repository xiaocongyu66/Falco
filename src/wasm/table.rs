//! WASM function table — used by `call_indirect`.
//!
//! A table holds function references (indices into the module's function
//! list). The `call_indirect` instruction looks up a function by table
//! index and calls it.

use crate::wasm::{parser::Limits, WasmError};
use std::cell::RefCell;
use std::rc::Rc;

/// A WASM table — an array of function indices.
#[derive(Debug)]
pub struct Table {
    /// The function indices stored in the table.
    /// `None` means the slot is empty (uninitialized).
    elements: Vec<Option<u32>>,
    /// Current size (number of elements).
    current_size: u32,
    /// Maximum size, if specified.
    max_size: Option<u32>,
}

impl Table {
    /// Create a new table with the given limits.
    pub fn new(limits: Limits) -> Result<Self, WasmError> {
        let min = limits.min;
        let max = limits.max;
        if let Some(m) = max {
            if m < min {
                return Err(WasmError::Validate(format!(
                    "table max {} < min {}",
                    m, min
                )));
            }
        }
        Ok(Self {
            elements: vec![None; min as usize],
            current_size: min,
            max_size: max,
        })
    }

    /// Current number of elements.
    pub fn size(&self) -> u32 {
        self.current_size
    }

    /// Grow the table by `delta` elements. Returns the old size.
    pub fn grow(&mut self, delta: u32) -> Result<u32, WasmError> {
        let old_size = self.current_size;
        let new_size = self.current_size.checked_add(delta).ok_or_else(|| {
            WasmError::Trap("table.grow: size overflow".to_string())
        })?;
        if let Some(max) = self.max_size {
            if new_size > max {
                return Err(WasmError::Trap(format!(
                    "table.grow would exceed max {} (current {}, delta {})",
                    max, self.current_size, delta
                )));
            }
        }
        self.elements.resize(new_size as usize, None);
        self.current_size = new_size;
        Ok(old_size)
    }

    /// Get the function index at `idx`. Returns an error if out of bounds
    /// or the slot is empty (null).
    pub fn get(&self, idx: u32) -> Result<u32, WasmError> {
        let i = idx as usize;
        if i >= self.elements.len() {
            return Err(WasmError::Trap(format!(
                "table.get: out of bounds index {} (size {})",
                idx, self.current_size
            )));
        }
        self.elements[i].ok_or_else(|| {
            WasmError::Trap(format!(
                "table.get: null reference at index {}",
                idx
            ))
        })
    }

    /// Set the function index at `idx`.
    pub fn set(&mut self, idx: u32, func_idx: u32) -> Result<(), WasmError> {
        let i = idx as usize;
        if i >= self.elements.len() {
            return Err(WasmError::Trap(format!(
                "table.set: out of bounds index {} (size {})",
                idx, self.current_size
            )));
        }
        self.elements[i] = Some(func_idx);
        Ok(())
    }

    /// Bulk-set elements starting at `offset`.
    pub fn set_bulk(&mut self, offset: u32, indices: &[u32]) -> Result<(), WasmError> {
        let start = offset as usize;
        let end = start.checked_add(indices.len()).ok_or_else(|| {
            WasmError::Trap("table.set_bulk: offset overflow".to_string())
        })?;
        if end > self.elements.len() {
            return Err(WasmError::Trap(format!(
                "table.set_bulk: out of bounds (offset {} + count {} > size {})",
                offset,
                indices.len(),
                self.current_size
            )));
        }
        for (i, &func_idx) in indices.iter().enumerate() {
            self.elements[start + i] = Some(func_idx);
        }
        Ok(())
    }
}

/// A shared table (Rc<RefCell> for shared ownership).
pub type SharedTable = Rc<RefCell<Table>>;

/// Create a shared table.
pub fn new_shared(limits: Limits) -> Result<SharedTable, WasmError> {
    Ok(Rc::new(RefCell::new(Table::new(limits)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_creation() {
        let t = Table::new(Limits {
            min: 5,
            max: Some(10),
        })
        .unwrap();
        assert_eq!(t.size(), 5);
    }

    #[test]
    fn table_get_set() {
        let mut t = Table::new(Limits {
            min: 5,
            max: None,
        })
        .unwrap();
        t.set(2, 42).unwrap();
        assert_eq!(t.get(2).unwrap(), 42);
    }

    #[test]
    fn table_get_unset() {
        let t = Table::new(Limits {
            min: 5,
            max: None,
        })
        .unwrap();
        // Slot 0 is uninitialized.
        assert!(t.get(0).is_err());
    }

    #[test]
    fn table_out_of_bounds() {
        let t = Table::new(Limits {
            min: 5,
            max: None,
        })
        .unwrap();
        assert!(t.get(100).is_err());
    }

    #[test]
    fn table_grow() {
        let mut t = Table::new(Limits {
            min: 1,
            max: Some(10),
        })
        .unwrap();
        let old = t.grow(3).unwrap();
        assert_eq!(old, 1);
        assert_eq!(t.size(), 4);
    }

    #[test]
    fn table_grow_exceeds_max() {
        let mut t = Table::new(Limits {
            min: 1,
            max: Some(2),
        })
        .unwrap();
        assert!(t.grow(5).is_err());
    }

    #[test]
    fn table_bulk_set() {
        let mut t = Table::new(Limits {
            min: 10,
            max: None,
        })
        .unwrap();
        t.set_bulk(2, &[10, 20, 30]).unwrap();
        assert_eq!(t.get(2).unwrap(), 10);
        assert_eq!(t.get(3).unwrap(), 20);
        assert_eq!(t.get(4).unwrap(), 30);
    }
}
