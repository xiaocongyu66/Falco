//! TJS extensions — Promise/microtask, Symbol, BigInt, Map/Set, WeakMap/WeakSet,
//! Iterator protocol, Generator, Reflect API.
//!
//! These are the Web standard runtime features that complement the existing
//! TJS interpreter/VM. They are implemented as Rust types with the JS-visible
//! API surface mounted by the JS bridge layer.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

// ===================== Symbol =====================

#[derive(Debug)]
pub struct Symbol {
    pub description: Option<String>,
    pub id: u64,
}

impl Symbol {
    pub fn new(description: Option<String>, id: u64) -> Self {
        Self { description, id }
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Symbol {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WellKnownSymbol {
    Iterator,
    AsyncIterator,
    HasInstance,
    ToPrimitive,
    ToStringTag,
    IsConcatSpreadable,
    Species,
    Unscopables,
    Match,
    MatchAll,
    Replace,
    Search,
    Split,
}

impl WellKnownSymbol {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Iterator => "Symbol.iterator",
            Self::AsyncIterator => "Symbol.asyncIterator",
            Self::HasInstance => "Symbol.hasInstance",
            Self::ToPrimitive => "Symbol.toPrimitive",
            Self::ToStringTag => "Symbol.toStringTag",
            Self::IsConcatSpreadable => "Symbol.isConcatSpreadable",
            Self::Species => "Symbol.species",
            Self::Unscopables => "Symbol.unscopables",
            Self::Match => "Symbol.match",
            Self::MatchAll => "Symbol.matchAll",
            Self::Replace => "Symbol.replace",
            Self::Search => "Symbol.search",
            Self::Split => "Symbol.split",
        }
    }

    pub fn id(&self) -> u64 {
        match self {
            Self::Iterator => 1,
            Self::AsyncIterator => 2,
            Self::HasInstance => 3,
            Self::ToPrimitive => 4,
            Self::ToStringTag => 5,
            Self::IsConcatSpreadable => 6,
            Self::Species => 7,
            Self::Unscopables => 8,
            Self::Match => 9,
            Self::MatchAll => 10,
            Self::Replace => 11,
            Self::Search => 12,
            Self::Split => 13,
        }
    }
}

// ===================== BigInt =====================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigInt {
    pub limbs: Vec<u32>,
    pub negative: bool,
}

impl BigInt {
    pub fn zero() -> Self {
        Self {
            limbs: vec![],
            negative: false,
        }
    }

    pub fn from_i64(n: i64) -> Self {
        let negative = n < 0;
        let mag = if negative {
            (n as i128).unsigned_abs() as u64
        } else {
            n as u64
        };
        let mut limbs = Vec::new();
        let mut m = mag;
        while m > 0 {
            limbs.push((m & 0xFFFFFFFF) as u32);
            m >>= 32;
        }
        let has_limbs = !limbs.is_empty();
        Self {
            limbs,
            negative: negative && has_limbs,
        }
    }

    pub fn to_i64(&self) -> Option<i64> {
        if self.limbs.is_empty() {
            return Some(0);
        }
        if self.limbs.len() > 2 {
            return None;
        }
        let low = self.limbs[0] as u64;
        let high = if self.limbs.len() == 2 {
            (self.limbs[1] as u64) << 32
        } else {
            0
        };
        let mag = low | high;
        if self.negative {
            if mag > i64::MAX as u64 + 1 {
                return None;
            }
            Some(-(mag as i128) as i64)
        } else {
            if mag > i64::MAX as u64 {
                return None;
            }
            Some(mag as i64)
        }
    }

