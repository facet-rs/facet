//! Variance types for lifetime and type parameter tracking.
//!
//! Variance describes how a type relates to its type/lifetime parameters
//! with respect to subtyping.
//!
//! ## Vocabulary
//!
//! Throughout this codebase we say:
//!
//! - “covariant / contravariant / invariant **with respect to** `P`”
//!
//! The Rust Reference and Rustonomicon often use equivalent phrasing like:
//!
//! - “...variant **in** `P`”
//! - “...variant **over** `P`”
//!
//! These are the same concept; we standardize on “with respect to” to avoid the
//! “in vs over” bikeshed and to read more clearly in prose.
//!
//! ## Variance Lattice
//!
//! Variance forms a lattice with four elements:
//!
//! ```text
//!        Bivariant (top - no constraints)
//!        /        \
//!   Covariant    Contravariant
//!        \        /
//!        Invariant (bottom - maximum constraints)
//! ```
//!
//! - **Bivariant**: No lifetime constraints (e.g., `i32`, `String`)
//! - **Covariant**: Can shrink lifetimes (e.g., `&'a T`)
//! - **Contravariant**: Can grow lifetimes (e.g., `fn(&'a T)`)
//! - **Invariant**: Maximum constraints (e.g., `*mut T`, `&'a mut &'b T`)
//!
//! See:
//! - [Rust Reference: Subtyping and Variance](https://doc.rust-lang.org/reference/subtyping.html)
//! - [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
//! - [The Rustonomicon: Subtyping and Variance](https://doc.rust-lang.org/nomicon/subtyping.html)
//! - [GitHub Issue #1708](https://github.com/facet-rs/facet/issues/1708) - Bivariance support

use super::Shape;

/// Maximum recursion depth for variance computation to prevent stack overflow
/// on recursive types.
pub const MAX_VARIANCE_DEPTH: usize = 32;

/// Variance of a type with respect to its type/lifetime parameters.
///
/// See:
/// - [Rust Reference: Subtyping and Variance](https://doc.rust-lang.org/reference/subtyping.html)
/// - [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
/// - [The Rustonomicon: Subtyping and Variance](https://doc.rust-lang.org/nomicon/subtyping.html)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Variance {
    /// Type is bivariant: no lifetime constraints at all.
    ///
    /// A type is bivariant if it has no lifetime parameters and contains no
    /// references or interior mutability. Such types can be freely substituted
    /// regardless of lifetime constraints.
    ///
    /// Examples: `i32`, `String`, `bool`, all primitives, types with no lifetime dependency
    ///
    /// Bivariant is the "top" of the variance lattice - it imposes no constraints.
    /// When combined with any other variance, the other variance "wins":
    /// - `Bivariant.combine(X) == X` for any variance X
    ///
    /// When flipped (for contravariant positions like fn arguments):
    /// - `Bivariant.flip() == Bivariant` (no change)
    ///
    /// See [GitHub Issue #1708](https://github.com/facet-rs/facet/issues/1708)
    Bivariant = 0,

    /// Type is covariant: can safely shrink lifetimes (`'static` → `'a`).
    ///
    /// A type `F<T>` is covariant if `F<Sub>` is a subtype of `F<Super>` when `Sub` is a
    /// subtype of `Super`. This means the type "preserves" the subtyping relationship.
    ///
    /// Examples: `&'a T`, `*const T`, `Box<T>`, `Vec<T>`, `[T; N]`
    ///
    /// Note: Prior to issue #1708, types with no lifetime parameters were also
    /// marked as Covariant. Now they should be marked as Bivariant.
    ///
    /// See [Rust Reference: Covariance](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.covariant)
    Covariant = 1,

    /// Type is contravariant: can safely grow lifetimes (`'a` → `'static`).
    ///
    /// A type `F<T>` is contravariant if `F<Super>` is a subtype of `F<Sub>` when `Sub` is a
    /// subtype of `Super`. This means the type "reverses" the subtyping relationship.
    ///
    /// Examples: `fn(T)` is contravariant with respect to `T`
    ///
    /// See [Rust Reference: Contravariance](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.contravariant)
    Contravariant = 2,

    /// Type is invariant: no lifetime or type parameter changes allowed.
    ///
    /// A type `F<T>` is invariant if neither `F<Sub>` nor `F<Super>` is a subtype of the other,
    /// regardless of the relationship between `Sub` and `Super`.
    ///
    /// Examples (overall lifetime variance): `*mut T`, `&'a mut &'b T`
    ///
    /// Note: `&'a mut T` is invariant with respect to `T`, but if `T` contributes `Bivariant`
    /// (no lifetime constraints), the overall lifetime variance is still `Covariant` (from `'a`).
    ///
    /// Invariant is the "bottom" of the variance lattice - it imposes maximum constraints.
    /// When combined with any other variance, Invariant always "wins":
    /// - `X.combine(Invariant) == Invariant` for any variance X
    ///
    /// See [Rust Reference: Invariance](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.invariant)
    #[default]
    Invariant = 3,
}

