//! Object (map) value type.

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc, realloc};
#[cfg(feature = "alloc")]
use alloc::borrow::ToOwned;
#[cfg(feature = "std")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::collections::BTreeMap;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};
use core::iter::FromIterator;
use core::ops::{Index, IndexMut};
use core::{cmp, mem, ptr};

#[cfg(feature = "std")]
use indexmap::IndexMap;
#[cfg(feature = "std")]
use std::collections::HashMap;

use crate::string::VString;
use crate::value::{TypeTag, Value};

/// Threshold at which we switch from inline array to IndexMap storage.
/// Below this size, linear search is competitive with hash lookups due to cache locality.
#[cfg(feature = "std")]
const LARGE_MODE_THRESHOLD: usize = 32;

/// Sentinel value for capacity indicating large mode (IndexMap storage).
#[cfg(feature = "std")]
const LARGE_MODE_CAP_SENTINEL: usize = usize::MAX;

/// A key-value pair.
#[repr(C)]
struct KeyValuePair {
    key: VString,
    value: Value,
}

/// Header for heap-allocated objects in small mode.
#[repr(C, align(8))]
struct ObjectHeader {
    /// Number of key-value pairs
    len: usize,
    /// Capacity (usize::MAX indicates large mode with IndexMap storage)
    cap: usize,
    // Array of KeyValuePair follows immediately after (only in small mode)
}

/// Wrapper for IndexMap storage in large mode.
/// Uses the same layout prefix as ObjectHeader so we can detect the mode.
#[cfg(feature = "std")]
#[repr(C, align(8))]
struct LargeModeStorage {
    /// Unused in large mode, but must be at same offset as ObjectHeader.len
    _len_unused: usize,
    /// Sentinel value (usize::MAX) to indicate large mode
    cap_sentinel: usize,
    /// The actual IndexMap
    map: IndexMap<VString, Value>,
}

