/// Keeps track of which fields were initialized, up to 64 fields
#[derive(Clone, Copy, Default, Debug)]
pub struct ISet {
    flags: u64,
}

impl ISet {
    /// The maximum number of fields that can be tracked (64).
    pub const MAX_FIELDS: usize = 64;

    /// Creates a new ISet with all bits set except for the lowest `count` bits, which are unset.
    ///
    /// # Panics
    ///
    /// Panics if `count` > MAX_FIELDS.
    #[inline]
    pub fn new(count: usize) -> Self {
        if count > Self::MAX_FIELDS {
            panic!(
                "ISet can only track up to {} fields. Count {count} is out of bounds.",
                Self::MAX_FIELDS
            );
        }
        // For count=MAX_FIELDS, we want flags=0 (all fields unset)
        // For count<MAX_FIELDS, flags = !((1 << count) - 1) sets bits >= count
        let flags = if count == Self::MAX_FIELDS {
            0
        } else {
            !((1u64 << count) - 1)
        };
        Self { flags }
    }

    /// Sets the bit at the given index.
    #[inline]
    pub fn set(&mut self, index: usize) {
        if index >= Self::MAX_FIELDS {
            panic!(
                "ISet can only track up to {} fields. Index {index} is out of bounds.",
                Self::MAX_FIELDS
            );
        }
        self.flags |= 1 << index;
    }

    /// Unsets the bit at the given index.
    #[inline]
    pub fn unset(&mut self, index: usize) {
        if index >= Self::MAX_FIELDS {
            panic!(
                "ISet can only track up to {} fields. Index {index} is out of bounds.",
                Self::MAX_FIELDS
            );
        }
        self.flags &= !(1 << index);
    }

    /// Checks if the bit at the given index is set.
    #[inline]
    pub fn get(&self, index: usize) -> bool {
        if index >= Self::MAX_FIELDS {
            panic!(
                "ISet can only track up to {} fields. Index {index} is out of bounds.",
                Self::MAX_FIELDS
            );
        }
        (self.flags & (1 << index)) != 0
    }

    /// Returns true if all bits up to `count` are set.
    #[inline]
    pub const fn all_set(&self, count: usize) -> bool {
        // Check that the lowest `count` bits are all set
        if count == 0 {
            return true;
        }
        if count >= Self::MAX_FIELDS {
            return self.flags == u64::MAX;
        }
        let mask = (1u64 << count) - 1;
        (self.flags & mask) == mask
    }

    /// Sets all bits up to `count`.
    #[inline]
    pub const fn set_all(&mut self, count: usize) {
        if count >= Self::MAX_FIELDS {
            self.flags = u64::MAX;
        } else {
            self.flags |= (1u64 << count) - 1;
        }
    }
}