    pub fn add(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            let mut result = Vec::new();
            let mut carry = 0u32;
            let max_len = self.limbs.len().max(other.limbs.len());
            for i in 0..max_len {
                let a = self.limbs.get(i).copied().unwrap_or(0) as u64;
                let b = other.limbs.get(i).copied().unwrap_or(0) as u64;
                let sum = a + b + carry as u64;
                result.push((sum & 0xFFFFFFFF) as u32);
                carry = (sum >> 32) as u32;
            }
            if carry > 0 {
                result.push(carry);
            }
            Self {
                limbs: result,
                negative: self.negative,
            }
        } else {
            let cmp = self.cmp_mag(other);
            match cmp {
                std::cmp::Ordering::Equal => Self::zero(),
                std::cmp::Ordering::Greater => {
                    let mut result = self.sub_mag(other);
                    result.negative = self.negative;
                    result
                }
                std::cmp::Ordering::Less => {
                    let mut result = other.sub_mag(self);
                    result.negative = other.negative;
                    result
                }
            }
        }
    }

    pub fn negate(&self) -> Self {
        if self.limbs.is_empty() {
            return self.clone();
        }
        Self {
            limbs: self.limbs.clone(),
            negative: !self.negative,
        }
    }

    fn cmp_mag(&self, other: &Self) -> std::cmp::Ordering {
        if self.limbs.len() != other.limbs.len() {
            return self.limbs.len().cmp(&other.limbs.len());
        }
        for i in (0..self.limbs.len()).rev() {
            if self.limbs[i] != other.limbs[i] {
                return self.limbs[i].cmp(&other.limbs[i]);
            }
        }
        std::cmp::Ordering::Equal
    }

    fn sub_mag(&self, other: &Self) -> Self {
        let mut result = Vec::new();
        let mut borrow = 0i32;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as i64;
            let b = other.limbs.get(i).copied().unwrap_or(0) as i64;
            let mut diff = a - b - borrow as i64;
            if diff < 0 {
                diff += 1i64 << 32;
                borrow = 1;
            } else {
                borrow = 0;
            }
            result.push((diff & 0xFFFFFFFF) as u32);
        }
        while result.last() == Some(&0) {
            result.pop();
        }
        Self {
            limbs: result,
            negative: false,
        }
    }

    pub fn to_string(&self) -> String {
        if self.limbs.is_empty() {
            return "0".to_string();
        }
        let mut limbs = self.limbs.clone();
        let mut digits = String::new();
        while !limbs.is_empty() {
            let mut rem: u64 = 0;
            for i in (0..limbs.len()).rev() {
                let cur = (rem << 32) | limbs[i] as u64;
                limbs[i] = (cur / 1_000_000_000) as u32;
                rem = cur % 1_000_000_000;
            }
            while limbs.last() == Some(&0) {
                limbs.pop();
            }
            let chunk = rem.to_string();
            if digits.is_empty() {
                digits = chunk;
            } else {
                digits = format!("{}{:0>9}", chunk, digits);
            }
        }
        if self.negative {
            format!("-{}", digits)
        } else {
            digits
        }
    }
}

// ===================== Promise + Microtask Queue =====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromiseState {
    Pending,
    Fulfilled,
    Rejected,
}

pub struct Promise {
    pub state: RefCell<PromiseState>,
    pub value: RefCell<Option<PromiseValue>>,
    pub then_callbacks: RefCell<Vec<PromiseCallback>>,
    pub catch_callbacks: RefCell<Vec<PromiseCallback>>,
    pub finally_callbacks: RefCell<Vec<Box<dyn Fn()>>>,
}

#[derive(Debug, Clone)]
pub enum PromiseValue {
    Resolved(crate::tjs::value::Value),
    Rejected(crate::tjs::value::Value),
}

pub type PromiseCallback = Rc<dyn Fn(PromiseValue) -> Result<PromiseValue, String>>;