/// Returns [`Variance::Bivariant`], ignoring the shape parameter.
const fn bivariant(_: &'static Shape) -> Variance {
    Variance::Bivariant
}

/// Returns [`Variance::Covariant`], ignoring the shape parameter.
const fn covariant(_: &'static Shape) -> Variance {
    Variance::Covariant
}

/// Returns [`Variance::Contravariant`], ignoring the shape parameter.
const fn contravariant(_: &'static Shape) -> Variance {
    Variance::Contravariant
}

/// Returns [`Variance::Invariant`], ignoring the shape parameter.
const fn invariant(_: &'static Shape) -> Variance {
    Variance::Invariant
}

impl Variance {
    /// Function that returns [`Variance::Bivariant`].
    ///
    /// Use this for types with **no lifetime parameters** (like `i32`, `String`),
    /// since they impose no constraints on lifetimes.
    ///
    /// Bivariant is the "top" of the variance lattice - when combined with any
    /// other variance, the other variance wins. When flipped, bivariant stays
    /// bivariant.
    ///
    /// See [GitHub Issue #1708](https://github.com/facet-rs/facet/issues/1708)
    pub const BIVARIANT: fn(&'static Shape) -> Variance = bivariant;

    /// Function that returns [`Variance::Covariant`].
    ///
    /// Use this for types that are covariant with respect to their type/lifetime parameter,
    /// such as `&'a T`, `*const T`, `Box<T>`, `Vec<T>`, `[T; N]`.
    ///
    /// Note: For types with **no lifetime parameters** (like `i32`, `String`),
    /// use [`Self::BIVARIANT`] instead, as they impose no constraints on lifetimes.
    ///
    /// See [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
    pub const COVARIANT: fn(&'static Shape) -> Variance = covariant;

    /// Function that returns [`Variance::Contravariant`].
    ///
    /// Use this for types that are contravariant with respect to their type/lifetime parameter,
    /// such as `fn(T)` (contravariant with respect to `T`).
    ///
    /// See [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
    pub const CONTRAVARIANT: fn(&'static Shape) -> Variance = contravariant;

    /// Function that returns [`Variance::Invariant`].
    ///
    /// Use this for types that are invariant with respect to their type/lifetime parameter,
    /// such as `*mut T`, `Cell<T>`, `UnsafeCell<T>`.
    ///
    /// This is the **safe default** when variance is unknown.
    ///
    /// See [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
    pub const INVARIANT: fn(&'static Shape) -> Variance = invariant;

    /// Combine two variances (used when a type contains multiple lifetime-carrying fields).
    ///
    /// This is the "meet" (greatest lower bound) operation in the variance lattice:
    ///
    /// ```text
    ///        Bivariant (top)
    ///        /        \
    ///   Covariant    Contravariant
    ///        \        /
    ///        Invariant (bottom)
    /// ```
    ///
    /// Rules:
    /// - Bivariant is identity: `Bi.combine(X) == X`
    /// - Same variance: keep it
    /// - Mixed covariant/contravariant: becomes invariant
    /// - Invariant dominates everything
    #[inline]
    pub const fn combine(self, other: Variance) -> Variance {
        match (self, other) {
            // Bivariant is the identity element (top of lattice)
            (Variance::Bivariant, x) | (x, Variance::Bivariant) => x,

            // Same variance = keep it
            (Variance::Covariant, Variance::Covariant) => Variance::Covariant,
            (Variance::Contravariant, Variance::Contravariant) => Variance::Contravariant,

            // Invariant dominates everything (bottom of lattice)
            (Variance::Invariant, _) | (_, Variance::Invariant) => Variance::Invariant,

            // Mixed covariant/contravariant = invariant
            (Variance::Covariant, Variance::Contravariant)
            | (Variance::Contravariant, Variance::Covariant) => Variance::Invariant,
        }
    }

    /// Flip variance (used when type appears in contravariant position, like fn args).
    ///
    /// - Bivariant stays Bivariant (no lifetime constraints to flip)
    /// - Covariant ↔ Contravariant
    /// - Invariant stays Invariant
    #[inline]
    pub const fn flip(self) -> Variance {
        match self {
            Variance::Bivariant => Variance::Bivariant,
            Variance::Covariant => Variance::Contravariant,
            Variance::Contravariant => Variance::Covariant,
            Variance::Invariant => Variance::Invariant,
        }
    }

    /// Returns `true` if lifetimes can be safely shrunk (`'static` → `'a`).
    ///
    /// True for Covariant and Bivariant types.
    #[inline]
    pub const fn can_shrink(self) -> bool {
        matches!(self, Variance::Covariant | Variance::Bivariant)
    }

    /// Returns `true` if lifetimes can be safely grown (`'a` → `'static`).
    ///
    /// True for Contravariant and Bivariant types.
    #[inline]
    pub const fn can_grow(self) -> bool {
        matches!(self, Variance::Contravariant | Variance::Bivariant)
    }
}

// =============================================================================
// Declarative Variance Description
// =============================================================================

/// Position of a type dependency in variance computation.
///
/// This determines how the dependency's variance is transformed before combining:
/// - `Covariant`: Use the dependency's variance as-is
/// - `Contravariant`: Flip the dependency's variance before combining
/// - `Invariant`: Converts non-bivariant variance to invariant
///
/// From the [Rust Reference](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types):
/// - `&'a T` is covariant with respect to T → T is in covariant position
/// - `fn(T) -> U` is contravariant with respect to T → T is in contravariant position
/// - `fn(T) -> U` is covariant with respect to U → U is in covariant position
/// - `&'a mut T` is invariant with respect to T → T is in invariant position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VariancePosition {
    /// Dependency's variance is used as-is.
    ///
    /// Examples: T in `&T`, T in `Box<T>`, return type in `fn() -> T`
    Covariant = 0,

    /// Dependency's variance is flipped before combining.
    ///
    /// Examples: argument T in `fn(T)`
    Contravariant = 1,

    /// The type parameter is in invariant position (per Rustonomicon xform rules).
    ///
    /// From the [Rustonomicon](https://doc.rust-lang.org/nomicon/subtyping.html),
    /// `&'a mut T` is covariant with respect to `'a` and invariant with respect to `T`. When we
    /// compute combined variance, any lifetimes inside T are in invariant position.
    ///
    /// Transformation (xform):
    /// - Bivariant → Bivariant (T has no lifetimes, nothing to make invariant)
    /// - Covariant/Contravariant/Invariant → Invariant (lifetimes in T become invariant)
    ///
    /// This means `&'a mut i32` is Covariant (only `'a`, which is covariant),
    /// while `&'a mut &'b U` is Invariant (`'b` is in invariant position).
    ///
    /// Examples: T in `&mut T`, T in `Cell<T>`, T in `UnsafeCell<T>`
    Invariant = 2,
}

/// A dependency for variance computation.
///
/// Represents a type that this type depends on, along with the position
/// (covariant or contravariant) in which it appears.
#[derive(Debug, Clone, Copy)]
pub struct VarianceDep {
    /// The position of this dependency (covariant or contravariant)
    pub position: VariancePosition,
    /// The shape of the dependency
    pub shape: &'static Shape,
}

impl VarianceDep {
    /// Create a new variance dependency in covariant position.
    #[inline]
    pub const fn covariant(shape: &'static Shape) -> Self {
        Self {
            position: VariancePosition::Covariant,
            shape,
        }
    }

    /// Create a new variance dependency in contravariant position.
    #[inline]
    pub const fn contravariant(shape: &'static Shape) -> Self {
        Self {
            position: VariancePosition::Contravariant,
            shape,
        }
    }

    /// Create a new variance dependency in invariant position.
    ///
    /// Use this when the outer type is invariant with respect to this parameter,
    /// but you still want bivariant inner types to contribute nothing
    /// (rather than forcing the whole type to be invariant).
    ///
    /// Example: `&mut T` is invariant with respect to T, but `&mut i32` should be
    /// covariant with respect to its own lifetime when computing variance for some
    /// unrelated parameter (since `i32` contributes `Bivariant`).
    #[inline]
    pub const fn invariant(shape: &'static Shape) -> Self {
        Self {
            position: VariancePosition::Invariant,
            shape,
        }
    }
}

/// Declarative description of a type's variance.
///
/// Instead of using a function that computes variance (which could accidentally
/// create new visited sets and break cycle detection), types declare their
/// variance as data. The central `computed_variance_impl` function interprets
/// this description, ensuring consistent cycle detection across all types.
///
/// ## Structure
///
/// - `base`: The variance this type contributes regardless of its dependencies.
///   For most types this is `Bivariant` (no inherent contribution).
///   For `&T` and `&mut T`, this is `Covariant` (from the lifetime `'a`).
///   For `*mut T`, this is `Invariant` (no lifetime, always invariant with respect to T).
///
/// - `deps`: Type dependencies to combine. Each dependency has a position
///   (covariant, contravariant, or invariant) that determines how its variance
///   is transformed before combining.
///
/// ## Examples
///
/// From the [Rustonomicon](https://doc.rust-lang.org/nomicon/subtyping.html):
///
/// ```text
/// &'a T:        base=Covariant,  deps=[(Covariant, T)]
///               // covariant with respect to 'a, covariant with respect to T
/// &'a mut T:    base=Covariant,  deps=[(Invariant, T)]
///               // covariant with respect to 'a, invariant with respect to T
/// *const T:     base=Bivariant,  deps=[(Covariant, T)]
///               // covariant with respect to T
/// *mut T:       base=Invariant,  deps=[]
///               // invariant with respect to T (no lifetime to be covariant with respect to)
/// Box<T>:       base=Bivariant,  deps=[(Covariant, T)]
/// fn(A) -> R:   base=Bivariant,  deps=[(Contravariant, A), (Covariant, R)]
/// struct {x,y}: base=Bivariant,  deps=[(Covariant, x), (Covariant, y)]
/// ```
///
/// Note: an `(Invariant, shape)` dependency doesn't automatically force the
/// whole type to be invariant. If `shape`'s computed variance is `Bivariant`
/// (it doesn't mention the parameter being analyzed), it contributes nothing.
///
/// ## Computation
///
/// The final variance is computed as:
/// 1. Start with `base`
/// 2. For each `(position, shape)` in `deps`:
///    - Get `shape`'s variance (recursively, with cycle detection)
///    - If position is Contravariant, flip the variance
///    - Combine with the running total
/// 3. Return the result
#[derive(Debug, Clone, Copy)]
pub struct VarianceDesc {
    /// The base variance this type contributes.
    ///
    /// - `Bivariant` for types that only derive variance from dependencies
    /// - `Covariant` for types with an inherent lifetime (like `&'a T`, `&'a mut T`)
    /// - `Invariant` for types that are always invariant (like `*mut T`)
    pub base: Variance,

    /// Dependencies whose variances are combined to produce the final variance.
    ///
    /// Empty for types with constant variance (like `*mut T` which is always Invariant).
    /// For `&'a mut T`, contains T in Invariant position.
    pub deps: &'static [VarianceDep],
}

impl VarianceDesc {
    /// Always bivariant — no lifetime parameters.
    ///
    /// Examples: `i32`, `String`, `bool`, `()`.
    pub const BIVARIANT: Self = Self {
        base: Variance::Bivariant,
        deps: &[],
    };

    /// Always invariant — no lifetime, invariant with respect to type parameter.
    ///
    /// Examples: `*mut T`.
    pub const INVARIANT: Self = Self {
        base: Variance::Invariant,
        deps: &[],
    };

    /// Create a variance description with the given base and dependencies.
    #[inline]
    pub const fn new(base: Variance, deps: &'static [VarianceDep]) -> Self {
        Self { base, deps }
    }
}
