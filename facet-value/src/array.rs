//! Array value type.

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc, realloc};
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};
use core::iter::FromIterator;
use core::ops::{Deref, DerefMut, Index, IndexMut};
use core::slice::SliceIndex;
use core::{cmp, ptr};

use crate::value::{TypeTag, Value};

/// Header for heap-allocated arrays.
#[repr(C, align(8))]
struct ArrayHeader {
    /// Number of elements
    len: usize,
    /// Capacity (number of elements that can be stored)
    cap: usize,
    // Array of Value follows immediately after
}

/// An array value.
///
/// `VArray` is a dynamic array of `Value`s, similar to `Vec<Value>`.
/// The length and capacity are stored in a heap-allocated header.
#[repr(transparent)]
#[derive(Clone)]
pub struct VArray(pub(crate) Value);

impl VArray {
    fn layout(cap: usize) -> Layout {
        Layout::new::<ArrayHeader>()
            .extend(Layout::array::<Value>(cap).unwrap())
            .unwrap()
            .0
            .pad_to_align()
    }

    #[cfg(feature = "alloc")]
    fn alloc(cap: usize) -> *mut ArrayHeader {
        unsafe {
            let layout = Self::layout(cap);
            let ptr = alloc(layout).cast::<ArrayHeader>();
            (*ptr).len = 0;
            (*ptr).cap = cap;
            ptr
        }
    }

    #[cfg(feature = "alloc")]
    fn realloc_ptr(ptr: *mut ArrayHeader, new_cap: usize) -> *mut ArrayHeader {
        unsafe {
            let old_cap = (*ptr).cap;
            let old_layout = Self::layout(old_cap);
            let new_layout = Self::layout(new_cap);
            let new_ptr =
                realloc(ptr.cast::<u8>(), old_layout, new_layout.size()).cast::<ArrayHeader>();
            (*new_ptr).cap = new_cap;
            new_ptr
        }
    }

    #[cfg(feature = "alloc")]
    fn dealloc_ptr(ptr: *mut ArrayHeader) {
        unsafe {
            let cap = (*ptr).cap;
            let layout = Self::layout(cap);
            dealloc(ptr.cast::<u8>(), layout);
        }
    }

    fn header(&self) -> &ArrayHeader {
        unsafe { &*(self.0.heap_ptr() as *const ArrayHeader) }
    }

    fn header_mut(&mut self) -> &mut ArrayHeader {
        unsafe { &mut *(self.0.heap_ptr_mut() as *mut ArrayHeader) }
    }

    fn items_ptr(&self) -> *const Value {
        // Go through heap_ptr directly to avoid creating intermediate reference
        // that would limit provenance to just the header
        unsafe { (self.0.heap_ptr() as *const ArrayHeader).add(1).cast() }
    }

    fn items_ptr_mut(&mut self) -> *mut Value {
        // Use heap_ptr_mut directly to preserve mutable provenance
        unsafe { (self.0.heap_ptr_mut() as *mut ArrayHeader).add(1).cast() }
    }