impl Promise {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(PromiseState::Pending),
            value: RefCell::new(None),
            then_callbacks: RefCell::new(Vec::new()),
            catch_callbacks: RefCell::new(Vec::new()),
            finally_callbacks: RefCell::new(Vec::new()),
        })
    }

    pub fn resolve(&self, value: crate::tjs::value::Value) {
        if *self.state.borrow() != PromiseState::Pending {
            return;
        }
        *self.state.borrow_mut() = PromiseState::Fulfilled;
        *self.value.borrow_mut() = Some(PromiseValue::Resolved(value.clone()));
        let callbacks: Vec<_> = self.then_callbacks.borrow_mut().drain(..).collect();
        for cb in callbacks {
            let _ = cb(PromiseValue::Resolved(value.clone()));
        }
        let finally: Vec<_> = self.finally_callbacks.borrow_mut().drain(..).collect();
        for f in finally {
            f();
        }
    }

    pub fn reject(&self, reason: crate::tjs::value::Value) {
        if *self.state.borrow() != PromiseState::Pending {
            return;
        }
        *self.state.borrow_mut() = PromiseState::Rejected;
        *self.value.borrow_mut() = Some(PromiseValue::Rejected(reason.clone()));
        let callbacks: Vec<_> = self.catch_callbacks.borrow_mut().drain(..).collect();
        for cb in callbacks {
            let _ = cb(PromiseValue::Rejected(reason.clone()));
        }
        let finally: Vec<_> = self.finally_callbacks.borrow_mut().drain(..).collect();
        for f in finally {
            f();
        }
    }

    pub fn then(&self, callback: PromiseCallback) -> Rc<Self> {
        let next = Promise::new();
        let next_clone = next.clone();
        let wrapped: PromiseCallback = Rc::new(move |value| {
            let result = callback(value.clone())?;
            match &result {
                PromiseValue::Resolved(v) => next_clone.resolve(v.clone()),
                PromiseValue::Rejected(v) => next_clone.reject(v.clone()),
            }
            Ok(result)
        });
        match *self.state.borrow() {
            PromiseState::Pending => {
                self.then_callbacks.borrow_mut().push(wrapped);
            }
            PromiseState::Fulfilled => {
                let value = self.value.borrow().clone().unwrap();
                let _ = wrapped(value);
            }
            PromiseState::Rejected => {}
        }
        next
    }

    pub fn catch(&self, callback: PromiseCallback) -> Rc<Self> {
        let next = Promise::new();
        let next_clone = next.clone();
        let wrapped: PromiseCallback = Rc::new(move |value| {
            let result = callback(value.clone())?;
            match &result {
                PromiseValue::Resolved(v) => next_clone.resolve(v.clone()),
                PromiseValue::Rejected(v) => next_clone.reject(v.clone()),
            }
            Ok(result)
        });
        match *self.state.borrow() {
            PromiseState::Pending => {
                self.catch_callbacks.borrow_mut().push(wrapped);
            }
            PromiseState::Rejected => {
                let value = self.value.borrow().clone().unwrap();
                let _ = wrapped(value);
            }
            _ => {}
        }
        next
    }
}

pub struct MicrotaskQueue {
    pub queue: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl Default for MicrotaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl MicrotaskQueue {
    pub fn new() -> Self {
        Self {
            queue: RefCell::new(Vec::new()),
        }
    }

    pub fn enqueue<F: FnOnce() + 'static>(&self, f: F) {
        self.queue.borrow_mut().push(Box::new(f));
    }

    pub fn drain(&self) {
        loop {
            let task = self.queue.borrow_mut().pop();
            match task {
                Some(t) => t(),
                None => break,
            }
        }
    }
}

// ===================== Map / Set / WeakMap / WeakSet =====================

pub struct JsMap {
    pub entries: RefCell<Vec<(crate::tjs::value::Value, crate::tjs::value::Value)>>,
}

impl Default for JsMap {
    fn default() -> Self {
        Self::new()
    }
}

