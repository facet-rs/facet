use alloc::vec::Vec;

/// Keeps track of which fields were initialized.
///
/// For counts up to 64, uses a single `u64` bitmap.
/// For larger counts, uses a `Vec<u64>` dynamically.
#[derive(Clone, Default, Debug)]
pub struct ISet {
    inner: ISetInner,
}

#[derive(Clone, Debug)]
enum ISetInner {
    /// Small case: up to 64 bits in a single u64
    Small(u64),
    /// Large case: multiple u64s for more than 64 bits
    Large(Vec<u64>),
}

impl Default for ISetInner {
    fn default() -> Self {
        ISetInner::Small(0)
    }
}

impl ISet {
    /// The maximum number of fields that can be tracked with a single u64 (64).
    const BITS_PER_WORD: usize = 64;

    /// Creates a new ISet with all bits unset for `count` fields.
    ///
    /// The ISet can track any number of fields. For up to 64 fields,
    /// it uses a single u64. For more fields, it dynamically allocates.
    #[inline]
    pub fn new(count: usize) -> Self {
        if count <= Self::BITS_PER_WORD {
            // Small case: just use a single u64 with all bits unset
            Self {
                inner: ISetInner::Small(0),
            }
        } else {
            // Large case: allocate enough u64s
            let num_words = count.div_ceil(Self::BITS_PER_WORD);
            Self {
                inner: ISetInner::Large(alloc::vec![0; num_words]),
            }
        }
    }

    /// Sets the bit at the given index.
    #[inline]
    pub fn set(&mut self, index: usize) {
        match &mut self.inner {
            ISetInner::Small(flags) => {
                debug_assert!(
                    index < Self::BITS_PER_WORD,
                    "index out of bounds for small ISet"
                );
                *flags |= 1u64 << index;
            }
            ISetInner::Large(words) => {
                let word_idx = index / Self::BITS_PER_WORD;
                let bit_idx = index % Self::BITS_PER_WORD;
                debug_assert!(word_idx < words.len(), "index out of bounds for large ISet");
                words[word_idx] |= 1u64 << bit_idx;
            }
        }
    }

    /// Unsets the bit at the given index.
    #[inline]
    pub fn unset(&mut self, index: usize) {
        match &mut self.inner {
            ISetInner::Small(flags) => {
                debug_assert!(
                    index < Self::BITS_PER_WORD,
                    "index out of bounds for small ISet"
                );
                *flags &= !(1u64 << index);
            }
            ISetInner::Large(words) => {
                let word_idx = index / Self::BITS_PER_WORD;
                let bit_idx = index % Self::BITS_PER_WORD;
                debug_assert!(word_idx < words.len(), "index out of bounds for large ISet");
                words[word_idx] &= !(1u64 << bit_idx);
            }
        }
    }

    /// Checks if the bit at the given index is set.
    #[inline]
    pub fn get(&self, index: usize) -> bool {
        match &self.inner {
            ISetInner::Small(flags) => {
                debug_assert!(
                    index < Self::BITS_PER_WORD,
                    "index out of bounds for small ISet"
                );
                (*flags & (1u64 << index)) != 0
            }
            ISetInner::Large(words) => {
                let word_idx = index / Self::BITS_PER_WORD;
                let bit_idx = index % Self::BITS_PER_WORD;
                debug_assert!(word_idx < words.len(), "index out of bounds for large ISet");
                (words[word_idx] & (1u64 << bit_idx)) != 0
            }
        }
    }

    /// Returns true if all bits up to `count` are set.
    #[inline]
    pub fn all_set(&self, count: usize) -> bool {
        if count == 0 {
            return true;
        }

        match &self.inner {
            ISetInner::Small(flags) => {
                if count >= Self::BITS_PER_WORD {
                    *flags == u64::MAX
                } else {
                    let mask = (1u64 << count) - 1;
                    (*flags & mask) == mask
                }
            }
            ISetInner::Large(words) => {
                let full_words = count / Self::BITS_PER_WORD;
                let remaining_bits = count % Self::BITS_PER_WORD;

                // Check all full words are completely set
                for word in words.iter().take(full_words) {
                    if *word != u64::MAX {
                        return false;
                    }
                }

                // Check remaining bits in the last partial word
                if remaining_bits > 0 && full_words < words.len() {
                    let mask = (1u64 << remaining_bits) - 1;
                    if (words[full_words] & mask) != mask {
                        return false;
                    }
                }

                true
            }
        }
    }

