//! Core diff types.
//!
//! These types represent the result of a diff computation and can be
//! traversed/rendered by serializers.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use facet_core::Shape;
use facet_reflect::Peek;

use crate::sequences::Updates;

/// The difference between two values.
///
/// The `from` value does not necessarily have to have the same type as the `to` value.
pub enum Diff<'mem, 'facet> {
    /// The two values are equal
    Equal {
        /// The value (stored for display purposes)
        value: Option<Peek<'mem, 'facet>>,
    },

    /// Fallback case.
    ///
    /// We do not know much about the values, apart from that they are unequal to each other.
    Replace {
        /// The `from` value.
        from: Peek<'mem, 'facet>,

        /// The `to` value.
        to: Peek<'mem, 'facet>,
    },

    /// The two values are both structures or both enums with similar variants.
    User {
        /// The shape of the `from` struct.
        from: &'static Shape,

        /// The shape of the `to` struct.
        to: &'static Shape,

        /// The name of the variant, this is [`None`] if the values are structs
        variant: Option<&'static str>,

        /// The value of the struct/enum variant (tuple or struct fields)
        value: Value<'mem, 'facet>,
    },

    /// A diff between two sequences
    Sequence {
        /// The shape of the `from` sequence.
        from: &'static Shape,

        /// The shape of the `to` sequence.
        to: &'static Shape,

        /// The updates on the sequence
        updates: Updates<'mem, 'facet>,
    },
}

impl<'mem, 'facet> Diff<'mem, 'facet> {
    /// Returns true if the two values were equal
    pub const fn is_equal(&self) -> bool {
        matches!(self, Self::Equal { .. })
    }
}

/// A set of updates, additions, deletions, insertions etc. for a tuple or a struct
pub enum Value<'mem, 'facet> {
    /// A tuple value
    Tuple {
        /// The updates on the sequence
        updates: Updates<'mem, 'facet>,
    },

    /// A struct value
    Struct {
        /// The fields that are updated between the structs
        updates: HashMap<Cow<'static, str>, Diff<'mem, 'facet>>,

        /// The fields that are in `from` but not in `to`.
        deletions: HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,

        /// The fields that are in `to` but not in `from`.
        insertions: HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,

        /// The fields that are unchanged
        unchanged: HashSet<Cow<'static, str>>,
    },
}

impl<'mem, 'facet> Value<'mem, 'facet> {
    /// Returns a measure of how similar the values are (higher = more similar)
    pub fn closeness(&self) -> usize {
        match self {
            Self::Tuple { updates } => updates.closeness(),
            Self::Struct { unchanged, .. } => unchanged.len(),
        }
    }
}