    /// Creates a new empty array.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a new array with the specified capacity.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        unsafe {
            let ptr = Self::alloc(cap);
            VArray(Value::new_ptr(ptr.cast(), TypeTag::ArrayOrTrue))
        }
    }

    /// Returns the number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.header().len
    }

    /// Returns `true` if the array is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.header().cap
    }

    /// Returns a slice of the elements.
    #[must_use]
    pub fn as_slice(&self) -> &[Value] {
        unsafe { core::slice::from_raw_parts(self.items_ptr(), self.len()) }
    }

    /// Returns a mutable slice of the elements.
    pub fn as_mut_slice(&mut self) -> &mut [Value] {
        unsafe { core::slice::from_raw_parts_mut(self.items_ptr_mut(), self.len()) }
    }

    /// Reserves capacity for at least `additional` more elements.
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

    /// Pushes an element onto the back.
    #[cfg(feature = "alloc")]
    pub fn push(&mut self, value: impl Into<Value>) {
        self.reserve(1);
        unsafe {
            let len = self.header().len;
            let ptr = self.items_ptr_mut().add(len);
            ptr.write(value.into());
            self.header_mut().len = len + 1;
        }
    }

    /// Pops an element from the back.
    pub fn pop(&mut self) -> Option<Value> {
        let len = self.len();
        if len == 0 {
            return None;
        }
        unsafe {
            self.header_mut().len = len - 1;
            let ptr = self.items_ptr_mut().add(len - 1);
            Some(ptr.read())
        }
    }

    /// Inserts an element at the specified index.
    #[cfg(feature = "alloc")]
    pub fn insert(&mut self, index: usize, value: impl Into<Value>) {
        let len = self.len();
        assert!(index <= len, "index out of bounds");

        self.reserve(1);

        unsafe {
            let ptr = self.items_ptr_mut().add(index);
            // Shift elements to the right
            if index < len {
                ptr::copy(ptr, ptr.add(1), len - index);
            }
            ptr.write(value.into());
            self.header_mut().len = len + 1;
        }
    }

    /// Removes and returns the element at the specified index.
    pub fn remove(&mut self, index: usize) -> Option<Value> {
        let len = self.len();
        if index >= len {
            return None;
        }

        unsafe {
            let ptr = self.items_ptr_mut().add(index);
            let value = ptr.read();
            // Shift elements to the left
            if index < len - 1 {
                ptr::copy(ptr.add(1), ptr, len - index - 1);
            }
            self.header_mut().len = len - 1;
            Some(value)
        }
    }

    /// Removes an element by swapping it with the last element.
    /// More efficient than `remove` but doesn't preserve order.
    pub fn swap_remove(&mut self, index: usize) -> Option<Value> {
        let len = self.len();
        if index >= len {
            return None;
        }

        self.as_mut_slice().swap(index, len - 1);
        self.pop()
    }

    /// Clears the array.
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Truncates the array to the specified length.
    pub fn truncate(&mut self, len: usize) {
        while self.len() > len {
            self.pop();
        }
    }

    /// Gets an element by index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.as_slice().get(index)
    }

    /// Gets a mutable element by index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Value> {
        self.as_mut_slice().get_mut(index)
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
        let mut new = VArray::with_capacity(self.len());
        for v in self.as_slice() {
            new.push(v.clone());
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

// === Iterator ===

/// Iterator over owned `Value`s from a `VArray`.
pub struct ArrayIntoIter {
    array: VArray,
}

impl Iterator for ArrayIntoIter {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        if self.array.is_empty() {
            None
        } else {
            self.array.remove(0)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.array.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for ArrayIntoIter {}

impl IntoIterator for VArray {
    type Item = Value;
    type IntoIter = ArrayIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        ArrayIntoIter { array: self }
    }
}

impl<'a> IntoIterator for &'a VArray {
    type Item = &'a Value;
    type IntoIter = core::slice::Iter<'a, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a> IntoIterator for &'a mut VArray {
    type Item = &'a mut Value;
    type IntoIter = core::slice::IterMut<'a, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
    }
}

// === Deref ===

impl Deref for VArray {
    type Target = [Value];

    fn deref(&self) -> &[Value] {
        self.as_slice()
    }
}

impl DerefMut for VArray {
    fn deref_mut(&mut self) -> &mut [Value] {
        self.as_mut_slice()
    }
}

impl Borrow<[Value]> for VArray {
    fn borrow(&self) -> &[Value] {
        self.as_slice()
    }
}

impl BorrowMut<[Value]> for VArray {
    fn borrow_mut(&mut self) -> &mut [Value] {
        self.as_mut_slice()
    }
}

impl AsRef<[Value]> for VArray {
    fn as_ref(&self) -> &[Value] {
        self.as_slice()
    }
}

// === Index ===

impl<I: SliceIndex<[Value]>> Index<I> for VArray {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.as_slice()[index]
    }
}

impl<I: SliceIndex<[Value]>> IndexMut<I> for VArray {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.as_mut_slice()[index]
    }
}

// === Comparison ===

