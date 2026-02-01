//! Arena allocation with free list for slot reuse.

use std::marker::PhantomData;

/// A typed index into an arena.
///
/// The phantom type prevents mixing indices from different arenas.
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

impl<T> std::hash::Hash for Idx<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> Idx<T> {
    /// Sentinel: slot not started (reserved, slot 0)
    #[allow(dead_code)]
    pub const NOT_STARTED: Self = Self {
        raw: 0,
        _ty: PhantomData,
    };

    /// Sentinel: slot completed/freed
    pub const COMPLETE: Self = Self {
        raw: u32::MAX,
        _ty: PhantomData,
    };

    #[inline]
    #[allow(dead_code)]
    pub fn is_not_started(self) -> bool {
        self.raw == 0
    }

    #[inline]
    #[allow(dead_code)]
    pub fn is_complete(self) -> bool {
        self.raw == u32::MAX
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.raw != 0 && self.raw != u32::MAX
    }

    #[inline]
    fn index(self) -> usize {
        debug_assert!(self.is_valid(), "cannot get index of sentinel");
        self.raw as usize
    }
}

/// Arena with free list for slot reuse.
///
/// Slot 0 is reserved for the `NOT_STARTED` sentinel.
pub struct Arena<T> {
    slots: Vec<Option<T>>,
    free_list: Vec<u32>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Arena {
            slots: vec![None], // slot 0 reserved
            free_list: Vec::new(),
        }
    }

    pub fn alloc(&mut self, value: T) -> Idx<T> {
        let raw = if let Some(idx) = self.free_list.pop() {
            debug_assert!(self.slots[idx as usize].is_none());
            self.slots[idx as usize] = Some(value);
            idx
        } else {
            let idx = self.slots.len();
            assert!(idx < u32::MAX as usize, "arena full");
            self.slots.push(Some(value));
            idx as u32
        };
        Idx {
            raw,
            _ty: PhantomData,
        }
    }

    pub fn free(&mut self, id: Idx<T>) -> T {
        debug_assert!(id.is_valid());
        let value = self.slots[id.index()].take().expect("double-free");
        self.free_list.push(id.raw);
        value
    }

    pub fn get(&self, id: Idx<T>) -> &T {
        debug_assert!(id.is_valid());
        self.slots[id.index()].as_ref().expect("slot empty")
    }

    pub fn get_mut(&mut self, id: Idx<T>) -> &mut T {
        debug_assert!(id.is_valid());
        self.slots[id.index()].as_mut().expect("slot empty")
    }

    #[allow(dead_code)]
    pub fn live_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn idx_sentinels_are_distinct() {
        // NOT_STARTED and COMPLETE must be different
        let not_started: Idx<u32> = Idx::NOT_STARTED;
        let complete: Idx<u32> = Idx::COMPLETE;

        kani::assert(not_started.raw != complete.raw, "sentinels must differ");
        kani::assert(!not_started.is_valid(), "NOT_STARTED is not valid");
        kani::assert(!complete.is_valid(), "COMPLETE is not valid");
    }

    #[kani::proof]
    fn is_valid_excludes_sentinels() {
        let raw: u32 = kani::any();

        let idx: Idx<u32> = Idx {
            raw,
            _ty: std::marker::PhantomData,
        };

        // is_valid should be true iff raw is neither 0 nor u32::MAX
        let expected_valid = raw != 0 && raw != u32::MAX;
        kani::assert(idx.is_valid() == expected_valid, "is_valid correctness");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idx_sentinels() {
        assert!(Idx::<u32>::NOT_STARTED.is_not_started());
        assert!(!Idx::<u32>::NOT_STARTED.is_complete());
        assert!(!Idx::<u32>::NOT_STARTED.is_valid());

        assert!(!Idx::<u32>::COMPLETE.is_not_started());
        assert!(Idx::<u32>::COMPLETE.is_complete());
        assert!(!Idx::<u32>::COMPLETE.is_valid());
    }

    #[test]
    fn alloc_and_get() {
        let mut arena = Arena::new();
        let id = arena.alloc(42u32);

        assert!(id.is_valid());
        assert_eq!(arena.live_count(), 1);
        assert_eq!(*arena.get(id), 42);
    }

    #[test]
    fn free_and_reuse() {
        let mut arena = Arena::new();

        let id1 = arena.alloc(1u32);
        let _id2 = arena.alloc(2u32);
        assert_eq!(arena.live_count(), 2);

        let val = arena.free(id1);
        assert_eq!(val, 1);
        assert_eq!(arena.live_count(), 1);

        // Next alloc reuses freed slot
        let id3 = arena.alloc(3u32);
        assert_eq!(id3.raw, id1.raw);
        assert_eq!(*arena.get(id3), 3);
    }

    #[test]
    #[should_panic(expected = "double-free")]
    fn double_free_panics() {
        let mut arena = Arena::new();
        let id = arena.alloc(1u32);
        arena.free(id);
        arena.free(id);
    }

    #[test]
    fn get_mut() {
        let mut arena = Arena::new();
        let id = arena.alloc(1u32);

        *arena.get_mut(id) = 99;
        assert_eq!(*arena.get(id), 99);
    }
}
