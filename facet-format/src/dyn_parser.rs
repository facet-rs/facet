//! Trait-object-safe parser interface.
//!
//! This module provides `DynParser`, a dynamically-dispatched version of `FormatParser`
//! that erases the error type. This enables writing deserializer code that works with
//! `&mut dyn DynParser` instead of being generic over `P: FormatParser`, which avoids
//! monomorphizing the deserializer for each parser type.
//!
//! Tradeoffs:
//! - Dynamic dispatch overhead (likely negligible, not measured)
//! - Parser errors are converted to strings (loses type information)

extern crate alloc;

use alloc::format;
use alloc::string::String;

use crate::{EnumVariantHint, FormatParser, ParseEvent, SavePoint, ScalarTypeHint};
use facet_reflect::Span;

/// Error type for dyn-safe parser operations.
///
/// This erases the parser-specific error type to a string representation,
/// enabling trait object safety.
#[derive(Debug)]
pub struct DynParserError {
    /// Debug representation of the original error.
    pub message: String,
}

impl core::fmt::Display for DynParserError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DynParserError {}

/// Result type for dyn-safe parser operations.
pub type DynResult<T> = Result<T, DynParserError>;

/// Trait-object-safe parser interface.
///
/// This trait mirrors `FormatParser` but with the error type erased to `DynParserError`.
///
/// # Object Safety
///
/// This trait is object-safe because:
/// - The error type is concrete (`DynParserError`)
/// - There are no generic methods
/// - There are no associated types
/// - All methods take `&mut self` or return concrete types
pub trait DynParser<'de> {
    /// Read the next parse event, or `None` if the input is exhausted.
    fn next_event(&mut self) -> DynResult<Option<ParseEvent<'de>>>;

    /// Peek at the next event without consuming it.
    fn peek_event(&mut self) -> DynResult<Option<ParseEvent<'de>>>;

    /// Skip the current value.
    fn skip_value(&mut self) -> DynResult<()>;

    /// Save the current position and start recording events.
    fn save(&mut self) -> SavePoint;

    /// Restore to a save point, replaying recorded events.
    fn restore(&mut self, save_point: SavePoint);

    /// Capture the raw representation of the current value.
    fn capture_raw(&mut self) -> DynResult<Option<&'de str>>;

    /// Returns the shape of the format's raw capture type.
    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape>;

    /// Returns true if this format is self-describing.
    fn is_self_describing(&self) -> bool;

    /// Hint that a struct with the given number of fields is expected.
    fn hint_struct_fields(&mut self, num_fields: usize);

    /// Hint what scalar type is expected next.
    fn hint_scalar_type(&mut self, hint: ScalarTypeHint);

    /// Hint that a sequence is expected.
    fn hint_sequence(&mut self);

    /// Hint that a byte sequence is expected. Returns true if handled.
    fn hint_byte_sequence(&mut self) -> bool;

    /// Hint that a fixed-size array is expected.
    fn hint_array(&mut self, len: usize);

    /// Hint that an Option is expected.
    fn hint_option(&mut self);

    /// Hint that a map is expected.
    fn hint_map(&mut self);

    /// Hint that a dynamic value is expected.
    fn hint_dynamic_value(&mut self);

    /// Hint that an enum is expected.
    fn hint_enum(&mut self, variants: &[EnumVariantHint]);

    /// Hint that an opaque scalar type is expected. Returns true if handled.
    fn hint_opaque_scalar(
        &mut self,
        type_identifier: &'static str,
        shape: &'static facet_core::Shape,
    ) -> bool;

    /// Returns the source span of the most recently consumed event.
    fn current_span(&self) -> Option<Span>;

    /// Returns the format namespace for format-specific proxy resolution.
    fn format_namespace(&self) -> Option<&'static str>;
}