impl JsMap {
    pub fn new() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
        }
    }

    pub fn get(&self, key: &crate::tjs::value::Value) -> Option<crate::tjs::value::Value> {
        self.entries
            .borrow()
            .iter()
            .find(|(k, _)| values_equal(k, key))
            .map(|(_, v)| v.clone())
    }

    pub fn set(&self, key: crate::tjs::value::Value, value: crate::tjs::value::Value) {
        let mut entries = self.entries.borrow_mut();
        if let Some(slot) = entries.iter_mut().find(|(k, _)| values_equal(k, &key)) {
            slot.1 = value;
        } else {
            entries.push((key, value));
        }
    }

    pub fn has(&self, key: &crate::tjs::value::Value) -> bool {
        self.entries
            .borrow()
            .iter()
            .any(|(k, _)| values_equal(k, key))
    }

    pub fn delete(&self, key: &crate::tjs::value::Value) -> bool {
        let mut entries = self.entries.borrow_mut();
        let len = entries.len();
        entries.retain(|(k, _)| !values_equal(k, key));
        entries.len() != len
    }

    pub fn size(&self) -> usize {
        self.entries.borrow().len()
    }
}

pub struct JsSet {
    pub values: RefCell<Vec<crate::tjs::value::Value>>,
}

impl Default for JsSet {
    fn default() -> Self {
        Self::new()
    }
}

impl JsSet {
    pub fn new() -> Self {
        Self {
            values: RefCell::new(Vec::new()),
        }
    }

    pub fn add(&self, value: crate::tjs::value::Value) {
        if !self.has(&value) {
            self.values.borrow_mut().push(value);
        }
    }

    pub fn has(&self, value: &crate::tjs::value::Value) -> bool {
        self.values.borrow().iter().any(|v| values_equal(v, value))
    }

    pub fn delete(&self, value: &crate::tjs::value::Value) -> bool {
        let mut values = self.values.borrow_mut();
        let len = values.len();
        values.retain(|v| !values_equal(v, value));
        values.len() != len
    }

    pub fn size(&self) -> usize {
        self.values.borrow().len()
    }
}

pub struct WeakMap {
    pub entries: RefCell<
        Vec<(
            Weak<RefCell<crate::tjs::value::ObjectValue>>,
            crate::tjs::value::Value,
        )>,
    >,
}

impl Default for WeakMap {
    fn default() -> Self {
        Self::new()
    }
}

impl WeakMap {
    pub fn new() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
        }
    }

    pub fn get(&self, key: &crate::tjs::value::Value) -> Option<crate::tjs::value::Value> {
        if let crate::tjs::value::Value::Object(obj) = key {
            self.entries
                .borrow()
                .iter()
                .find(|(w, _)| w.upgrade().map(|s| Rc::ptr_eq(&s, obj)).unwrap_or(false))
                .map(|(_, v)| v.clone())
        } else {
            None
        }
    }

    pub fn set(
        &self,
        key: crate::tjs::value::Value,
        value: crate::tjs::value::Value,
    ) -> Result<(), String> {
        if let crate::tjs::value::Value::Object(obj) = &key {
            let weak = Rc::downgrade(obj);
            let mut entries = self.entries.borrow_mut();
            if let Some(slot) = entries
                .iter_mut()
                .find(|(w, _)| w.upgrade().map(|s| Rc::ptr_eq(&s, obj)).unwrap_or(false))
            {
                slot.1 = value;
            } else {
                entries.push((weak, value));
            }
            entries.retain(|(w, _)| w.upgrade().is_some());
            Ok(())
        } else {
            Err("WeakMap key must be an object".to_string())
        }
    }

    pub fn has(&self, key: &crate::tjs::value::Value) -> bool {
        self.get(key).is_some()
    }

    pub fn delete(&self, key: &crate::tjs::value::Value) -> bool {
        if let crate::tjs::value::Value::Object(obj) = key {
            let mut entries = self.entries.borrow_mut();
            let len = entries.len();
            entries.retain(|(w, _)| !w.upgrade().map(|s| Rc::ptr_eq(&s, obj)).unwrap_or(false));
            entries.len() != len
        } else {
            false
        }
    }
}

pub struct WeakSet {
    pub values: RefCell<Vec<Weak<RefCell<crate::tjs::value::ObjectValue>>>>,
}

impl Default for WeakSet {
    fn default() -> Self {
        Self::new()
    }
}

impl WeakSet {
    pub fn new() -> Self {
        Self {
            values: RefCell::new(Vec::new()),
        }
    }

