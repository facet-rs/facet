//! Declaration identifier for identifying type declarations independent of type parameters.

use core::fmt::{self, Debug};
use core::hash::{Hash, Hasher};

/// Identifies a type declaration, independent of type parameters.
///
/// Two shapes with the same `DeclId` come from the same source declaration
/// (the same generic type with potentially different type arguments).
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
///
/// #[derive(Facet)]
/// struct Wrapper<T> {
///     inner: T,
/// }
///
/// // Different types (different Shape::id)
/// assert!(<Wrapper<u32>>::SHAPE.id != <Wrapper<String>>::SHAPE.id);
///
/// // Same declaration (same Shape::decl_id)
/// assert!(<Wrapper<u32>>::SHAPE.decl_id == <Wrapper<String>>::SHAPE.decl_id);
/// ```
///
/// # Stability
///
/// **`DeclId` is completely opaque and provides no stability guarantees:**
///
/// - NOT stable across different compilations
/// - NOT stable across refactors (adding a comment changes line numbers)
/// - NOT stable across reformatting (column numbers change)
/// - NOT suitable for persistence or serialization
///
/// The **only** guarantee: within a single compilation, the same declaration
/// produces the same `DeclId`.
///
/// This is sufficient for runtime use cases like "group all instantiations of
/// `Vec<T>` in this program" but NOT for cross-compilation comparisons or
/// persistence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct DeclId(pub u128);

impl DeclId {
    /// Create a `DeclId` from a raw hash value.
    ///
    /// This is typically called by the derive macro using `const_fnv1a_hash`.
    #[inline]
    pub const fn new(hash: u128) -> Self {
        Self(hash)
    }
}

impl Debug for DeclId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show as hex for compactness, but make clear it's opaque
        write!(f, "DeclId({:#034x})", self.0)
    }
}

impl Hash for DeclId {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Computes a `DeclId` hash from a string at compile time.
///
/// This is a const function that can be used to compute declaration IDs
/// in const contexts.
#[inline]
pub const fn decl_id_hash(s: &str) -> u128 {
    const_fnv1a_hash::fnv1a_hash_str_128(s)
}