/// An object (map) value.
///
/// `VObject` is an ordered map of string keys to `Value`s.
/// It preserves insertion order.
///
/// Storage modes:
/// - Small mode (default): inline array of KeyValuePair with linear search
/// - Large mode (std feature, >= 32 entries): IndexMap for O(1) lookups
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

    /// Returns true if this object is in large mode (IndexMap storage).
    #[cfg(feature = "std")]
    #[inline]
    fn is_large_mode(&self) -> bool {
        // In large mode, the cap_sentinel field (at same offset as ObjectHeader.cap)
        // is set to LARGE_MODE_CAP_SENTINEL
        unsafe {
            let header = self.0.heap_ptr() as *const ObjectHeader;
            (*header).cap == LARGE_MODE_CAP_SENTINEL
        }
    }

    /// Returns true if this object is in large mode (IndexMap storage).
    #[cfg(not(feature = "std"))]
    #[inline]
    fn is_large_mode(&self) -> bool {
        // Without std, we never use large mode
        false
    }

    /// Returns a reference to the IndexMap (large mode only).
    #[cfg(feature = "std")]
    #[inline]
    fn as_indexmap(&self) -> &IndexMap<VString, Value> {
        debug_assert!(self.is_large_mode());
        unsafe {
            let storage = self.0.heap_ptr() as *const LargeModeStorage;
            &(*storage).map
        }
    }

    /// Returns a mutable reference to the IndexMap (large mode only).
    #[cfg(feature = "std")]
    #[inline]
    fn as_indexmap_mut(&mut self) -> &mut IndexMap<VString, Value> {
        debug_assert!(self.is_large_mode());
        unsafe {
            let storage = self.0.heap_ptr_mut() as *mut LargeModeStorage;
            &mut (*storage).map
        }
    }

    fn header(&self) -> &ObjectHeader {
        debug_assert!(!self.is_large_mode());
        unsafe { &*(self.0.heap_ptr() as *const ObjectHeader) }
    }

    fn header_mut(&mut self) -> &mut ObjectHeader {
        debug_assert!(!self.is_large_mode());
        unsafe { &mut *(self.0.heap_ptr_mut() as *mut ObjectHeader) }
    }

    fn items_ptr(&self) -> *const KeyValuePair {
        debug_assert!(!self.is_large_mode());
        // Go through heap_ptr directly to avoid creating intermediate reference
        // that would limit provenance to just the header
        unsafe { (self.0.heap_ptr() as *const ObjectHeader).add(1).cast() }
    }

    fn items_ptr_mut(&mut self) -> *mut KeyValuePair {
        debug_assert!(!self.is_large_mode());
        // Use heap_ptr_mut directly to preserve mutable provenance
        unsafe { (self.0.heap_ptr_mut() as *mut ObjectHeader).add(1).cast() }
    }

    fn items(&self) -> &[KeyValuePair] {
        debug_assert!(!self.is_large_mode());
        unsafe { core::slice::from_raw_parts(self.items_ptr(), self.small_len()) }
    }

    fn items_mut(&mut self) -> &mut [KeyValuePair] {
        debug_assert!(!self.is_large_mode());
        unsafe { core::slice::from_raw_parts_mut(self.items_ptr_mut(), self.small_len()) }
    }

    /// Returns the length when in small mode.
    #[inline]
    fn small_len(&self) -> usize {
        debug_assert!(!self.is_large_mode());
        self.header().len
    }

    /// Converts from small mode to large mode (IndexMap).
    #[cfg(feature = "std")]
    fn convert_to_large_mode(&mut self) {
        debug_assert!(!self.is_large_mode());

        // Build IndexMap from existing items
        let mut map = IndexMap::with_capacity(self.small_len() + 1);
        unsafe {
            let len = self.small_len();
            let items_ptr = self.items_ptr_mut();

            // Move items into the IndexMap (taking ownership)
            for i in 0..len {
                let kv = items_ptr.add(i).read();
                map.insert(kv.key, kv.value);
            }

            // Free the old small-mode allocation
            Self::dealloc_ptr(self.0.heap_ptr_mut().cast());

            // Allocate and store the LargeModeStorage
            let storage = LargeModeStorage {
                _len_unused: 0,
                cap_sentinel: LARGE_MODE_CAP_SENTINEL,
                map,
            };
            let boxed = Box::new(storage);
            let ptr = Box::into_raw(boxed);
            self.0.set_ptr(ptr.cast());
        }
    }

    /// Creates a new empty object.
    #[cfg(feature = "alloc")]
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a new object with the specified capacity.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        // For large initial capacity with std feature, start directly in large mode
        #[cfg(feature = "std")]
        if cap >= LARGE_MODE_THRESHOLD {
            let storage = LargeModeStorage {
                _len_unused: 0,
                cap_sentinel: LARGE_MODE_CAP_SENTINEL,
                map: IndexMap::with_capacity(cap),
            };
            let boxed = Box::new(storage);
            let ptr = Box::into_raw(boxed);
            return VObject(unsafe { Value::new_ptr(ptr.cast(), TypeTag::Object) });
        }

        unsafe {
            let ptr = Self::alloc(cap);
            VObject(Value::new_ptr(ptr.cast(), TypeTag::Object))
        }
    }

    /// Returns the number of entries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap().len();
        }
        self.header().len
    }

    /// Returns `true` if the object is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the capacity.
    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap().capacity();
        }
        self.header().cap
    }

    /// Reserves capacity for at least `additional` more entries.
    #[cfg(feature = "alloc")]
    pub fn reserve(&mut self, additional: usize) {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            self.as_indexmap_mut().reserve(additional);
            return;
        }

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

    /// Gets a value by key.
    #[inline]
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap().get(key);
        }
        self.items()
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| &kv.value)
    }

    /// Gets a mutable value by key.
    #[inline]
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap_mut().get_mut(key);
        }
        self.items_mut()
            .iter_mut()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| &mut kv.value)
    }

    /// Gets a key-value pair by key.
    #[inline]
    #[must_use]
    pub fn get_key_value(&self, key: &str) -> Option<(&VString, &Value)> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap().get_key_value(key);
        }
        self.items()
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| (&kv.key, &kv.value))
    }

    /// Returns `true` if the object contains the key.
    #[inline]
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap().contains_key(key);
        }
        self.items().iter().any(|kv| kv.key.as_str() == key)
    }

    /// Inserts a key-value pair. Returns the old value if the key existed.
    #[cfg(feature = "alloc")]
    pub fn insert(&mut self, key: impl Into<VString>, value: impl Into<Value>) -> Option<Value> {
        let key = key.into();
        let value = value.into();

        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap_mut().insert(key, value);
        }

        // Check if key exists (linear search in small mode)
        if let Some(idx) = self
            .items()
            .iter()
            .position(|kv| kv.key.as_str() == key.as_str())
        {
            // Key exists, replace value
            return Some(mem::replace(&mut self.items_mut()[idx].value, value));
        }

        // Check if we should convert to large mode
        #[cfg(feature = "std")]
        if self.small_len() >= LARGE_MODE_THRESHOLD {
            self.convert_to_large_mode();
            return self.as_indexmap_mut().insert(key, value);
        }

        // New key in small mode
        self.reserve(1);
        let new_idx = self.header().len;

        unsafe {
            let ptr = self.items_ptr_mut().add(new_idx);
            ptr.write(KeyValuePair { key, value });
            self.header_mut().len = new_idx + 1;
        }

        None
    }

    /// Removes a key-value pair. Returns the value if the key existed.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.remove_entry(key).map(|(_, v)| v)
    }

    /// Removes and returns a key-value pair.
    pub fn remove_entry(&mut self, key: &str) -> Option<(VString, Value)> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return self.as_indexmap_mut().shift_remove_entry(key);
        }

        let idx = self.items().iter().position(|kv| kv.key.as_str() == key)?;
        let len = self.small_len();

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
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            self.as_indexmap_mut().clear();
            return;
        }

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
    #[inline]
    pub fn keys(&self) -> Keys<'_> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return Keys(KeysInner::Large(self.as_indexmap().keys()));
        }
        Keys(KeysInner::Small(self.items().iter()))
    }

    /// Returns an iterator over values.
    #[inline]
    pub fn values(&self) -> Values<'_> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return Values(ValuesInner::Large(self.as_indexmap().values()));
        }
        Values(ValuesInner::Small(self.items().iter()))
    }

    /// Returns an iterator over mutable values.
    #[inline]
    pub fn values_mut(&mut self) -> ValuesMut<'_> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return ValuesMut(ValuesMutInner::Large(self.as_indexmap_mut().values_mut()));
        }
        ValuesMut(ValuesMutInner::Small(self.items_mut().iter_mut()))
    }

    /// Returns an iterator over key-value pairs.
    #[inline]
    pub fn iter(&self) -> Iter<'_> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return Iter(IterInner::Large(self.as_indexmap().iter()));
        }
        Iter(IterInner::Small(self.items().iter()))
    }

    /// Returns an iterator over mutable key-value pairs.
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_> {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            return IterMut(IterMutInner::Large(self.as_indexmap_mut().iter_mut()));
        }
        IterMut(IterMutInner::Small(self.items_mut().iter_mut()))
    }

    /// Shrinks the capacity to match the length.
    #[cfg(feature = "alloc")]
    pub fn shrink_to_fit(&mut self) {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            self.as_indexmap_mut().shrink_to_fit();
            return;
        }

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
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            let storage = LargeModeStorage {
                _len_unused: 0,
                cap_sentinel: LARGE_MODE_CAP_SENTINEL,
                map: self.as_indexmap().clone(),
            };
            let boxed = Box::new(storage);
            let ptr = Box::into_raw(boxed);
            return unsafe { Value::new_ptr(ptr.cast(), TypeTag::Object) };
        }

        let mut new = VObject::with_capacity(self.len());
        for kv in self.items() {
            new.insert(kv.key.clone(), kv.value.clone());
        }
        new.0
    }

    pub(crate) fn drop_impl(&mut self) {
        #[cfg(feature = "std")]
        if self.is_large_mode() {
            unsafe {
                drop(Box::from_raw(self.0.heap_ptr_mut() as *mut LargeModeStorage));
            }
            return;
        }

        self.clear();
        unsafe {
            Self::dealloc_ptr(self.0.heap_ptr_mut().cast());
        }
    }
}

