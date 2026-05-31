//! The codegen input: a set of root Rust types, resolved (via the facet bridge)
//! into one schema registry plus the roots' content-derived ids.
//!
//! Every target language renders from a [`Module`]: the registry gives the type
//! graph, the roots name the entry points.

use std::collections::BTreeMap;

use facet::{Facet, Shape};
use phon::derive::{DeriveError, of_shape};
use phon_schema::{Schema, SchemaId};

/// A root entry type the caller asked to generate.
#[derive(Clone, Debug)]
pub struct Root {
    /// The Rust type identifier (used as the generated type name).
    pub name: String,
    /// The root type's content-derived schema id.
    pub id: SchemaId,
}

/// A resolved set of types to generate from.
#[derive(Clone, Debug, Default)]
pub struct Module {
    /// Every composite schema reachable from the roots, deduped and sorted by id
    /// (primitives are intrinsic and not listed). This is the registry a peer
    /// rebuilds from the emitted schema-bytes.
    pub schemas: Vec<Schema>,
    /// The root entry types, in the order they were added.
    pub roots: Vec<Root>,
}

impl Module {
    /// Resolve a set of root shapes into one module. Schemas reachable from more
    /// than one root are deduped by their content-derived id.
    ///
    /// # Errors
    /// [`DeriveError`] if any shape uses a kind the bridge does not handle.
    pub fn from_shapes(roots: &[&'static Shape]) -> Result<Module, DeriveError> {
        let mut by_id: BTreeMap<u64, Schema> = BTreeMap::new();
        let mut root_list = Vec::with_capacity(roots.len());
        for &shape in roots {
            let derived = of_shape(shape)?;
            for s in derived.schemas {
                by_id.entry(s.id.0).or_insert(s);
            }
            root_list.push(Root {
                name: shape.type_identifier.to_string(),
                id: derived.root,
            });
        }
        Ok(Module {
            schemas: by_id.into_values().collect(),
            roots: root_list,
        })
    }

    /// Look up a composite schema by id (primitives are not stored here).
    #[must_use]
    pub fn schema(&self, id: SchemaId) -> Option<&Schema> {
        self.schemas.iter().find(|s| s.id == id)
    }
}

/// Builder for accumulating root types by their Rust type, sidestepping the
/// `&'static Shape` lifetime dance at the call site.
#[derive(Default)]
pub struct Builder {
    shapes: Vec<&'static Shape>,
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a root type `T`.
    #[must_use]
    pub fn add<'a, T: Facet<'a>>(mut self) -> Self {
        self.shapes.push(T::SHAPE);
        self
    }

    /// Resolve the accumulated roots into a [`Module`].
    ///
    /// # Errors
    /// As [`Module::from_shapes`].
    pub fn build(&self) -> Result<Module, DeriveError> {
        Module::from_shapes(&self.shapes)
    }
}
