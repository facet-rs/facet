#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct MarkerTraits(u8);

impl MarkerTraits {
    /// No marker traits
    pub const EMPTY: Self = Self(0);

    /// Type implements Copy
    pub const COPY: Self = Self(0b0000_0001);
    /// Type implements Send
    pub const SEND: Self = Self(0b0000_0010);
    /// Type implements Sync
    pub const SYNC: Self = Self(0b0000_0100);
    /// Type implements Eq (not just PartialEq)
    pub const EQ: Self = Self(0b0000_1000);
    /// Type implements Unpin
    pub const UNPIN: Self = Self(0b0001_0000);
    /// Type implements UnwindSafe
    pub const UNWIND_SAFE: Self = Self(0b0010_0000);
    /// Type implements RefUnwindSafe
    pub const REF_UNWIND_SAFE: Self = Self(0b0100_0000);

    /// Create marker traits from raw bits
    #[inline]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Get the raw bits
    #[inline]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Check if a marker trait is set
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Set a marker trait
    #[inline]
    pub const fn insert(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Check if Copy is implemented
    #[inline]
    pub const fn is_copy(self) -> bool {
        self.contains(Self::COPY)
    }

    /// Check if Send is implemented
    #[inline]
    pub const fn is_send(self) -> bool {
        self.contains(Self::SEND)
    }

    /// Check if Sync is implemented
    #[inline]
    pub const fn is_sync(self) -> bool {
        self.contains(Self::SYNC)
    }

    /// Check if Eq is implemented
    #[inline]
    pub const fn is_eq(self) -> bool {
        self.contains(Self::EQ)
    }

    /// Check if Unpin is implemented
    #[inline]
    pub const fn is_unpin(self) -> bool {
        self.contains(Self::UNPIN)
    }

    /// Check if UnwindSafe is implemented
    #[inline]
    pub const fn is_unwind_safe(self) -> bool {
        self.contains(Self::UNWIND_SAFE)
    }

    /// Check if RefUnwindSafe is implemented
    #[inline]
    pub const fn is_ref_unwind_safe(self) -> bool {
        self.contains(Self::REF_UNWIND_SAFE)
    }

    /// Add Copy marker
    #[inline]
    pub const fn with_copy(self) -> Self {
        self.insert(Self::COPY)
    }

    /// Add Send marker
    #[inline]
    pub const fn with_send(self) -> Self {
        self.insert(Self::SEND)
    }

    /// Add Sync marker
    #[inline]
    pub const fn with_sync(self) -> Self {
        self.insert(Self::SYNC)
    }

    /// Add Eq marker
    #[inline]
    pub const fn with_eq(self) -> Self {
        self.insert(Self::EQ)
    }

    /// Add Unpin marker
    #[inline]
    pub const fn with_unpin(self) -> Self {
        self.insert(Self::UNPIN)
    }

    /// Add UnwindSafe marker
    #[inline]
    pub const fn with_unwind_safe(self) -> Self {
        self.insert(Self::UNWIND_SAFE)
    }

    /// Add RefUnwindSafe marker
    #[inline]
    pub const fn with_ref_unwind_safe(self) -> Self {
        self.insert(Self::REF_UNWIND_SAFE)
    }
}
