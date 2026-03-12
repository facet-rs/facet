extern crate alloc;

use alloc::borrow::Cow;

use crate::{FieldLocationHint, ScalarValue, ValueTypeHint};

/// Evidence describing a serialized field encountered while probing input.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldEvidence<'de> {
    /// Serialized field name (after rename resolution).
    pub name: Cow<'de, str>,
    /// Where the field resides.
    pub location: FieldLocationHint,
    /// Optional type hint extracted from the wire (self-describing formats only).
    pub value_type: Option<ValueTypeHint>,
    /// Optional scalar value captured during probing.
    /// This is used for value-based variant disambiguation (e.g., finding tag values).
    /// Complex values (objects/arrays) are skipped and not captured here.
    pub scalar_value: Option<ScalarValue<'de>>,
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
            scalar_value: None,
        }
    }

    /// Construct a new evidence entry with a scalar value.
    pub fn with_scalar_value(
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        value_type: Option<ValueTypeHint>,
        scalar_value: ScalarValue<'de>,
    ) -> Self {
        Self {
            name: name.into(),
            location,
            value_type,
            scalar_value: Some(scalar_value),
        }
    }
}
