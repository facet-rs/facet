//! Query ingredients (inputs, derived queries, and interning).

mod derived;
mod input;
mod interned;

/// Re-export `Cell` from `derived` as `DerivedCell` to avoid conflicts with `std::cell::Cell`.
/// Use `DerivedCell` as the canonical public name when working with derived query cells.
pub use derived::{Cell as DerivedCell, DerivedIngredient};
pub use input::{InputEntry, InputIngredient};
pub use interned::{InternId, InternedIngredient};