// === Iterators ===

enum KeysInner<'a> {
    Small(core::slice::Iter<'a, KeyValuePair>),
    #[cfg(feature = "std")]
    Large(indexmap::map::Keys<'a, VString, Value>),
}

/// Iterator over keys.
pub struct Keys<'a>(KeysInner<'a>);

impl<'a> Iterator for Keys<'a> {
    type Item = &'a VString;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            KeysInner::Small(iter) => iter.next().map(|kv| &kv.key),
            #[cfg(feature = "std")]
            KeysInner::Large(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            KeysInner::Small(iter) => iter.size_hint(),
            #[cfg(feature = "std")]
            KeysInner::Large(iter) => iter.size_hint(),
        }
    }
}

impl ExactSizeIterator for Keys<'_> {}

enum ValuesInner<'a> {
    Small(core::slice::Iter<'a, KeyValuePair>),
    #[cfg(feature = "std")]
    Large(indexmap::map::Values<'a, VString, Value>),
}

/// Iterator over values.
pub struct Values<'a>(ValuesInner<'a>);

impl<'a> Iterator for Values<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            ValuesInner::Small(iter) => iter.next().map(|kv| &kv.value),
            #[cfg(feature = "std")]
            ValuesInner::Large(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            ValuesInner::Small(iter) => iter.size_hint(),
            #[cfg(feature = "std")]
            ValuesInner::Large(iter) => iter.size_hint(),
        }
    }
}

