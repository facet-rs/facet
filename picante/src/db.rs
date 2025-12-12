//! Database integration traits used by Picante for precise revalidation.

use crate::error::PicanteResult;
use crate::key::{Key, QueryKindId};
use crate::persist::PersistableIngredient;
use crate::revision::Revision;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;

/// Metadata returned from touching a query/input key.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Touch {
    /// The last revision at which the value logically changed.
    pub changed_at: Revision,
}

/// Object-safe operations over ingredients, used to revalidate dependency keys.
pub trait DynIngredient<DB>: PersistableIngredient {
    /// Ensure the `(kind, key)` is valid at `db.runtime().current_revision()` and return metadata.
    fn touch<'a>(&'a self, db: &'a DB, key: Key) -> BoxFuture<'a, PicanteResult<Touch>>;
}

/// A simple registry of ingredients keyed by [`QueryKindId`].
pub struct IngredientRegistry<DB> {
    ingredients: HashMap<QueryKindId, Arc<dyn DynIngredient<DB>>>,
}

impl<DB> Default for IngredientRegistry<DB> {
    fn default() -> Self {
        Self {
            ingredients: HashMap::new(),
        }
    }
}

impl<DB> IngredientRegistry<DB> {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an ingredient (overwrites any previous registration for the same kind id).
    pub fn register<I>(&mut self, ingredient: Arc<I>)
    where
        I: DynIngredient<DB> + 'static,
    {
        let kind = ingredient.kind();
        self.ingredients.insert(kind, ingredient);
    }

    /// Look up a registered ingredient by kind id.
    pub fn ingredient(&self, kind: QueryKindId) -> Option<&dyn DynIngredient<DB>> {
        self.ingredients.get(&kind).map(|i| i.as_ref())
    }

    /// Convenience helper for persistence APIs.
    pub fn persistable_ingredients(&self) -> Vec<&dyn PersistableIngredient> {
        self.ingredients
            .values()
            .map(|i| i.as_ref() as &dyn PersistableIngredient)
            .collect()
    }
}

/// Trait implemented by database types that can map kind ids to ingredient instances.
pub trait IngredientLookup: crate::runtime::HasRuntime + Sized {
    /// Get an ingredient by kind id.
    fn ingredient(&self, kind: QueryKindId) -> Option<&dyn DynIngredient<Self>>;
}
