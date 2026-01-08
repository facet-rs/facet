// r[trace.crate]
// r[trace.no-subscriber]
// r[trace.levels]
#![warn(missing_docs)]
#![doc = include_str!("../../../README.md")]

//! Picante is an async incremental query runtime, inspired by Salsa but designed for Tokio-first
//! pipelines.
//!
//! Picante provides:
//!
//! - Inputs via [`InputIngredient`]
//! - Async derived queries via [`DerivedIngredient`]
//! - Dependency tracking via Tokio task-local frames
//! - Snapshot persistence via [`persist`] (using `facet` + `facet-postcard`, **no serde**)
//!
//! ## Minimal example
//!
//! ```no_run
//! use picante::{DerivedIngredient, DynIngredient, HasRuntime, IngredientLookup, IngredientRegistry, InputIngredient, QueryKindId, Runtime};
//! use std::sync::Arc;
//!
//! #[derive(Default)]
//! struct Db {
//!     runtime: Runtime,
//!     ingredients: IngredientRegistry<Db>,
//! }
//!
//! impl HasRuntime for Db {
//!     fn runtime(&self) -> &Runtime {
//!         &self.runtime
//!     }
//! }
//!
//! impl IngredientLookup for Db {
//!     fn ingredient(&self, kind: QueryKindId) -> Option<&dyn DynIngredient<Self>> {
//!         self.ingredients.ingredient(kind)
//!     }
//! }
//!
//! # fn main() -> picante::PicanteResult<()> {
//! # let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
//! # rt.block_on(async {
//! let db = Db::default();
//!
//! let text: Arc<InputIngredient<String, String>> =
//!     Arc::new(InputIngredient::new(QueryKindId(1), "Text"));
//!
//! let len: Arc<DerivedIngredient<Db, String, u64>> = {
//!     let text = text.clone();
//!     Arc::new(DerivedIngredient::new(QueryKindId(2), "Len", move |db, key| {
//!         let text = text.clone();
//!         Box::pin(async move {
//!             let s = text.get(db, &key)?.unwrap_or_default();
//!             Ok(s.len() as u64)
//!         })
//!     }))
//! };
//!
//! text.set(&db, "a".into(), "hello".into());
//! assert_eq!(len.get(&db, "a".into()).await?, 5);
//! # Ok(()) }) }
//! ```

pub mod db;
pub mod debug;
pub mod error;
mod facet_eq;
pub mod frame;
pub(crate) mod inflight;
pub mod ingredient;
pub mod key;
pub mod persist;
pub mod revision;
pub mod runtime;
pub mod wal;

pub use db::{DynIngredient, IngredientLookup, IngredientRegistry, Touch};
pub use error::{PicanteError, PicanteResult};
pub use ingredient::{DerivedIngredient, InputIngredient, InternId, InternedIngredient};
pub use key::{Dep, DynKey, Key, QueryKindId};
pub use revision::Revision;
pub use runtime::{HasRuntime, Runtime, RuntimeEvent, RuntimeId};

#[cfg(feature = "macros")]
pub use picante_macros::{db, input, interned, tracked};

#[doc(hidden)]
pub fn __test_shared_cache_clear() {
    inflight::__test_shared_cache_clear();
}

#[doc(hidden)]
pub fn __test_shared_cache_set_max_entries(max_entries: usize) {
    inflight::__test_shared_cache_set_max_entries(max_entries);
}
