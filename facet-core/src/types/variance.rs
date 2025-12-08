//! Variance types for lifetime and type parameter tracking.
//!
//! Variance describes how a type relates to its type/lifetime parameters
//! with respect to subtyping.
//!
//! See:
//! - [Rust Reference: Subtyping and Variance](https://doc.rust-lang.org/reference/subtyping.html)
//! - [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
//! - [The Rustonomicon: Subtyping and Variance](https://doc.rust-lang.org/nomicon/subtyping.html)

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
    /// Type is covariant: can safely shrink lifetimes (`'static` → `'a`).
    ///
    /// A type `F<T>` is covariant if `F<Sub>` is a subtype of `F<Super>` when `Sub` is a
    /// subtype of `Super`. This means the type "preserves" the subtyping relationship.
    ///
    /// Examples: `&'a T`, `*const T`, `Box<T>`, `Vec<T>`, `[T; N]`
    ///
    /// See [Rust Reference: Covariance](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.covariant)
    Covariant = 0,

    /// Type is contravariant: can safely grow lifetimes (`'a` → `'static`).
    ///
    /// A type `F<T>` is contravariant if `F<Super>` is a subtype of `F<Sub>` when `Sub` is a
    /// subtype of `Super`. This means the type "reverses" the subtyping relationship.
    ///
    /// Examples: `fn(T)` is contravariant in `T`
    ///
    /// See [Rust Reference: Contravariance](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.contravariant)
    Contravariant = 1,

    /// Type is invariant: no lifetime or type parameter changes allowed.
    ///
    /// A type `F<T>` is invariant if neither `F<Sub>` nor `F<Super>` is a subtype of the other,
    /// regardless of the relationship between `Sub` and `Super`.
    ///
    /// Examples: `&'a mut T` (invariant in `T`), `Cell<T>`, `UnsafeCell<T>`, `*mut T`
    ///
    /// See [Rust Reference: Invariance](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.invariant)
    #[default]
    Invariant = 2,
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
    /// Function that returns [`Variance::Covariant`].
    ///
    /// Use this for types that are covariant over their type/lifetime parameter,
    /// such as `&'a T`, `*const T`, `Box<T>`, `Vec<T>`, `[T; N]`.
    ///
    /// Also use for types with **no lifetime parameters** (like `i32`, `String`),
    /// since they impose no constraints on lifetimes.
    ///
    /// See [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
    pub const COVARIANT: fn(&'static Shape) -> Variance = covariant;

    /// Function that returns [`Variance::Contravariant`].
    ///
    /// Use this for types that are contravariant over their type/lifetime parameter,
    /// such as `fn(T)` (contravariant in `T`).
    ///
    /// See [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
    pub const CONTRAVARIANT: fn(&'static Shape) -> Variance = contravariant;

    /// Function that returns [`Variance::Invariant`].
    ///
    /// Use this for types that are invariant over their type/lifetime parameter,
    /// such as `&'a mut T` (invariant in `T`), `*mut T`, `Cell<T>`, `UnsafeCell<T>`.
    ///
    /// This is the **safe default** when variance is unknown.
    ///
    /// See [Rust Reference: Variance of built-in types](https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types)
    pub const INVARIANT: fn(&'static Shape) -> Variance = invariant;

    /// Combine two variances (used when a type contains multiple lifetime-carrying fields).
    ///
    /// The rules follow Rust's variance composition:
    /// - Same variance: keep it
    /// - Mixed covariant/contravariant: becomes invariant
    /// - Invariant dominates everything
    #[inline]
    pub const fn combine(self, other: Variance) -> Variance {
        match (self, other) {
            // Same variance = keep it
            (Variance::Covariant, Variance::Covariant) => Variance::Covariant,
            (Variance::Contravariant, Variance::Contravariant) => Variance::Contravariant,
            (Variance::Invariant, _) | (_, Variance::Invariant) => Variance::Invariant,

            // Mixed covariant/contravariant = invariant
            (Variance::Covariant, Variance::Contravariant)
            | (Variance::Contravariant, Variance::Covariant) => Variance::Invariant,
        }
    }

    /// Flip variance (used when type appears in contravariant position, like fn args).
    ///
    /// - Covariant ↔ Contravariant
    /// - Invariant stays Invariant
    #[inline]
    pub const fn flip(self) -> Variance {
        match self {
            Variance::Covariant => Variance::Contravariant,
            Variance::Contravariant => Variance::Covariant,
            Variance::Invariant => Variance::Invariant,
        }
    }

    /// Returns `true` if lifetimes can be safely shrunk (`'static` → `'a`).
    #[inline]
    pub const fn can_shrink(self) -> bool {
        matches!(self, Variance::Covariant)
    }

    /// Returns `true` if lifetimes can be safely grown (`'a` → `'static`).
    #[inline]
    pub const fn can_grow(self) -> bool {
        matches!(self, Variance::Contravariant)
    }
}