    pub fn add(&self, value: crate::tjs::value::Value) -> Result<(), String> {
        if let crate::tjs::value::Value::Object(obj) = &value {
            let weak = Rc::downgrade(obj);
            let mut values = self.values.borrow_mut();
            if !values
                .iter()
                .any(|w| w.upgrade().map(|s| Rc::ptr_eq(&s, obj)).unwrap_or(false))
            {
                values.push(weak);
            }
            values.retain(|w| w.upgrade().is_some());
            Ok(())
        } else {
            Err("WeakSet value must be an object".to_string())
        }
    }

    pub fn has(&self, value: &crate::tjs::value::Value) -> bool {
        if let crate::tjs::value::Value::Object(obj) = value {
            self.values
                .borrow()
                .iter()
                .any(|w| w.upgrade().map(|s| Rc::ptr_eq(&s, obj)).unwrap_or(false))
        } else {
            false
        }
    }

    pub fn delete(&self, value: &crate::tjs::value::Value) -> bool {
        if let crate::tjs::value::Value::Object(obj) = value {
            let mut values = self.values.borrow_mut();
            let len = values.len();
            values.retain(|w| !w.upgrade().map(|s| Rc::ptr_eq(&s, obj)).unwrap_or(false));
            values.len() != len
        } else {
            false
        }
    }
}

