//! Arena-based allocation with typed 32-bit indices.
//!
//! This module provides arena infrastructure for the TypePlan system, replacing
//! bumpalo's 64-bit pointer references with 32-bit indices for better memory efficiency.
//!
//! # Memory Savings
//!
//! | Type | Before (bumpalo) | After (arena) |
//! |------|------------------|---------------|
//! | Node reference | 8 bytes | 4 bytes |
//! | Slice reference | 16 bytes | 8 bytes |

use alloc::vec::Vec;
use core::marker::PhantomData;
use core::ops::Index;

/// A typed 32-bit index into an arena.
///
/// This is a newtype wrapper around `u32` that carries phantom type information
/// to prevent accidentally mixing indices from different arenas.
#[derive(Debug)]
pub struct Idx<T> {
    raw: u32,
    _ty: PhantomData<fn() -> T>,
}

impl<T> Clone for Idx<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Idx<T> {}

impl<T> PartialEq for Idx<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Idx<T> {}

impl<T> core::hash::Hash for Idx<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> Idx<T> {
    /// Create a new index from a raw u32 value.
    ///
    /// # Safety
    /// The caller must ensure this index is valid for the corresponding arena.
    #[inline]
    pub const fn new(raw: u32) -> Self {
        Self {
            raw,
            _ty: PhantomData,
        }
    }

    /// Get the raw index value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.raw
    }

    /// Create a sentinel/invalid index.
    /// Used for nodes that should never be accessed (e.g., BackRef targets).
    #[inline]
    pub const fn invalid() -> Self {
        Self {
            raw: u32::MAX,
            _ty: PhantomData,
        }
    }

    /// Check if this is a valid index (not the sentinel value).
    #[inline]
    pub const fn is_valid(self) -> bool {
        self.raw != u32::MAX
    }
}

/// A range into an arena slice, storing start index and length.
///
/// This is 8 bytes total (two u32s) vs 16 bytes for a fat pointer slice reference.
#[derive(Debug)]
pub struct SliceRange<T> {
    start: u32,
    len: u32,
    _ty: PhantomData<fn() -> T>,
}

impl<T> Clone for SliceRange<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SliceRange<T> {}

impl<T> PartialEq for SliceRange<T> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.len == other.len
    }
}

impl<T> Eq for SliceRange<T> {}

impl<T> SliceRange<T> {
    /// Create a new slice range.
    #[inline]
    pub const fn new(start: u32, len: u32) -> Self {
        Self {
            start,
            len,
            _ty: PhantomData,
        }
    }

    /// Create an empty slice range.
    #[inline]
    pub const fn empty() -> Self {
        Self::new(0, 0)
    }

    /// Get the start index.
    #[inline]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Get the length.
    #[inline]
    pub const fn len(self) -> u32 {
        self.len
    }

    /// Check if empty.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    /// Get an index at offset `n` within this range.
    /// Returns None if out of bounds.
    #[inline]
    pub const fn get(self, n: usize) -> Option<Idx<T>> {
        if n < self.len as usize {
            Some(Idx::new(self.start + n as u32))
        } else {
            None
        }
    }
}

/// A simple arena that stores values in a Vec and returns typed indices.
///
/// This provides the same memory locality benefits as bump allocation
/// while using 32-bit indices instead of 64-bit pointers.
#[derive(Debug)]
pub struct Arena<T> {
    data: Vec<T>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    /// Create a new empty arena.
    #[inline]
    pub const fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Create an arena with pre-allocated capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Allocate a single value and return its index.
    ///
    /// # Panics
    /// Panics if the arena has more than `u32::MAX - 1` elements.
    #[inline]
    pub fn alloc(&mut self, value: T) -> Idx<T> {
        let idx = self.data.len();
        assert!(idx < u32::MAX as usize, "arena overflow");
        self.data.push(value);
        Idx::new(idx as u32)
    }

    /// Allocate multiple values from an iterator and return a range.
    ///
    /// # Panics
    /// Panics if the arena would overflow u32::MAX elements.
    #[inline]
    pub fn alloc_extend(&mut self, values: impl IntoIterator<Item = T>) -> SliceRange<T> {
        let start = self.data.len() as u32;
        self.data.extend(values);
        let end = self.data.len() as u32;
        SliceRange::new(start, end - start)
    }

    /// Get a reference to a value by index.
    ///
    /// In debug builds, this panics on invalid index.
    /// In release builds, this uses unchecked access for performance.
    #[inline]
    pub fn get(&self, idx: Idx<T>) -> &T {
        debug_assert!(
            (idx.raw as usize) < self.data.len(),
            "arena index out of bounds"
        );
        // SAFETY: In debug mode we assert. In release mode, the TypePlan builder
        // guarantees valid indices are always used.
        unsafe { self.data.get_unchecked(idx.raw as usize) }
    }

    /// Get a mutable reference to a value by index.
    #[inline]
    pub fn get_mut(&mut self, idx: Idx<T>) -> &mut T {
        debug_assert!(
            (idx.raw as usize) < self.data.len(),
            "arena index out of bounds"
        );
        unsafe { self.data.get_unchecked_mut(idx.raw as usize) }
    }

    /// Get a slice from a range.
    ///
    /// In debug builds, this panics on invalid range.
    /// In release builds, this uses unchecked access for performance.
    #[inline]
    pub fn get_slice(&self, range: SliceRange<T>) -> &[T] {
        let start = range.start as usize;
        let end = start + range.len as usize;
        debug_assert!(end <= self.data.len(), "arena slice out of bounds");
        // SAFETY: Same as get() - builder guarantees valid ranges.
        unsafe { self.data.get_unchecked(start..end) }
    }

    /// Get the number of elements in the arena.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the arena is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Iterate over all elements.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }
}

impl<T> Index<Idx<T>> for Arena<T> {
    type Output = T;

    #[inline]
    fn index(&self, idx: Idx<T>) -> &Self::Output {
        self.get(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_basic() {
        let mut arena: Arena<i32> = Arena::new();
        let idx1 = arena.alloc(42);
        let idx2 = arena.alloc(100);

        assert_eq!(*arena.get(idx1), 42);
        assert_eq!(*arena.get(idx2), 100);
    }

    #[test]
    fn test_arena_extend() {
        let mut arena: Arena<i32> = Arena::new();
        let range = arena.alloc_extend([1, 2, 3, 4, 5]);

        assert_eq!(range.len(), 5);
        let slice = arena.get_slice(range);
        assert_eq!(slice, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_idx_equality() {
        let idx1: Idx<i32> = Idx::new(5);
        let idx2: Idx<i32> = Idx::new(5);
        let idx3: Idx<i32> = Idx::new(10);

        assert_eq!(idx1, idx2);
        assert_ne!(idx1, idx3);
    }

    #[test]
    fn test_slice_range_get() {
        let range: SliceRange<i32> = SliceRange::new(10, 5);
        assert_eq!(range.get(0).map(|i| i.raw()), Some(10));
        assert_eq!(range.get(4).map(|i| i.raw()), Some(14));
        assert!(range.get(5).is_none());
    }

    #[test]
    fn test_invalid_index() {
        let invalid: Idx<i32> = Idx::invalid();
        assert!(!invalid.is_valid());

        let valid: Idx<i32> = Idx::new(0);
        assert!(valid.is_valid());
    }
}
