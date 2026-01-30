//! Builder API for constructing operations.

use super::{Build, Imm, Op, Path, Source};
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
    /// Set at a single index.
    pub fn at(mut self, index: u32) -> Self {
        self.path.push(index);
        self
    }

    /// Set at a path.
    pub fn at_path(mut self, indices: &[u32]) -> Self {
        for &i in indices {
            self.path.push(i);
        }
        self
    }

    /// Complete with an immediate value
    pub fn imm<'a, 'f, T: Facet<'f>>(self, value: &'a T) -> Op<'a> {
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

    /// Complete with build (push frame).
    pub fn build(self) -> Op<'static> {
        Op::Set {
            dst: self.path,
            src: Source::Build(Build { len_hint: None }),
        }
    }

    /// Complete with build and length hint.
    pub fn build_with_hint(self, hint: usize) -> Op<'static> {
        Op::Set {
            dst: self.path,
            src: Source::Build(Build {
                len_hint: Some(hint),
            }),
        }
    }
}
