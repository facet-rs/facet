//! Query ingredients (inputs, derived queries, and interning).

mod derived;
mod input;
mod interned;

pub use derived::DerivedIngredient;
pub use input::InputIngredient;
pub use interned::{InternId, InternedIngredient};
