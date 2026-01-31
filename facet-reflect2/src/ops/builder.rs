//! Builder API for constructing operations.

use super::{Imm, Op, Path, Source};
use facet_core::Facet;

/// Builder for Set operations.
pub struct SetBuilder {
    path: Path,
}

impl Op<'_> {
    /// Start building a Set operation.
    pub fn set() -> SetBuilder {
        SetBuilder {
            path: Path::default(),
        }
    }

    /// Create an End operation.
    pub fn end() -> Op<'static> {
        Op::End
    }
}

impl SetBuilder {
    /// Set at a single field index.
    pub fn at(mut self, index: u32) -> Self {
        self.path = self.path.then_field(index);
        self
    }

    /// Set at a path of field indices.
    pub fn at_path(mut self, indices: &[u32]) -> Self {
        for &i in indices {
            self.path = self.path.then_field(i);
        }
        self
    }

    /// Append to a collection (list, set, or map entry).
    pub fn append(mut self) -> Self {
        self.path = self.path.then_append();
        self
    }

    /// Navigate to root first.
    pub fn root(mut self) -> Self {
        self.path = Path::root();
        self
    }

    /// Complete with an immediate value.
    pub fn imm<'a, 'f, T: Facet<'f>>(self, value: &'a mut T) -> Op<'a> {
        Op::Set {
            dst: self.path,
            src: Source::Imm(Imm::from_ref(value)),
        }
    }

    /// Complete with a default value.
    pub fn default(self) -> Op<'static> {
        Op::Set {
            dst: self.path,
            src: Source::Default,
        }
    }

    /// Complete with stage (push frame for incremental construction).
    pub fn stage(self) -> Op<'static> {
        Op::Set {
            dst: self.path,
            src: Source::Stage(None),
        }
    }

    /// Complete with stage and capacity hint.
    pub fn stage_with_capacity(self, hint: usize) -> Op<'static> {
        Op::Set {
            dst: self.path,
            src: Source::Stage(Some(hint)),
        }
    }
}

// --- Deprecated builders for migration ---

/// Builder for Push operations (deprecated: use Set with append).
#[deprecated(note = "Use Op::set().append() instead of Op::set().append()")]
pub struct PushBuilder;

/// Builder for Insert operations (deprecated: use Set with append for map entries).
#[deprecated(note = "Use Op::set().append() to create map entry frames")]
pub struct InsertBuilder<'a> {
    key: Imm<'a>,
}

#[allow(deprecated)]
impl Op<'_> {
    /// Start building a Push operation (deprecated: use set().append()).
    #[deprecated(note = "Use Op::set().append() instead")]
    pub fn push() -> PushBuilder {
        PushBuilder
    }

    /// Start building an Insert operation (deprecated: use set().append() for map entries).
    #[deprecated(note = "Use Op::set().append() for map entries")]
    pub fn insert<'a, 'f, K: Facet<'f>>(key: &'a mut K) -> InsertBuilder<'a> {
        InsertBuilder {
            key: Imm::from_ref(key),
        }
    }
}

#[allow(deprecated)]
impl PushBuilder {
    /// Push an immediate value (deprecated).
    #[deprecated(note = "Use Op::set().append().imm(value) instead")]
    pub fn imm<'a, 'f, T: Facet<'f>>(self, value: &'a mut T) -> Op<'a> {
        Op::Set {
            dst: Path::append(),
            src: Source::Imm(Imm::from_ref(value)),
        }
    }

    /// Push a default value (deprecated).
    #[deprecated(note = "Use Op::set().append().default() instead")]
    pub fn default(self) -> Op<'static> {
        Op::Set {
            dst: Path::append(),
            src: Source::Default,
        }
    }

    /// Push with stage for complex elements (deprecated).
    #[deprecated(note = "Use Op::set().append().stage() instead")]
    pub fn build(self) -> Op<'static> {
        Op::Set {
            dst: Path::append(),
            src: Source::Stage(None),
        }
    }
}

#[allow(deprecated)]
impl<'a> InsertBuilder<'a> {
    /// Get the key for this insert builder.
    pub fn key(&self) -> &Imm<'a> {
        &self.key
    }

    /// Insert with an immediate value (deprecated).
    /// Note: This returns a Set with Append path. The key must be set separately.
    #[deprecated(note = "Use Op::set().append().stage() then set fields 0 (key) and 1 (value)")]
    pub fn imm<'f, V: Facet<'f>>(self, _value: &'a mut V) -> Op<'a> {
        // This is a compatibility shim - the actual implementation
        // will need to use the new multi-step API
        panic!(
            "Insert with Imm is deprecated. Use Op::set().append().stage() then set key/value fields."
        )
    }

    /// Insert with a default value (deprecated).
    #[deprecated(note = "Use Op::set().append().stage() then set fields 0 (key) and 1 (value)")]
    pub fn default(self) -> Op<'a> {
        panic!(
            "Insert with Default is deprecated. Use Op::set().append().stage() then set key/value fields."
        )
    }

    /// Insert with stage for complex values (deprecated).
    #[deprecated(note = "Use Op::set().append().stage() then set fields 0 (key) and 1 (value)")]
    pub fn build(self) -> Op<'a> {
        panic!(
            "Insert with Build is deprecated. Use Op::set().append().stage() then set key/value fields."
        )
    }
}
