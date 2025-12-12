#![warn(missing_docs)]

//! Picante: an async incremental query runtime.

pub mod error;
pub mod frame;
pub mod ingredient;
pub mod key;
pub mod persist;
pub mod revision;
pub mod runtime;

pub use error::{PicanteError, PicanteResult};
pub use ingredient::{DerivedIngredient, InputIngredient};
pub use key::{Dep, DynKey, Key, QueryKindId};
pub use revision::Revision;
pub use runtime::{HasRuntime, Runtime};