// Blanket impl: any FormatParser can be used as a DynParser directly.
impl<'de, P: FormatParser<'de>> DynParser<'de> for P {
    fn next_event(&mut self) -> DynResult<Option<ParseEvent<'de>>> {
        FormatParser::next_event(self).map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn peek_event(&mut self) -> DynResult<Option<ParseEvent<'de>>> {
        FormatParser::peek_event(self).map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn skip_value(&mut self) -> DynResult<()> {
        FormatParser::skip_value(self).map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn save(&mut self) -> SavePoint {
        FormatParser::save(self)
    }

    fn restore(&mut self, save_point: SavePoint) {
        FormatParser::restore(self, save_point)
    }

    fn capture_raw(&mut self) -> DynResult<Option<&'de str>> {
        FormatParser::capture_raw(self).map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        FormatParser::raw_capture_shape(self)
    }

    fn is_self_describing(&self) -> bool {
        FormatParser::is_self_describing(self)
    }

    fn hint_struct_fields(&mut self, num_fields: usize) {
        FormatParser::hint_struct_fields(self, num_fields);
    }

    fn hint_scalar_type(&mut self, hint: ScalarTypeHint) {
        FormatParser::hint_scalar_type(self, hint);
    }

    fn hint_sequence(&mut self) {
        FormatParser::hint_sequence(self);
    }

    fn hint_byte_sequence(&mut self) -> bool {
        FormatParser::hint_byte_sequence(self)
    }

    fn hint_array(&mut self, len: usize) {
        FormatParser::hint_array(self, len);
    }

    fn hint_option(&mut self) {
        FormatParser::hint_option(self);
    }

    fn hint_map(&mut self) {
        FormatParser::hint_map(self);
    }

    fn hint_dynamic_value(&mut self) {
        FormatParser::hint_dynamic_value(self);
    }

    fn hint_enum(&mut self, variants: &[EnumVariantHint]) {
        FormatParser::hint_enum(self, variants);
    }

    fn hint_opaque_scalar(
        &mut self,
        type_identifier: &'static str,
        shape: &'static facet_core::Shape,
    ) -> bool {
        FormatParser::hint_opaque_scalar(self, type_identifier, shape)
    }

    fn current_span(&self) -> Option<Span> {
        FormatParser::current_span(self)
    }

    fn format_namespace(&self) -> Option<&'static str> {
        FormatParser::format_namespace(self)
    }
}

/// Wrapper that implements `DynParser` for any `FormatParser` (owning version).
///
/// Use this when you need to own the parser. For borrowed access, you can
/// use `&mut parser as &mut dyn DynParser` directly via the blanket impl.
pub struct DynParserWrapper<'de, P: FormatParser<'de>> {
    parser: P,
    _marker: core::marker::PhantomData<&'de ()>,
}

impl<'de, P: FormatParser<'de>> DynParserWrapper<'de, P> {
    /// Create a new wrapper around a concrete parser.
    pub fn new(parser: P) -> Self {
        Self {
            parser,
            _marker: core::marker::PhantomData,
        }
    }

    /// Consume the wrapper and return the inner parser.
    pub fn into_inner(self) -> P {
        self.parser
    }

    /// Get a reference to the inner parser.
    pub fn inner(&self) -> &P {
        &self.parser
    }

    /// Get a mutable reference to the inner parser.
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.parser
    }
}

