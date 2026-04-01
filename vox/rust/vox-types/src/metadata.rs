use std::borrow::Cow;

use facet::Facet;

// r[impl rpc.metadata]
// r[impl rpc.metadata.value]
/// Metadata value.
///
/// Uses `Cow` so values can be borrowed (from wire data) or owned (runtime-constructed).
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum MetadataValue<'a> {
    String(Cow<'a, str>) = 0,
    Bytes(Cow<'a, [u8]>) = 1,
    U64(u64) = 2,
}

/// Metadata entry flags.
///
/// Flags control metadata handling behavior.
// r[impl rpc.metadata.flags]
// r[impl rpc.metadata.flags.sensitive]
// r[impl rpc.metadata.flags.no-propagate]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct MetadataFlags(u64);

impl MetadataFlags {
    /// No special handling.
    pub const NONE: Self = Self(0);

    /// Value MUST NOT be logged, traced, or included in error messages.
    pub const SENSITIVE: Self = Self(1 << 0);

    /// Value MUST NOT be forwarded to downstream calls.
    pub const NO_PROPAGATE: Self = Self(1 << 1);

    /// Returns `true` if all flags in `other` are set in `self`.
    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl std::ops::BitOr for MetadataFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for MetadataFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for MetadataFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::BitAndAssign for MetadataFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

// r[impl rpc.metadata.keys]
// r[impl rpc.metadata.duplicates]
/// A single metadata entry with a key, value, and flags.
///
/// Uses `Cow` for the key so entries can be borrowed (from wire data) or owned.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct MetadataEntry<'a> {
    pub key: Cow<'a, str>,
    pub value: MetadataValue<'a>,
    pub flags: MetadataFlags,
}

impl<'a> MetadataValue<'a> {
    /// Convert to a `'static` lifetime by cloning any borrowed data.
    pub fn into_owned(self) -> MetadataValue<'static> {
        match self {
            MetadataValue::String(s) => MetadataValue::String(Cow::Owned(s.into_owned())),
            MetadataValue::Bytes(b) => MetadataValue::Bytes(Cow::Owned(b.into_owned())),
            MetadataValue::U64(n) => MetadataValue::U64(n),
        }
    }
}

impl<'a> MetadataEntry<'a> {
    /// Convert to a `'static` lifetime by cloning any borrowed data.
    pub fn into_owned(self) -> MetadataEntry<'static> {
        MetadataEntry {
            key: Cow::Owned(self.key.into_owned()),
            value: self.value.into_owned(),
            flags: self.flags,
        }
    }
}

// r[impl rpc.metadata.unknown]
/// A list of metadata entries.
pub type Metadata<'a> = Vec<MetadataEntry<'a>>;

/// Convert a `Metadata<'a>` to `Metadata<'static>` by cloning any borrowed data.
pub fn metadata_into_owned(metadata: Metadata<'_>) -> Metadata<'static> {
    metadata
        .into_iter()
        .map(MetadataEntry::into_owned)
        .collect()
}
