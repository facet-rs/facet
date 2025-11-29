//! Object (map) value type.

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc, realloc};
#[cfg(feature = "alloc")]
use alloc::borrow::ToOwned;
#[cfg(feature = "alloc")]
use alloc::collections::BTreeMap;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};
use core::iter::FromIterator;
use core::ops::{Index, IndexMut};
use core::{cmp, mem, ptr};

#[cfg(feature = "std")]
use std::collections::HashMap;

use crate::string::VString;
use crate::value::{TypeTag, Value};

/// A key-value pair.
#[repr(C)]
struct KeyValuePair {
    key: VString,
    value: Value,
}

/// Header for heap-allocated objects.
#[repr(C, align(8))]
struct ObjectHeader {
    /// Number of key-value pairs
    len: usize,
    /// Capacity
    cap: usize,
    // Array of KeyValuePair follows immediately after
}

/// An object (map) value.
///
/// `VObject` is an ordered map of string keys to `Value`s.
/// It preserves insertion order and uses linear search for lookups.
/// This is efficient for small objects (which are common in JSON).
#[repr(transparent)]
#[derive(Clone)]
pub struct VObject(pub(crate) Value);

impl VObject {
    fn layout(cap: usize) -> Layout {
        Layout::new::<ObjectHeader>()
            .extend(Layout::array::<KeyValuePair>(cap).unwrap())
            .unwrap()
            .0
            .pad_to_align()
    }

    #[cfg(feature = "alloc")]
    fn alloc(cap: usize) -> *mut ObjectHeader {
        unsafe {
            let layout = Self::layout(cap);
            let ptr = alloc(layout).cast::<ObjectHeader>();
            (*ptr).len = 0;
            (*ptr).cap = cap;
            ptr
        }
    }

    #[cfg(feature = "alloc")]
    fn realloc_ptr(ptr: *mut ObjectHeader, new_cap: usize) -> *mut ObjectHeader {
        unsafe {
            let old_cap = (*ptr).cap;
            let old_layout = Self::layout(old_cap);
            let new_layout = Self::layout(new_cap);
            let new_ptr =
                realloc(ptr.cast::<u8>(), old_layout, new_layout.size()).cast::<ObjectHeader>();
            (*new_ptr).cap = new_cap;
            new_ptr
        }
    }

    #[cfg(feature = "alloc")]
    fn dealloc_ptr(ptr: *mut ObjectHeader) {
        unsafe {
            let cap = (*ptr).cap;
            let layout = Self::layout(cap);
            dealloc(ptr.cast::<u8>(), layout);
        }
    }

    fn header(&self) -> &ObjectHeader {
        unsafe { &*(self.0.heap_ptr() as *const ObjectHeader) }
    }

    fn header_mut(&mut self) -> &mut ObjectHeader {
        unsafe { &mut *(self.0.heap_ptr_mut() as *mut ObjectHeader) }
    }

    fn items_ptr(&self) -> *const KeyValuePair {
        // Go through heap_ptr directly to avoid creating intermediate reference
        // that would limit provenance to just the header
        unsafe { (self.0.heap_ptr() as *const ObjectHeader).add(1).cast() }
    }

    fn items_ptr_mut(&mut self) -> *mut KeyValuePair {
        // Use heap_ptr_mut directly to preserve mutable provenance
        unsafe { (self.0.heap_ptr_mut() as *mut ObjectHeader).add(1).cast() }
    }

    fn items(&self) -> &[KeyValuePair] {
        unsafe { core::slice::from_raw_parts(self.items_ptr(), self.len()) }
    }

    fn items_mut(&mut self) -> &mut [KeyValuePair] {
        unsafe { core::slice::from_raw_parts_mut(self.items_ptr_mut(), self.len()) }
    }

