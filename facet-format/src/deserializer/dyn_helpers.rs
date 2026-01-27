//! Helper functions for inner deserialization functions using dyn dispatch.
//!
//! These functions are thin wrappers around `FormatDeserializer` methods,
//! providing a convenient API for the inner deserialization functions.

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;

use facet_reflect::Partial;

use crate::{DeserializeError, FieldEvidence, FormatDeserializer, FormatParser, ParseEvent};

/// Type alias for the dyn-dispatched deserializer used by inner functions.
pub type DynDeser<'input, 'p, const BORROW: bool> =
    FormatDeserializer<'input, BORROW, &'p mut dyn FormatParser<'input>>;

/// Read and consume the next event, returning an error if EOF.
#[inline]
pub fn expect_event<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    expected: &'static str,
) -> Result<ParseEvent<'input>, DeserializeError> {
    deser.expect_event(expected)
}

/// Peek at the next event without consuming, returning an error if EOF.
#[inline]
pub fn expect_peek<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    expected: &'static str,
) -> Result<ParseEvent<'input>, DeserializeError> {
    deser.expect_peek(expected)
}

/// Peek at the next event without consuming, returning None at EOF.
#[inline]
pub fn peek_raw<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
) -> Result<Option<ParseEvent<'input>>, DeserializeError> {
    deser.parser.peek_event()
}

/// Skip the current value.
#[inline]
pub fn skip<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
) -> Result<(), DeserializeError> {
    deser.parser.skip_value()
}

/// Recursively deserialize into a Partial.
#[inline]
pub fn deserialize_into<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_into(wip)
}

/// Collect field evidence for internally-tagged enums.
#[inline]
pub fn collect_evidence<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
) -> Result<Vec<FieldEvidence<'input>>, DeserializeError> {
    deser.collect_evidence()
}

/// Set a string value on a Partial.
#[inline]
pub fn set_string_value<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
    value: alloc::borrow::Cow<'input, str>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.set_string_value(wip, value)
}

/// Deserialize variant struct fields.
#[inline]
pub fn deserialize_variant_struct_fields<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_variant_struct_fields(wip)
}

/// Deserialize enum variant content.
#[inline]
pub fn deserialize_enum_variant_content<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_enum_variant_content(wip)
}

/// Deserialize other variant with captured tag.
#[inline]
pub fn deserialize_other_variant_with_captured_tag<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
    captured_tag: Option<&'input str>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_other_variant_with_captured_tag(wip, captured_tag)
}

/// Deserialize a value recursively with a shape hint.
#[inline]
pub fn deserialize_value_recursive<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
    hint_shape: &'static facet_core::Shape,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_value_recursive(wip, hint_shape)
}

/// Solve which variant matches for untagged enums.
#[inline]
pub fn solve_variant<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    shape: &'static facet_core::Shape,
) -> Result<Option<&'static str>, DeserializeError> {
    match crate::solve_variant(shape, &mut deser.parser) {
        Ok(Some(outcome)) => {
            let variant_name = outcome
                .resolution()
                .variant_selections()
                .first()
                .map(|vs| vs.variant_name);
            Ok(variant_name)
        }
        Ok(None) => Ok(None),
        Err(e) => Err(DeserializeError::unsupported(format!(
            "solve_variant failed: {e:?}"
        ))),
    }
}

/// Hint the parser about enum variants.
#[inline]
pub fn hint_enum<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    variants: &[crate::EnumVariantHint],
) {
    deser.parser.hint_enum(variants);
}

/// Deserialize a tuple with dynamic fields.
#[inline]
pub fn deserialize_tuple_dynamic<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
    fields: &'static [facet_core::Field],
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_tuple_dynamic(wip, fields)
}

/// Deserialize a struct with dynamic fields.
#[inline]
pub fn deserialize_struct_dynamic<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
    fields: &'static [facet_core::Field],
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_struct_dynamic(wip, fields)
}

/// Deserialize an enum as a struct (for non-self-describing formats).
#[inline]
pub fn deserialize_enum_as_struct<'input, const BORROW: bool>(
    deser: &mut DynDeser<'input, '_, BORROW>,
    wip: Partial<'input, BORROW>,
    enum_def: &'static facet_core::EnumType,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    deser.deserialize_enum_as_struct(wip, enum_def)
}
