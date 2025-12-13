//! Query ingredients (inputs, derived queries, and interning).

mod derived;
mod input;
mod interned;

pub use derived::{Cell as DerivedCell, DerivedIngredient};
pub use input::{InputEntry, InputIngredient};
pub use interned::{InternId, InternedIngredient};
