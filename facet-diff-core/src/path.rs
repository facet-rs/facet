//! Path types for navigating diff trees.

use std::borrow::Cow;

/// A path segment describing how to reach a child.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// A named field in a struct
    Field(Cow<'static, str>),
    /// An index in a list/array
    Index(usize),
    /// A key in a map
    Key(Cow<'static, str>),
    /// An enum variant
    Variant(Cow<'static, str>),
}

/// A path from root to a node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct Path(pub Vec<PathSegment>);

impl Path {
    /// Create a new empty path.
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Append a segment to this path.
    pub fn push(&mut self, segment: PathSegment) {
        self.0.push(segment);
    }

    /// Create a new path with an additional segment.
    pub fn with(&self, segment: PathSegment) -> Self {
        let mut new = self.clone();
        new.push(segment);
        new
    }
}

impl core::fmt::Display for Path {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, segment) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            match segment {
                PathSegment::Field(name) => write!(f, "{}", name)?,
                PathSegment::Index(idx) => write!(f, "[{}]", idx)?,
                PathSegment::Key(key) => write!(f, "[{:?}]", key)?,
                PathSegment::Variant(name) => write!(f, "::{}", name)?,
            }
        }
        Ok(())
    }
}
