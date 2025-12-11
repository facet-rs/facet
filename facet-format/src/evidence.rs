extern crate alloc;

use alloc::borrow::Cow;

use crate::{FieldLocationHint, ValueTypeHint};

/// Evidence describing a serialized field encountered while probing input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEvidence<'de> {
    /// Serialized field name (after rename/namespace resolution).
    pub name: Cow<'de, str>,
    /// Where the field resides (attribute/text/property/etc.).
    pub location: FieldLocationHint,
    /// Optional type hint extracted from the wire (self-describing formats only).
    pub value_type: Option<ValueTypeHint>,
}

impl<'de> FieldEvidence<'de> {
    /// Construct a new evidence entry.
    pub fn new(
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        value_type: Option<ValueTypeHint>,
    ) -> Self {
        Self {
            name: name.into(),
            location,
            value_type,
        }
    }
}