impl<'de, P: FormatParser<'de>> DynParser<'de> for DynParserWrapper<'de, P> {
    fn next_event(&mut self) -> DynResult<Option<ParseEvent<'de>>> {
        self.parser.next_event().map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn peek_event(&mut self) -> DynResult<Option<ParseEvent<'de>>> {
        self.parser.peek_event().map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn skip_value(&mut self) -> DynResult<()> {
        self.parser.skip_value().map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn save(&mut self) -> SavePoint {
        self.parser.save()
    }

    fn restore(&mut self, save_point: SavePoint) {
        self.parser.restore(save_point)
    }

    fn capture_raw(&mut self) -> DynResult<Option<&'de str>> {
        self.parser.capture_raw().map_err(|e| DynParserError {
            message: format!("{e:?}"),
        })
    }

    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        self.parser.raw_capture_shape()
    }

    fn is_self_describing(&self) -> bool {
        self.parser.is_self_describing()
    }

    fn hint_struct_fields(&mut self, num_fields: usize) {
        self.parser.hint_struct_fields(num_fields);
    }

    fn hint_scalar_type(&mut self, hint: ScalarTypeHint) {
        self.parser.hint_scalar_type(hint);
    }

    fn hint_sequence(&mut self) {
        self.parser.hint_sequence();
    }

    fn hint_byte_sequence(&mut self) -> bool {
        self.parser.hint_byte_sequence()
    }

    fn hint_array(&mut self, len: usize) {
        self.parser.hint_array(len);
    }

    fn hint_option(&mut self) {
        self.parser.hint_option();
    }

    fn hint_map(&mut self) {
        self.parser.hint_map();
    }

    fn hint_dynamic_value(&mut self) {
        self.parser.hint_dynamic_value();
    }

    fn hint_enum(&mut self, variants: &[EnumVariantHint]) {
        self.parser.hint_enum(variants);
    }

    fn hint_opaque_scalar(
        &mut self,
        type_identifier: &'static str,
        shape: &'static facet_core::Shape,
    ) -> bool {
        self.parser.hint_opaque_scalar(type_identifier, shape)
    }

    fn current_span(&self) -> Option<Span> {
        self.parser.current_span()
    }

    fn format_namespace(&self) -> Option<&'static str> {
        self.parser.format_namespace()
    }
}

// Implement FormatParser for dyn DynParser trait objects.
//
// This allows using `&mut dyn DynParser<'de>` wherever `P: FormatParser<'de>` is expected,
// enabling dynamic dispatch through FormatDeserializer without code changes.
impl<'de> FormatParser<'de> for &mut dyn DynParser<'de> {
    type Error = DynParserError;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        DynParser::next_event(*self)
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        DynParser::peek_event(*self)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        DynParser::skip_value(*self)
    }

    fn save(&mut self) -> SavePoint {
        DynParser::save(*self)
    }

    fn restore(&mut self, save_point: SavePoint) {
        DynParser::restore(*self, save_point)
    }

    fn capture_raw(&mut self) -> Result<Option<&'de str>, Self::Error> {
        DynParser::capture_raw(*self)
    }

    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        DynParser::raw_capture_shape(*self)
    }

    fn is_self_describing(&self) -> bool {
        DynParser::is_self_describing(*self)
    }

    fn hint_struct_fields(&mut self, num_fields: usize) {
        DynParser::hint_struct_fields(*self, num_fields);
    }

    fn hint_scalar_type(&mut self, hint: ScalarTypeHint) {
        DynParser::hint_scalar_type(*self, hint);
    }

    fn hint_sequence(&mut self) {
        DynParser::hint_sequence(*self);
    }

    fn hint_byte_sequence(&mut self) -> bool {
        DynParser::hint_byte_sequence(*self)
    }

    fn hint_array(&mut self, len: usize) {
        DynParser::hint_array(*self, len);
    }

    fn hint_option(&mut self) {
        DynParser::hint_option(*self);
    }

    fn hint_map(&mut self) {
        DynParser::hint_map(*self);
    }

    fn hint_dynamic_value(&mut self) {
        DynParser::hint_dynamic_value(*self);
    }

    fn hint_enum(&mut self, variants: &[EnumVariantHint]) {
        DynParser::hint_enum(*self, variants);
    }

    fn hint_opaque_scalar(
        &mut self,
        type_identifier: &'static str,
        shape: &'static facet_core::Shape,
    ) -> bool {
        DynParser::hint_opaque_scalar(*self, type_identifier, shape)
    }

    fn current_span(&self) -> Option<Span> {
        DynParser::current_span(*self)
    }

    fn format_namespace(&self) -> Option<&'static str> {
        DynParser::format_namespace(*self)
    }
}
