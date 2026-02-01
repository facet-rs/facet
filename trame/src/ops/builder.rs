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