impl PartialEq for VArray {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for VArray {}

impl PartialOrd for VArray {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Element-wise comparison
        let mut iter1 = self.iter();
        let mut iter2 = other.iter();
        loop {
            match (iter1.next(), iter2.next()) {
                (Some(a), Some(b)) => match a.partial_cmp(b) {
                    Some(Ordering::Equal) => continue,
                    other => return other,
                },
                (None, None) => return Some(Ordering::Equal),
                (Some(_), None) => return Some(Ordering::Greater),
                (None, Some(_)) => return Some(Ordering::Less),
            }
        }
    }
}

impl Hash for VArray {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl Debug for VArray {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(self.as_slice(), f)
    }
}

impl Default for VArray {
    fn default() -> Self {
        Self::new()
    }
}

// === FromIterator / Extend ===

#[cfg(feature = "alloc")]
impl<T: Into<Value>> FromIterator<T> for VArray {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let mut array = VArray::with_capacity(lower);
        for v in iter {
            array.push(v);
        }
        array
    }
}

#[cfg(feature = "alloc")]
impl<T: Into<Value>> Extend<T> for VArray {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        self.reserve(lower);
        for v in iter {
            self.push(v);
        }
    }
}

// === From implementations ===

#[cfg(feature = "alloc")]
impl<T: Into<Value>> From<Vec<T>> for VArray {
    fn from(vec: Vec<T>) -> Self {
        vec.into_iter().collect()
    }
}

#[cfg(feature = "alloc")]
impl<T: Into<Value> + Clone> From<&[T]> for VArray {
    fn from(slice: &[T]) -> Self {
        slice.iter().cloned().collect()
    }
}

// === Value conversions ===

impl AsRef<Value> for VArray {
    fn as_ref(&self) -> &Value {
        &self.0
    }
}

impl AsMut<Value> for VArray {
    fn as_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}

impl From<VArray> for Value {
    fn from(arr: VArray) -> Self {
        arr.0
    }
}

impl VArray {
    /// Converts this VArray into a Value, consuming self.
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
        let arr = VArray::new();
        assert!(arr.is_empty());
        assert_eq!(arr.len(), 0);
    }

    #[test]
    fn test_push_pop() {
        let mut arr = VArray::new();
        arr.push(Value::from(1));
        arr.push(Value::from(2));
        arr.push(Value::from(3));

        assert_eq!(arr.len(), 3);
        assert_eq!(arr.pop().unwrap().as_number().unwrap().to_i64(), Some(3));
        assert_eq!(arr.pop().unwrap().as_number().unwrap().to_i64(), Some(2));
        assert_eq!(arr.pop().unwrap().as_number().unwrap().to_i64(), Some(1));
        assert!(arr.pop().is_none());
    }

    #[test]
    fn test_insert_remove() {
        let mut arr = VArray::new();
        arr.push(Value::from(1));
        arr.push(Value::from(3));
        arr.insert(1, Value::from(2));

        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_number().unwrap().to_i64(), Some(1));
        assert_eq!(arr[1].as_number().unwrap().to_i64(), Some(2));
        assert_eq!(arr[2].as_number().unwrap().to_i64(), Some(3));

        let removed = arr.remove(1).unwrap();
        assert_eq!(removed.as_number().unwrap().to_i64(), Some(2));
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_clone() {
        let mut arr = VArray::new();
        arr.push(Value::from("hello"));
        arr.push(Value::from(42));

        let arr2 = arr.clone();
        assert_eq!(arr, arr2);
    }

    #[test]
    fn test_iter() {
        let mut arr = VArray::new();
        arr.push(Value::from(1));
        arr.push(Value::from(2));

        let sum: i64 = arr
            .iter()
            .map(|v| v.as_number().unwrap().to_i64().unwrap())
            .sum();
        assert_eq!(sum, 3);
    }

    #[test]
    fn test_collect() {
        let arr: VArray = vec![1i64, 2, 3].into_iter().map(Value::from).collect();
        assert_eq!(arr.len(), 3);
    }
}