    /// Creates a new empty object.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a new object with the specified capacity.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        unsafe {
            let ptr = Self::alloc(cap);
            VObject(Value::new_ptr(ptr.cast(), TypeTag::Object))
        }
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.header().len
    }

    /// Returns `true` if the object is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.header().cap
    }

    /// Reserves capacity for at least `additional` more entries.
    #[cfg(feature = "alloc")]
    pub fn reserve(&mut self, additional: usize) {
        let current_cap = self.capacity();
        let desired_cap = self
            .len()
            .checked_add(additional)
            .expect("capacity overflow");

        if current_cap >= desired_cap {
            return;
        }

        let new_cap = cmp::max(current_cap * 2, desired_cap.max(4));

        unsafe {
            let new_ptr = Self::realloc_ptr(self.0.heap_ptr_mut().cast(), new_cap);
            self.0.set_ptr(new_ptr.cast());
        }
    }

    /// Finds the index of a key.
    fn find_key(&self, key: &str) -> Option<usize> {
        self.items().iter().position(|kv| kv.key.as_str() == key)
    }

    /// Gets a value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.find_key(key).map(|i| &self.items()[i].value)
    }

    /// Gets a mutable value by key.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.find_key(key).map(|i| &mut self.items_mut()[i].value)
    }

    /// Gets a key-value pair by key.
    #[must_use]
    pub fn get_key_value(&self, key: &str) -> Option<(&VString, &Value)> {
        self.find_key(key).map(|i| {
            let kv = &self.items()[i];
            (&kv.key, &kv.value)
        })
    }

    /// Returns `true` if the object contains the key.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.find_key(key).is_some()
    }

    /// Inserts a key-value pair. Returns the old value if the key existed.
    #[cfg(feature = "alloc")]
    pub fn insert(&mut self, key: impl Into<VString>, value: impl Into<Value>) -> Option<Value> {
        let key = key.into();
        let value = value.into();

        if let Some(i) = self.find_key(key.as_str()) {
            // Key exists, replace value
            Some(mem::replace(&mut self.items_mut()[i].value, value))
        } else {
            // New key
            self.reserve(1);
            unsafe {
                let len = self.header().len;
                let ptr = self.items_ptr_mut().add(len);
                ptr.write(KeyValuePair { key, value });
                self.header_mut().len = len + 1;
            }
            None
        }
    }

    /// Removes a key-value pair. Returns the value if the key existed.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.remove_entry(key).map(|(_, v)| v)
    }

    /// Removes and returns a key-value pair.
    pub fn remove_entry(&mut self, key: &str) -> Option<(VString, Value)> {
        let idx = self.find_key(key)?;
        let len = self.len();

        unsafe {
            let ptr = self.items_ptr_mut().add(idx);
            let kv = ptr.read();

            // Shift remaining elements
            if idx < len - 1 {
                ptr::copy(ptr.add(1), ptr, len - idx - 1);
            }

            self.header_mut().len = len - 1;
            Some((kv.key, kv.value))
        }
    }

    /// Clears the object.
    pub fn clear(&mut self) {
        while !self.is_empty() {
            unsafe {
                let len = self.header().len;
                self.header_mut().len = len - 1;
                let ptr = self.items_ptr_mut().add(len - 1);
                ptr::drop_in_place(ptr);
            }
        }
    }

    /// Returns an iterator over keys.
    pub fn keys(&self) -> impl Iterator<Item = &VString> {
        self.items().iter().map(|kv| &kv.key)
    }

    /// Returns an iterator over values.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.items().iter().map(|kv| &kv.value)
    }

    /// Returns an iterator over mutable values.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.items_mut().iter_mut().map(|kv| &mut kv.value)
    }

    /// Returns an iterator over key-value pairs.
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self.items().iter(),
        }
    }

    /// Returns an iterator over mutable key-value pairs.
    pub fn iter_mut(&mut self) -> IterMut<'_> {
        IterMut {
            inner: self.items_mut().iter_mut(),
        }
    }

    /// Shrinks the capacity to match the length.
    #[cfg(feature = "alloc")]
    pub fn shrink_to_fit(&mut self) {
        let len = self.len();
        let cap = self.capacity();

        if len < cap {
            unsafe {
                let new_ptr = Self::realloc_ptr(self.0.heap_ptr_mut().cast(), len);
                self.0.set_ptr(new_ptr.cast());
            }
        }
    }

    pub(crate) fn clone_impl(&self) -> Value {
        let mut new = VObject::with_capacity(self.len());
        for kv in self.items() {
            new.insert(kv.key.clone(), kv.value.clone());
        }
        new.0
    }

    pub(crate) fn drop_impl(&mut self) {
        self.clear();
        unsafe {
            Self::dealloc_ptr(self.0.heap_ptr_mut().cast());
        }
    }
}

// === Iterators ===

/// Iterator over `(&VString, &Value)` pairs.
pub struct Iter<'a> {
    inner: core::slice::Iter<'a, KeyValuePair>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a VString, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|kv| (&kv.key, &kv.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for Iter<'_> {}

/// Iterator over `(&VString, &mut Value)` pairs.
pub struct IterMut<'a> {
    inner: core::slice::IterMut<'a, KeyValuePair>,
}

