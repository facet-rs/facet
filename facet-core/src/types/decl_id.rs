//! Declaration identifier for identifying type declarations independent of type parameters.

use core::fmt::{self, Debug};
use core::hash::{Hash, Hasher};

/// Identifies a type declaration, independent of type parameters.
///
/// Think of `DeclId` as a "type-parameter-erased" version of `Shape::id`. While
/// `Vec<u32>` and `Vec<String>` have different `Shape::id` values, they share
/// the same `DeclId` because they come from the same `Vec<T>` declaration.
///
/// # How DeclId is computed
///
/// ## Non-generic types
///
/// For types without type parameters (like `u32` or `MyStruct`), `DeclId` is
/// trivial—it can simply equal the hash of the type identifier. When implementing
/// `Facet` for such types, you don't need to do anything special; if you don't
/// call `.decl_id()` on the builder, it's automatically computed from the
/// `type_identifier`.
///
/// ## Generic types with `#[derive(Facet)]`
///
/// For generic types using the derive macro, `DeclId` is computed from:
/// ```text
/// file!():line!():column!()#kind#TypeName
/// ```
/// For example: `src/lib.rs:42:1#struct#Wrapper`
///
/// This strategy assumes no two declarations exist at the exact same source
/// location. This isn't strictly true with macros that generate multiple types,
/// which is why we also include the type name. Even so, this can be defeated
/// in edge cases—it's a best-effort approach.
///
/// ## Foreign generic types (manual `Facet` implementations)
///
/// When implementing `Facet` for an external generic type (like `Vec`, `Arc`,
/// `HashMap`), set the `module_path` on the builder:
///
/// ```ignore
/// ShapeBuilder::for_sized::<Arc<T>>("Arc")
///     .module_path("alloc::sync")
///     // ... other fields ...
///     .build()
/// ```
///
/// The `DeclId` is then auto-computed from: `@{module_path}#{kind}#{type_identifier}`
///
/// For example, `Arc` in `alloc::sync` with type `struct` produces:
/// `@alloc::sync#struct#Arc`
///
/// This is also not foolproof—if you have two different versions of the same
/// crate in your dependency graph, they'll produce the same `DeclId` even though
/// they're technically different declarations.
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

// FNV-1a 128-bit constants
const FNV_BASIS_128: u128 = 144066263297769815596495629667062367629;
const FNV_PRIME_128: u128 = 309485009821345068724781371;

/// Computes a `DeclId` hash from module path, kind, and type identifier.
///
/// This produces the same result as hashing `@{module_path}#{kind}#{type_identifier}`.
/// Used internally by `ShapeBuilder::build()` to auto-compute `DeclId` for foreign
/// generic types.
#[inline]
pub const fn decl_id_hash_extern(module_path: &str, kind: &str, type_identifier: &str) -> u128 {
    let mut hash = FNV_BASIS_128;

    // Hash "@"
    hash = (hash ^ b'@' as u128).wrapping_mul(FNV_PRIME_128);

    // Hash module_path
    let bytes = module_path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u128).wrapping_mul(FNV_PRIME_128);
        i += 1;
    }

    // Hash "#"
    hash = (hash ^ b'#' as u128).wrapping_mul(FNV_PRIME_128);

    // Hash kind
    let bytes = kind.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u128).wrapping_mul(FNV_PRIME_128);
        i += 1;
    }

    // Hash "#"
    hash = (hash ^ b'#' as u128).wrapping_mul(FNV_PRIME_128);

    // Hash type_identifier
    let bytes = type_identifier.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u128).wrapping_mul(FNV_PRIME_128);
        i += 1;
    }

    hash
}