impl ExactSizeIterator for Values<'_> {}

enum ValuesMutInner<'a> {
    Small(core::slice::IterMut<'a, KeyValuePair>),
    #[cfg(feature = "std")]
    Large(indexmap::map::ValuesMut<'a, VString, Value>),
}

/// Iterator over mutable values.
pub struct ValuesMut<'a>(ValuesMutInner<'a>);

impl<'a> Iterator for ValuesMut<'a> {
    type Item = &'a mut Value;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            ValuesMutInner::Small(iter) => iter.next().map(|kv| &mut kv.value),
            #[cfg(feature = "std")]
            ValuesMutInner::Large(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            ValuesMutInner::Small(iter) => iter.size_hint(),
            #[cfg(feature = "std")]
            ValuesMutInner::Large(iter) => iter.size_hint(),
        }
    }
}

impl ExactSizeIterator for ValuesMut<'_> {}

enum IterInner<'a> {
    Small(core::slice::Iter<'a, KeyValuePair>),
    #[cfg(feature = "std")]
    Large(indexmap::map::Iter<'a, VString, Value>),
}

/// Iterator over `(&VString, &Value)` pairs.
pub struct Iter<'a>(IterInner<'a>);

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a VString, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            IterInner::Small(iter) => iter.next().map(|kv| (&kv.key, &kv.value)),
            #[cfg(feature = "std")]
            IterInner::Large(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            IterInner::Small(iter) => iter.size_hint(),
            #[cfg(feature = "std")]
            IterInner::Large(iter) => iter.size_hint(),
        }
    }
}

impl ExactSizeIterator for Iter<'_> {}

enum IterMutInner<'a> {
    Small(core::slice::IterMut<'a, KeyValuePair>),
    #[cfg(feature = "std")]
    Large(indexmap::map::IterMut<'a, VString, Value>),
}

/// Iterator over `(&VString, &mut Value)` pairs.
pub struct IterMut<'a>(IterMutInner<'a>);

impl<'a> Iterator for IterMut<'a> {
    type Item = (&'a VString, &'a mut Value);

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            IterMutInner::Small(iter) => iter.next().map(|kv| (&kv.key, &mut kv.value)),
            #[cfg(feature = "std")]
            IterMutInner::Large(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            IterMutInner::Small(iter) => iter.size_hint(),
            #[cfg(feature = "std")]
            IterMutInner::Large(iter) => iter.size_hint(),
        }
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
    use crate::ValueType;

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

    #[test]
    fn inline_strings_in_objects_remain_inline() {
        let mut obj = VObject::new();
        for idx in 0..=crate::string::VString::INLINE_LEN_MAX.min(5) {
            let key = format!("k{idx}");
            let val = "v".repeat(idx);
            obj.insert(key.as_str(), Value::from(val.as_str()));
        }

        for (key, value) in obj.iter() {
            assert!(
                key.0.is_inline_string(),
                "object key {:?} expected inline storage",
                key.as_str()
            );
            if value.value_type() == ValueType::String {
                assert!(
                    value.is_inline_string(),
                    "object value {value:?} expected inline storage"
                );
            }
        }

        let mut cloned = obj.clone();
        for (key, value) in cloned.iter() {
            assert!(key.0.is_inline_string(), "cloned key lost inline storage");
            if value.value_type() == ValueType::String {
                assert!(value.is_inline_string(), "cloned value lost inline storage");
            }
        }

        let (removed_key, removed_value) = cloned.remove_entry("k1").expect("entry exists");
        assert!(
            removed_key.0.is_inline_string(),
            "removed key should stay inline"
        );
        if removed_value.value_type() == ValueType::String {
            assert!(
                removed_value.is_inline_string(),
                "removed value should stay inline"
            );
        }
    }
}