impl<'a> Iterator for IterMut<'a> {
    type Item = (&'a VString, &'a mut Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|kv| (&kv.key, &mut kv.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for IterMut<'_> {}

/// Iterator over owned `(VString, Value)` pairs.
pub struct ObjectIntoIter {
    object: VObject,
}

impl Iterator for ObjectIntoIter {
    type Item = (VString, Value);

    fn next(&mut self) -> Option<Self::Item> {
        if self.object.is_empty() {
            None
        } else {
            // Remove from the front to preserve order
            let key = self.object.items()[0].key.as_str().to_owned();
            self.object.remove_entry(&key)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.object.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for ObjectIntoIter {}

impl IntoIterator for VObject {
    type Item = (VString, Value);
    type IntoIter = ObjectIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        ObjectIntoIter { object: self }
    }
}

impl<'a> IntoIterator for &'a VObject {
    type Item = (&'a VString, &'a Value);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut VObject {
    type Item = (&'a VString, &'a mut Value);
    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

// === Index ===

impl Index<&str> for VObject {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key).expect("key not found")
    }
}

impl IndexMut<&str> for VObject {
    fn index_mut(&mut self, key: &str) -> &mut Value {
        self.get_mut(key).expect("key not found")
    }
}

// === Comparison ===

impl PartialEq for VObject {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for (k, v) in self.iter() {
            if other.get(k.as_str()) != Some(v) {
                return false;
            }
        }
        true
    }
}

impl Eq for VObject {}

impl Hash for VObject {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash length and then each key-value pair
        // Note: This doesn't depend on order, which is correct for map semantics
        self.len().hash(state);

        // Sum hashes to make order-independent (XOR is order-independent)
        let mut total: u64 = 0;
        for (k, _v) in self.iter() {
            // Simple hash combining for each pair
            let mut kh: u64 = 0;
            for byte in k.as_bytes() {
                kh = kh.wrapping_mul(31).wrapping_add(*byte as u64);
            }
            // Just XOR the key hash contribution
            total ^= kh;
        }
        total.hash(state);
    }
}

impl Debug for VObject {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl Default for VObject {
    fn default() -> Self {
        Self::new()
    }
}

// === FromIterator / Extend ===

#[cfg(feature = "alloc")]
impl<K: Into<VString>, V: Into<Value>> FromIterator<(K, V)> for VObject {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let mut obj = VObject::with_capacity(lower);
        for (k, v) in iter {
            obj.insert(k, v);
        }
        obj
    }
}

#[cfg(feature = "alloc")]
impl<K: Into<VString>, V: Into<Value>> Extend<(K, V)> for VObject {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        self.reserve(lower);
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

// === From implementations ===

#[cfg(feature = "std")]
impl<K: Into<VString>, V: Into<Value>> From<HashMap<K, V>> for VObject {
    fn from(map: HashMap<K, V>) -> Self {
        map.into_iter().collect()
    }
}

#[cfg(feature = "alloc")]
impl<K: Into<VString>, V: Into<Value>> From<BTreeMap<K, V>> for VObject {
    fn from(map: BTreeMap<K, V>) -> Self {
        map.into_iter().collect()
    }
}

// === Value conversions ===

impl AsRef<Value> for VObject {
    fn as_ref(&self) -> &Value {
        &self.0
    }
}

impl AsMut<Value> for VObject {
    fn as_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}

impl From<VObject> for Value {
    fn from(obj: VObject) -> Self {
        obj.0
    }
}

impl VObject {
    /// Converts this VObject into a Value, consuming self.
    #[inline]
    pub fn into_value(self) -> Value {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let obj = VObject::new();
        assert!(obj.is_empty());
        assert_eq!(obj.len(), 0);
    }

    #[test]
    fn test_insert_get() {
        let mut obj = VObject::new();
        obj.insert("name", Value::from("Alice"));
        obj.insert("age", Value::from(30));

        assert_eq!(obj.len(), 2);
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("age"));
        assert!(!obj.contains_key("email"));

        assert_eq!(
            obj.get("name").unwrap().as_string().unwrap().as_str(),
            "Alice"
        );
        assert_eq!(
            obj.get("age").unwrap().as_number().unwrap().to_i64(),
            Some(30)
        );
    }

    #[test]
    fn test_insert_replace() {
        let mut obj = VObject::new();
        assert!(obj.insert("key", Value::from(1)).is_none());
        assert!(obj.insert("key", Value::from(2)).is_some());
        assert_eq!(obj.len(), 1);
        assert_eq!(
            obj.get("key").unwrap().as_number().unwrap().to_i64(),
            Some(2)
        );
    }

    #[test]
    fn test_remove() {
        let mut obj = VObject::new();
        obj.insert("a", Value::from(1));
        obj.insert("b", Value::from(2));
        obj.insert("c", Value::from(3));

        let removed = obj.remove("b");
        assert!(removed.is_some());
        assert_eq!(obj.len(), 2);
        assert!(!obj.contains_key("b"));
    }

    #[test]
    fn test_clone() {
        let mut obj = VObject::new();
        obj.insert("key", Value::from("value"));

        let obj2 = obj.clone();
        assert_eq!(obj, obj2);
    }

    #[test]
    fn test_iter() {
        let mut obj = VObject::new();
        obj.insert("a", Value::from(1));
        obj.insert("b", Value::from(2));

        let keys: Vec<_> = obj.keys().map(|k| k.as_str()).collect();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn test_collect() {
        let obj: VObject = vec![("a", Value::from(1)), ("b", Value::from(2))]
            .into_iter()
            .collect();
        assert_eq!(obj.len(), 2);
    }

    #[test]
    fn test_index() {
        let mut obj = VObject::new();
        obj.insert("key", Value::from(42));

        assert_eq!(obj["key"].as_number().unwrap().to_i64(), Some(42));
    }
}