fn values_equal(a: &crate::tjs::value::Value, b: &crate::tjs::value::Value) -> bool {
    use crate::tjs::value::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::Undefined, Value::Undefined) => true,
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        (Value::Array(x), Value::Array(y)) => Rc::ptr_eq(x, y),
        (Value::Function(x), Value::Function(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

// ===================== Iterator Protocol =====================

#[derive(Debug, Clone)]
pub struct IteratorResult {
    pub value: crate::tjs::value::Value,
    pub done: bool,
}

pub struct Iterator {
    pub next_fn: Rc<dyn Fn() -> Result<IteratorResult, String>>,
}

impl Iterator {
    pub fn new<F: Fn() -> Result<IteratorResult, String> + 'static>(f: F) -> Self {
        Self {
            next_fn: Rc::new(f),
        }
    }

    pub fn next(&self) -> Result<IteratorResult, String> {
        (self.next_fn)()
    }

    pub fn collect(&self) -> Result<Vec<crate::tjs::value::Value>, String> {
        let mut out = Vec::new();
        loop {
            let r = self.next()?;
            if r.done {
                break;
            }
            out.push(r.value);
        }
        Ok(out)
    }
}

// ===================== Generator =====================

pub struct Generator {
    pub segments: Vec<GeneratorSegment>,
    pub current: RefCell<usize>,
    pub done: RefCell<bool>,
}

pub type GeneratorSegment = Rc<dyn Fn(&crate::tjs::value::Value) -> Result<GeneratorYield, String>>;

#[derive(Debug, Clone)]
pub enum GeneratorYield {
    Yield(crate::tjs::value::Value),
    Return(crate::tjs::value::Value),
}

impl Generator {
    pub fn new(segments: Vec<GeneratorSegment>) -> Self {
        Self {
            segments,
            current: RefCell::new(0),
            done: RefCell::new(false),
        }
    }

    pub fn next(&self, value: Option<crate::tjs::value::Value>) -> Result<IteratorResult, String> {
        if *self.done.borrow() {
            return Ok(IteratorResult {
                value: crate::tjs::value::Value::Undefined,
                done: true,
            });
        }
        let injected = value.unwrap_or(crate::tjs::value::Value::Undefined);
        let segment_idx = *self.current.borrow();
        if segment_idx >= self.segments.len() {
            *self.done.borrow_mut() = true;
            return Ok(IteratorResult {
                value: crate::tjs::value::Value::Undefined,
                done: true,
            });
        }
        let segment = &self.segments[segment_idx];
        let result = segment(&injected)?;
        *self.current.borrow_mut() += 1;
        match result {
            GeneratorYield::Yield(v) => Ok(IteratorResult {
                value: v,
                done: false,
            }),
            GeneratorYield::Return(v) => {
                *self.done.borrow_mut() = true;
                Ok(IteratorResult {
                    value: v,
                    done: true,
                })
            }
        }
    }
}

// ===================== Reflect API =====================

pub fn reflect_get(
    target: &crate::tjs::value::Value,
    key: &str,
) -> Result<crate::tjs::value::Value, String> {
    if let crate::tjs::value::Value::Object(obj) = target {
        if let Some(v) = obj.borrow().properties.get(key) {
            return Ok(v.clone());
        }
        let proto = obj.borrow().prototype.clone();
        if let Some(p) = proto {
            return reflect_get(&p, key);
        }
        Ok(crate::tjs::value::Value::Undefined)
    } else {
        Err("Reflect.get target must be an object".to_string())
    }
}

pub fn reflect_set(
    target: &crate::tjs::value::Value,
    key: &str,
    value: crate::tjs::value::Value,
) -> Result<bool, String> {
    if let crate::tjs::value::Value::Object(obj) = target {
        obj.borrow_mut().properties.insert(key.to_string(), value);
        Ok(true)
    } else {
        Err("Reflect.set target must be an object".to_string())
    }
}

pub fn reflect_has(target: &crate::tjs::value::Value, key: &str) -> Result<bool, String> {
    if let crate::tjs::value::Value::Object(obj) = target {
        if obj.borrow().properties.contains_key(key) {
            return Ok(true);
        }
        if let Some(proto) = obj.borrow().prototype.clone() {
            return reflect_has(&proto, key);
        }
        Ok(false)
    } else {
        Err("Reflect.has target must be an object".to_string())
    }
}

pub fn reflect_delete(target: &crate::tjs::value::Value, key: &str) -> Result<bool, String> {
    if let crate::tjs::value::Value::Object(obj) = target {
        Ok(obj.borrow_mut().properties.remove(key).is_some())
    } else {
        Err("Reflect.deleteProperty target must be an object".to_string())
    }
}

pub fn reflect_own_keys(target: &crate::tjs::value::Value) -> Result<Vec<String>, String> {
    if let crate::tjs::value::Value::Object(obj) = target {
        Ok(obj.borrow().properties.keys().cloned().collect())
    } else {
        Err("Reflect.ownKeys target must be an object".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tjs::value::Value;

    #[test]
    fn symbol_uniqueness() {
        let a = Symbol::new(Some("foo".into()), 1);
        let b = Symbol::new(Some("foo".into()), 2);
        assert!(a != b);
        let c = Symbol::new(Some("foo".into()), 1);
        assert!(a == c);
    }

    #[test]
    fn well_known_symbols_have_fixed_ids() {
        assert_eq!(WellKnownSymbol::Iterator.id(), 1);
        assert_eq!(WellKnownSymbol::AsyncIterator.id(), 2);
    }

    #[test]
    fn bigint_addition() {
        let a = BigInt::from_i64(100);
        let b = BigInt::from_i64(200);
        let c = a.add(&b);
        assert_eq!(c.to_i64(), Some(300));
    }

    #[test]
    fn bigint_large() {
        let a = BigInt::from_i64(i64::MAX);
        let b = BigInt::from_i64(1);
        let c = a.add(&b);
        assert!(c.to_i64().is_none());
        assert_eq!(c.to_string(), "9223372036854775808");
    }

    #[test]
    fn bigint_negative() {
        let a = BigInt::from_i64(-50);
        let b = BigInt::from_i64(30);
        let c = a.add(&b);
        assert_eq!(c.to_i64(), Some(-20));
    }

    #[test]
    fn bigint_to_string() {
        let a = BigInt::from_i64(123456789);
        assert_eq!(a.to_string(), "123456789");
        let b = BigInt::from_i64(-42);
        assert_eq!(b.to_string(), "-42");
    }

    #[test]
    fn promise_resolves_and_fires_then() {
        let p = Promise::new();
        let called = Rc::new(RefCell::new(false));
        let called_clone = called.clone();
        let _ = p.then(Rc::new(move |v| {
            if let PromiseValue::Resolved(_) = v {
                *called_clone.borrow_mut() = true;
            }
            Ok(v)
        }));
        p.resolve(Value::Number(42.0));
        assert!(*called.borrow());
    }

    #[test]
    fn microtask_queue_drains() {
        let q = MicrotaskQueue::new();
        let counter = Rc::new(RefCell::new(0));
        let c1 = counter.clone();
        let c2 = counter.clone();
        q.enqueue(move || {
            *c1.borrow_mut() += 1;
        });
        q.enqueue(move || {
            *c2.borrow_mut() += 1;
        });
        q.drain();
        assert_eq!(*counter.borrow(), 2);
    }

    #[test]
    fn map_operations() {
        let m = JsMap::new();
        m.set(Value::String("a".into()), Value::Number(1.0));
        m.set(Value::String("b".into()), Value::Number(2.0));
        assert_eq!(m.size(), 2);
        assert!(m.has(&Value::String("a".into())));
        assert!(!m.has(&Value::String("c".into())));
        let v = m.get(&Value::String("a".into()));
        assert!(matches!(v, Some(Value::Number(n)) if n == 1.0));
        assert!(m.delete(&Value::String("a".into())));
        assert_eq!(m.size(), 1);
    }

    #[test]
    fn set_operations() {
        let s = JsSet::new();
        s.add(Value::Number(1.0));
        s.add(Value::Number(2.0));
        s.add(Value::Number(1.0));
        assert_eq!(s.size(), 2);
        assert!(s.has(&Value::Number(1.0)));
        assert!(s.delete(&Value::Number(1.0)));
        assert_eq!(s.size(), 1);
    }

    #[test]
    fn weakmap_rejects_non_object_keys() {
        let wm = WeakMap::new();
        let result = wm.set(Value::Number(42.0), Value::String("foo".into()));
        assert!(result.is_err());
    }

    #[test]
    fn generator_yields_values() {
        let segments: Vec<GeneratorSegment> = vec![
            Rc::new(|_| Ok(GeneratorYield::Yield(Value::Number(1.0)))),
            Rc::new(|_| Ok(GeneratorYield::Yield(Value::Number(2.0)))),
            Rc::new(|_| Ok(GeneratorYield::Return(Value::Number(3.0)))),
        ];
        let gen = Generator::new(segments);
        let r1 = gen.next(None).unwrap();
        assert!(!r1.done);
        assert!(matches!(r1.value, Value::Number(n) if n == 1.0));
        let r2 = gen.next(None).unwrap();
        assert!(!r2.done);
        let r3 = gen.next(None).unwrap();
        assert!(r3.done);
        let r4 = gen.next(None).unwrap();
        assert!(r4.done);
    }

    #[test]
    fn reflect_get_set() {
        let obj = Rc::new(RefCell::new(crate::tjs::value::ObjectValue {
            properties: HashMap::new(),
            prototype: None,
        }));
        let target = Value::Object(obj);
        reflect_set(&target, "x", Value::Number(42.0)).unwrap();
        let v = reflect_get(&target, "x").unwrap();
        assert!(matches!(v, Value::Number(n) if n == 42.0));
        assert!(reflect_has(&target, "x").unwrap());
        assert!(!reflect_has(&target, "y").unwrap());
    }

    #[test]
    fn iterator_collect() {
        let counter = Rc::new(RefCell::new(0));
        let c = counter.clone();
        let iter = Iterator::new(move || {
            let mut n = c.borrow_mut();
            *n += 1;
            if *n <= 3 {
                Ok(IteratorResult {
                    value: Value::Number(*n as f64),
                    done: false,
                })
            } else {
                Ok(IteratorResult {
                    value: Value::Undefined,
                    done: true,
                })
            }
        });
        let v = iter.collect().unwrap();
        assert_eq!(v.len(), 3);
    }
}
