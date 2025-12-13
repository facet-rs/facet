extern crate alloc;

use alloc::borrow::Cow;

use crate::{FieldLocationHint, ScalarValue, ValueTypeHint};

/// Evidence describing a serialized field encountered while probing input.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldEvidence<'de> {
    /// Serialized field name (after rename/namespace resolution).
    pub name: Cow<'de, str>,
    /// Where the field resides (attribute/text/property/etc.).
    pub location: FieldLocationHint,
    /// Optional type hint extracted from the wire (self-describing formats only).
    pub value_type: Option<ValueTypeHint>,
    /// Optional scalar value captured during probing.
    /// This is used for value-based variant disambiguation (e.g., finding tag values).
    /// Complex values (objects/arrays) are skipped and not captured here.
    pub scalar_value: Option<ScalarValue<'de>>,
    /// Optional namespace URI (for XML namespace support).
    pub namespace: Option<Cow<'de, str>>,
}

impl<'de> FieldEvidence<'de> {
    /// Construct a new evidence entry.
    pub fn new(
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        value_type: Option<ValueTypeHint>,
        namespace: Option<Cow<'de, str>>,
    ) -> Self {
        Self {
            name: name.into(),
            location,
            value_type,
            scalar_value: None,
            namespace,
        }
    }

    /// Construct a new evidence entry with a scalar value.
    pub fn with_scalar_value(
        name: impl Into<Cow<'de, str>>,
        location: FieldLocationHint,
        value_type: Option<ValueTypeHint>,
        scalar_value: ScalarValue<'de>,
        namespace: Option<Cow<'de, str>>,
    ) -> Self {
        Self {
            name: name.into(),
            location,
            value_type,
            scalar_value: Some(scalar_value),
            namespace,
        }
    }
}