    /// Returns true if no bits up to `count` are set.
    #[inline]
    pub fn none_set(&self, count: usize) -> bool {
        if count == 0 {
            return true;
        }

        match &self.inner {
            ISetInner::Small(flags) => {
                if count >= Self::BITS_PER_WORD {
                    *flags == 0
                } else {
                    let mask = (1u64 << count) - 1;
                    (*flags & mask) == 0
                }
            }
            ISetInner::Large(words) => {
                let full_words = count / Self::BITS_PER_WORD;
                let remaining_bits = count % Self::BITS_PER_WORD;

                // Check all full words are completely unset
                for word in words.iter().take(full_words) {
                    if *word != 0 {
                        return false;
                    }
                }

                // Check remaining bits in the last partial word
                if remaining_bits > 0 && full_words < words.len() {
                    let mask = (1u64 << remaining_bits) - 1;
                    if (words[full_words] & mask) != 0 {
                        return false;
                    }
                }

                true
            }
        }
    }

    /// Sets all bits up to `count`.
    #[inline]
    pub fn set_all(&mut self, count: usize) {
        match &mut self.inner {
            ISetInner::Small(flags) => {
                if count >= Self::BITS_PER_WORD {
                    *flags = u64::MAX;
                } else {
                    *flags |= (1u64 << count) - 1;
                }
            }
            ISetInner::Large(words) => {
                let full_words = count / Self::BITS_PER_WORD;
                let remaining_bits = count % Self::BITS_PER_WORD;

                // Set all bits in full words
                for word in words.iter_mut().take(full_words) {
                    *word = u64::MAX;
                }

                // Set remaining bits in the last partial word
                if remaining_bits > 0 && full_words < words.len() {
                    words[full_words] |= (1u64 << remaining_bits) - 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_iset() {
        let mut iset = ISet::new(10);
        assert!(!iset.get(0));
        assert!(!iset.get(9));

        iset.set(0);
        assert!(iset.get(0));
        assert!(!iset.get(1));

        iset.set(9);
        assert!(iset.get(9));

        iset.unset(0);
        assert!(!iset.get(0));
    }

    #[test]
    fn test_small_all_set() {
        let mut iset = ISet::new(5);
        assert!(!iset.all_set(5));

        for i in 0..5 {
            iset.set(i);
        }
        assert!(iset.all_set(5));
    }

    #[test]
    fn test_large_iset() {
        let mut iset = ISet::new(100);
        assert!(!iset.get(0));
        assert!(!iset.get(99));

        iset.set(0);
        assert!(iset.get(0));
        assert!(!iset.get(1));

        iset.set(99);
        assert!(iset.get(99));
        assert!(!iset.get(64));

        iset.set(64);
        assert!(iset.get(64));

        iset.unset(0);
        assert!(!iset.get(0));
    }

    #[test]
    fn test_large_all_set() {
        let mut iset = ISet::new(100);
        assert!(!iset.all_set(100));

        for i in 0..100 {
            iset.set(i);
        }
        assert!(iset.all_set(100));
    }

    #[test]
    fn test_set_all_small() {
        let mut iset = ISet::new(10);
        iset.set_all(10);
        assert!(iset.all_set(10));
    }

    #[test]
    fn test_set_all_large() {
        let mut iset = ISet::new(100);
        iset.set_all(100);
        assert!(iset.all_set(100));
    }

    #[test]
    fn test_boundary_64() {
        // Test exactly 64 fields (boundary case)
        let mut iset = ISet::new(64);
        for i in 0..64 {
            iset.set(i);
        }
        assert!(iset.all_set(64));
    }

    #[test]
    fn test_boundary_65() {
        // Test 65 fields (just over the small case)
        let mut iset = ISet::new(65);
        for i in 0..65 {
            iset.set(i);
        }
        assert!(iset.all_set(65));
    }
}
