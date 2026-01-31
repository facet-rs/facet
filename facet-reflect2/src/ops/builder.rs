//! Builder API for constructing operations.

use super::{Build, Imm, Op, Path, Source};
use facet_core::Facet;

/// Builder for Set operations.
pub struct SetBuilder {
    path: Path,
}

/// Builder for Push operations.
pub struct PushBuilder;

/// Builder for Insert operations (maps).
pub struct InsertBuilder<'a> {
    key: Imm<'a>,
}

impl Op<'_> {
    /// Start building a Set operation.
    pub fn set() -> SetBuilder {
        SetBuilder {
            path: Path::default(),
        }
    }

    /// Start building a Push operation.
    pub fn push() -> PushBuilder {
        PushBuilder
    }

    /// Start building an Insert operation with an immediate key.
    pub fn insert<'a, 'f, K: Facet<'f>>(key: &'a mut K) -> InsertBuilder<'a> {
        InsertBuilder {
            key: Imm::from_ref(key),
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

    /// Complete with build (push frame).
    pub fn build(self) -> Op<'static> {
        Op::Set {
            dst: self.path,
            src: Source::Build(Build { len_hint: None }),
        }
    }

    /// Complete with build and length hint.
    pub fn build_with_len_hint(self, hint: usize) -> Op<'static> {
        Op::Set {
            dst: self.path,
            src: Source::Build(Build {
                len_hint: Some(hint),
            }),
        }
    }
}

impl PushBuilder {
    /// Push an immediate value.
    pub fn imm<'a, 'f, T: Facet<'f>>(self, value: &'a mut T) -> Op<'a> {
        Op::Push {
            src: Source::Imm(Imm::from_ref(value)),
        }
    }

    /// Push a default value.
    pub fn default(self) -> Op<'static> {
        Op::Push {
            src: Source::Default,
        }
    }

    /// Push with build (for complex elements).
    pub fn build(self) -> Op<'static> {
        Op::Push {
            src: Source::Build(Build { len_hint: None }),
        }
    }
}

impl<'a> InsertBuilder<'a> {
    /// Insert with an immediate value.
    pub fn imm<'f, V: Facet<'f>>(self, value: &'a mut V) -> Op<'a> {
        Op::Insert {
            key: self.key,
            value: Source::Imm(Imm::from_ref(value)),
        }
    }

    /// Insert with a default value.
    pub fn default(self) -> Op<'a> {
        Op::Insert {
            key: self.key,
            value: Source::Default,
        }
    }

    /// Insert with build (for complex values).
    pub fn build(self) -> Op<'a> {
        Op::Insert {
            key: self.key,
            value: Source::Build(Build { len_hint: None }),
        }
    }
}
