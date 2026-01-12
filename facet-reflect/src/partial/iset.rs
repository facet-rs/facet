/// Keeps track of which fields were initialized, up to 64 fields
#[derive(Clone, Copy, Default, Debug)]
pub struct ISet {
    flags: u64,
}

impl ISet {
    /// The maximum index that can be tracked.
    pub const MAX_INDEX: usize = 63;

    /// Creates a new ISet with all bits set except for the lowest `count` bits, which are unset.
    ///
    /// # Panics
    ///
    /// Panics if `count` >= 64 (max 63 fields due to shift overflow).
    #[inline]
    pub fn new(count: usize) -> Self {
        if count >= 64 {
            panic!(
                "ISet can only track up to 63 fields. Count {count} is out of bounds (shift overflow at 64)."
            );
        }
        let flags = !((1u64 << count) - 1);
        Self { flags }
    }

    /// Sets the bit at the given index.
    #[inline]
    pub fn set(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.flags |= 1 << index;
    }

    /// Unsets the bit at the given index.
    #[inline]
    pub fn unset(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.flags &= !(1 << index);
    }

    /// Checks if the bit at the given index is set.
    #[inline]
    pub fn get(&self, index: usize) -> bool {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        (self.flags & (1 << index)) != 0
    }

    /// Returns true if all bits up to MAX_INDEX are set.
    #[inline]
    pub const fn all_set(&self) -> bool {
        self.flags == u64::MAX >> (63 - Self::MAX_INDEX)
    }

    /// Sets all bits up to MAX_INDEX.
    #[inline]
    pub const fn set_all(&mut self) {
        self.flags = u64::MAX >> (63 - Self::MAX_INDEX);
    }
}
