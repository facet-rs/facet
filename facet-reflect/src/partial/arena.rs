//! Append-only arena with lightweight indices and unchecked access.

use alloc::vec::Vec;
use core::marker::PhantomData;

/// Index into an arena. 4 bytes.
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

impl<T> core::fmt::Debug for Idx<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Idx({})", self.raw)
    }
}

/// Append-only arena with unchecked access.
#[derive(Debug)]
pub(crate) struct Arena<T> {
    data: Vec<T>,
}

impl<T> Arena<T> {
    /// Create a new empty arena.
    #[inline]
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Add a value and return its index.
    #[inline]
    pub fn alloc(&mut self, value: T) -> Idx<T> {
        let raw = self.data.len() as u32;
        debug_assert!((raw as usize) < u32::MAX as usize, "arena overflow");
        self.data.push(value);
        Idx {
            raw,
            _ty: PhantomData,
        }
    }

    /// Get a reference. No bounds check.
    #[inline]
    pub fn get(&self, idx: Idx<T>) -> &T {
        debug_assert!((idx.raw as usize) < self.data.len());
        // SAFETY: Idx values are only created by alloc(), which guarantees valid indices.
        // The arena is append-only, so indices remain valid forever.
        unsafe { self.data.get_unchecked(idx.raw as usize) }
    }

    /// Get a mutable reference. No bounds check.
    #[inline]
    pub fn get_mut(&mut self, idx: Idx<T>) -> &mut T {
        debug_assert!((idx.raw as usize) < self.data.len());
        unsafe { self.data.get_unchecked_mut(idx.raw as usize) }
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_and_get() {
        let mut arena = Arena::new();
        let a = arena.alloc(42);
        let b = arena.alloc(100);

        assert_eq!(*arena.get(a), 42);
        assert_eq!(*arena.get(b), 100);
    }

    #[test]
    fn test_get_mut() {
        let mut arena = Arena::new();
        let idx = arena.alloc(42);

        *arena.get_mut(idx) = 100;
        assert_eq!(*arena.get(idx), 100);
    }

    #[test]
    fn test_idx_size() {
        assert_eq!(core::mem::size_of::<Idx<String>>(), 4);
    }
}
