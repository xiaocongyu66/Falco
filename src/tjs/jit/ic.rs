//! Inline caches for property access.
//!
//! # Why Inline Caches?
//!
//! Every `GetProperty(name)` bytecode normally does a HashMap lookup on
//! the object's properties. For hot code, this is slow.
//!
//! An inline cache (IC) remembers the *shape* of the object seen last
//! time, and the offset where the property was found. On subsequent
//! accesses, we just check the shape and load from the cached offset.
//!
//! # Monomorphic IC
//!
//! The simplest form: cache exactly one (shape_id, offset) pair.
//!
//! ```text
//! get_property_x:
//!   mov rax, [obj_ptr + shape_offset]    ; load object's shape id
//!   cmp rax, CACHED_SHAPE_ID             ; is it the same shape?
//!   jne .slow_path                       ; no → fall back to VM
//!   mov rax, [obj_ptr + CACHED_OFFSET]   ; yes → fast load
//!   ret
//! .slow_path:
//!   ; call VM's get_property, then update this IC
//! ```
//!
//! # Polymorphic IC
//!
//! If we see multiple shapes at the same access site, we extend to a
//! polymorphic IC that caches up to 4 shapes. Beyond that, we go
//! "megamorphic" and just call the VM every time.

use crate::tjs::value::{ObjectValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Maximum number of shapes cached in a polymorphic IC before going megamorphic.
pub const POLY_IC_MAX: usize = 4;

/// A monomorphic inline cache entry.
///
/// Records the (shape_id, property_offset) pair seen at a GetProperty
/// or SetProperty site.
#[derive(Debug, Clone, Default)]
pub struct InlineCacheEntry {
    /// A "shape id" — a hash of the object's property key set.
    /// Two objects with the same shape_id have the same keys in the
    /// same order (so the same property will be at the same offset).
    pub shape_id: u64,
    /// The offset (in the object's properties HashMap) where the
    /// property was found. For our HashMap-backed ObjectValue, this
    /// is just a key string (we don't have true shape-based storage).
    /// In a real engine, this would be a byte offset.
    pub key: String,
    /// The cached property value (for read-only access patterns).
    /// None for SetProperty caches.
    pub cached_value: Option<Value>,
    /// Whether this IC has ever been populated.
    pub initialized: bool,
}

/// An inline cache for a single bytecode site.
#[derive(Debug, Clone, Default)]
pub struct InlineCache {
    /// Monomorphic entry (most recent).
    pub mono: InlineCacheEntry,
    /// Polymorphic entries (when more than one shape is seen).
    pub poly: Vec<InlineCacheEntry>,
    /// Whether this IC has gone megamorphic (always calls VM).
    pub megamorphic: bool,
    /// Hit count (for profiling).
    pub hits: u64,
    /// Miss count.
    pub misses: u64,
}

impl InlineCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute a "shape id" for an object — a hash of its property key set.
    ///
    /// Two objects with the same keys (in any order) get the same shape_id.
    /// This is conservative — objects with different insertion orders but
    /// same keys will collide, but that's OK (it just means we'll do a
    /// HashMap lookup and find the property at a different "offset").
    pub fn compute_shape(obj: &ObjectValue) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // Hash the sorted list of keys.
        let mut keys: Vec<&String> = obj.properties.keys().collect();
        keys.sort();
        for k in keys {
            k.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Look up a property using this IC.
    ///
    /// Returns `Some(value)` on hit, `None` on miss (caller should then
    /// do a full lookup and call `update` to populate the cache).
    pub fn get(&mut self, obj: &ObjectValue, name: &str) -> Option<Value> {
        if self.megamorphic {
            self.misses += 1;
            return None;
        }

        let shape_id = Self::compute_shape(obj);

        // Try monomorphic first.
        if self.mono.initialized && self.mono.shape_id == shape_id {
            self.hits += 1;
            return obj.properties.get(name).cloned();
        }

        // Try polymorphic.
        for entry in &self.poly {
            if entry.shape_id == shape_id {
                self.hits += 1;
                return obj.properties.get(name).cloned();
            }
        }

        self.misses += 1;
        None
    }

    /// Update the IC after a miss.
    ///
    /// Records the new (shape_id, key) pair. If the IC is monomorphic
    /// and sees a different shape, it becomes polymorphic. If it sees
    /// more than POLY_IC_MAX shapes, it goes megamorphic.
    pub fn update(&mut self, obj: &ObjectValue, _name: &str) {
        if self.megamorphic {
            return;
        }

        let shape_id = Self::compute_shape(obj);

        if !self.mono.initialized {
            // First time — become monomorphic.
            self.mono = InlineCacheEntry {
                shape_id,
                key: _name.to_string(),
                cached_value: None,
                initialized: true,
            };
            return;
        }

        if self.mono.shape_id == shape_id {
            // Already cached — nothing to do.
            return;
        }

        // Different shape — promote to polymorphic.
        if self.poly.is_empty() {
            // Move the existing monomorphic entry to poly[0].
            self.poly.push(self.mono.clone());
        }

        // Check if this shape is already in poly.
        for entry in &self.poly {
            if entry.shape_id == shape_id {
                return; // already cached
            }
        }

        if self.poly.len() >= POLY_IC_MAX {
            // Too many shapes — go megamorphic.
            self.megamorphic = true;
            self.poly.clear();
            self.mono.initialized = false;
            return;
        }

        self.poly.push(InlineCacheEntry {
            shape_id,
            key: _name.to_string(),
            cached_value: None,
            initialized: true,
        });
    }

    /// Reset the IC (e.g., after a deopt that invalidated the cache).
    pub fn reset(&mut self) {
        self.mono = InlineCacheEntry::default();
        self.poly.clear();
        self.megamorphic = false;
        self.hits = 0;
        self.misses = 0;
    }

    /// Is this IC monomorphic (exactly one shape cached)?
    pub fn is_monomorphic(&self) -> bool {
        self.mono.initialized && self.poly.is_empty() && !self.megamorphic
    }

    /// Hit rate as a fraction (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_obj(pairs: &[(&str, f64)]) -> ObjectValue {
        let mut obj = ObjectValue::new();
        for (k, v) in pairs {
            obj.properties.insert(k.to_string(), Value::Number(*v));
        }
        obj
    }

    #[test]
    fn ic_monomorphic_hit() {
        let mut ic = InlineCache::new();
        let obj = make_obj(&[("x", 1.0), ("y", 2.0)]);

        // First access — miss.
        assert!(ic.get(&obj, "x").is_none());
        ic.update(&obj, "x");

        // Second access — hit.
        assert_eq!(ic.get(&obj, "x"), Some(Value::Number(1.0)));
        assert_eq!(ic.hits, 1);
        assert_eq!(ic.misses, 1);
    }

    #[test]
    fn ic_polymorphic() {
        let mut ic = InlineCache::new();
        let obj1 = make_obj(&[("x", 1.0), ("y", 2.0)]);
        let obj2 = make_obj(&[("x", 3.0), ("z", 4.0)]);

        // obj1 — miss + cache.
        ic.get(&obj1, "x");
        ic.update(&obj1, "x");

        // obj2 — miss + cache (different shape).
        ic.get(&obj2, "x");
        ic.update(&obj2, "x");

        // Now both should hit.
        assert_eq!(ic.get(&obj1, "x"), Some(Value::Number(1.0)));
        assert_eq!(ic.get(&obj2, "x"), Some(Value::Number(3.0)));
    }

    #[test]
    fn ic_megamorphic() {
        let mut ic = InlineCache::new();
        // Add 5 different shapes → should go megamorphic.
        for i in 0..(POLY_IC_MAX + 2) {
            let obj = make_obj(&[("k", i as f64), (&format!("extra{}", i), 0.0)]);
            ic.get(&obj, "k");
            ic.update(&obj, "k");
        }
        assert!(ic.megamorphic);
        // Megamorphic always misses.
        let obj = make_obj(&[("k", 99.0)]);
        assert!(ic.get(&obj, "k").is_none());
    }

    #[test]
    fn ic_shape_id_stable() {
        let obj1 = make_obj(&[("a", 1.0), ("b", 2.0)]);
        let obj2 = make_obj(&[("a", 3.0), ("b", 4.0)]);
        // Same keys, different values → same shape.
        assert_eq!(
            InlineCache::compute_shape(&obj1),
            InlineCache::compute_shape(&obj2)
        );
    }
}
