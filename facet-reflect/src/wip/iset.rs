use facet_core::Field;

/// Keeps track of which fields were initialized, up to 64 fields
#[derive(Clone, Copy, Default, Debug)]
pub struct ISet {
    flags: u64,
}

impl ISet {
    /// The maximum index that can be tracked.
    pub const MAX_INDEX: usize = 63;

    /// Creates a new ISet with all (given) fields set.
    #[inline]
    pub fn all(fields: &[Field]) -> Self {
        let mut iset = ISet::default();
        for (i, _field) in fields.iter().enumerate() {
            iset.set(i);
        }
        iset
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
    pub fn has(&self, index: usize) -> bool {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        (self.flags & (1 << index)) != 0
    }

    /// Checks if all bits up to the given count are set.
    #[inline]
    pub fn are_all_set(&self, count: usize) -> bool {
        if count > 64 {
            panic!("ISet can only track up to 64 fields. Count {count} is out of bounds.");
        }
        let mask = (1 << count) - 1;
        self.flags & mask == mask
    }

    /// Checks if any bit in the ISet is set.
    #[inline]
    pub fn is_any_set(&self) -> bool {
        self.flags != 0
    }

    /// Clears all bits in the ISet.
    #[inline]
    pub fn clear(&mut self) {
        self.flags = 0;
    }
}
