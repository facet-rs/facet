//! Query ingredients (inputs, derived queries, and interning).

mod derived;
mod input;
mod interned;

pub use derived::ErasedReadyRecord;
/// Re-export `Cell` from `derived` as `DerivedCell` to avoid conflicts with `std::cell::Cell`.
/// Use `DerivedCell` as the canonical public name when working with derived query cells.
///
/// Note: as of the type-erased derived-query implementation, derived cells are no longer generic
/// over the stored value type.
pub use derived::{DerivedIngredient, ErasedCell as DerivedCell};
pub use input::{InputEntry, InputIngredient};
pub use interned::{InternId, InternedIngredient};
