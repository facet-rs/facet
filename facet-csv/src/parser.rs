//! CSV parser implementation using FormatParser trait.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use facet_format::{
    ContainerKind, DeserializeErrorKind, FormatParser, ParseError, ParseEvent, SavePoint,
    ScalarTypeHint, ScalarValue,
};
use facet_reflect::Span;

/// Parser state for CSV.
#[derive(Debug, Clone)]
enum ParserState {
    /// Ready to start parsing.
    Ready,
    /// Inside a struct, tracking remaining fields.
    InStruct { remaining_fields: usize },
}

/// A parsed field with its byte offset and length.
#[derive(Debug, Clone, Copy)]
struct FieldSpan<'de> {
    value: &'de str,
    offset: usize,
    len: usize,
}

/// CSV parser that emits FormatParser events.
///
/// CSV is parsed as a struct where each comma-separated field corresponds
/// to a struct field in definition order. The format does not support
/// nested structures or arrays.
///
/// Unlike fully self-describing formats (JSON), CSV is positional:
/// - Fields are identified by column order, not names
/// - The parser uses `hint_struct_fields` to know how many fields to expect
/// - Each field emits an `OrderedField` event followed by a `Scalar` value
pub struct CsvParser<'de> {
    input: &'de str,
    fields: Vec<FieldSpan<'de>>,
    field_index: usize,
    state_stack: Vec<ParserState>,
    peeked: Option<ParseEvent<'de>>,
    /// Pending struct field count from `hint_struct_fields`.
    pending_struct_fields: Option<usize>,
    /// Pending scalar type hint from `hint_scalar_type`.
    pending_scalar_type: Option<ScalarTypeHint>,
}

impl<'de> CsvParser<'de> {
    /// Create a new CSV parser for a single row.
    pub fn new(input: &'de str) -> Self {
        let trimmed = input.trim();
        // Calculate the offset of the trimmed content within the original input
        let trim_offset = input.len() - input.trim_start().len();
        let fields = if trimmed.is_empty() {
            Vec::new()
        } else {
            parse_csv_row_with_spans(trimmed, trim_offset)
        };

        Self {
            input,
            fields,
            field_index: 0,
            state_stack: Vec::new(),
            peeked: None,
            pending_struct_fields: None,
            pending_scalar_type: None,
        }
    }

    /// Get the current parser state.
    fn current_state(&self) -> &ParserState {
        self.state_stack.last().unwrap_or(&ParserState::Ready)
    }

    /// Get the span for the current field, or EOF span if past the end.
    fn current_field_span(&self) -> Span {
        if self.field_index > 0 && self.field_index <= self.fields.len() {
            let field = &self.fields[self.field_index - 1];
            Span::new(field.offset, field.len)
        } else {
            // EOF span
            Span::new(self.input.len(), 0)
        }
    }

    /// Generate the next event based on current state.
    fn generate_next_event(&mut self) -> Result<ParseEvent<'de>, ParseError> {
        // Check if we have a pending scalar type hint
        if let Some(hint) = self.pending_scalar_type.take() {
            if self.field_index > 0 && self.field_index <= self.fields.len() {
                let field = &self.fields[self.field_index - 1];
                return Ok(ParseEvent::Scalar(parse_scalar_with_hint(field.value, hint)));
            } else {
                return Err(ParseError::new(
                    Span::new(self.input.len(), 0),
                    DeserializeErrorKind::UnexpectedEof {
                        expected: "field for scalar hint",
                    },
                ));
            }
        }

        // Check if we have a pending struct hint
        if let Some(num_fields) = self.pending_struct_fields.take() {
            self.state_stack.push(ParserState::InStruct {
                remaining_fields: num_fields,
            });
            return Ok(ParseEvent::StructStart(ContainerKind::Object));
        }

        // Process based on current state
        match self.current_state().clone() {
            ParserState::Ready => {
                // Without a hint, we can't know how many fields to expect
                // Return an error - the driver should call hint_struct_fields first
                Err(ParseError::new(
                    Span::new(0, self.input.len()),
                    DeserializeErrorKind::InvalidValue {
                        message: "CSV parser requires hint_struct_fields to know field count"
                            .into(),
                    },
                ))
            }
            ParserState::InStruct { remaining_fields } => {
                if remaining_fields == 0 {
                    // Struct complete
                    self.state_stack.pop();
                    Ok(ParseEvent::StructEnd)
                } else {
                    // More fields to go - emit OrderedField and decrement
                    if let Some(ParserState::InStruct { remaining_fields }) =
                        self.state_stack.last_mut()
                    {
                        *remaining_fields -= 1;
                    }
                    // Advance field index when emitting OrderedField
                    self.field_index += 1;
                    Ok(ParseEvent::OrderedField)
                }
            }
        }
    }
}

/// Parse a CSV row into fields with spans, handling quoted fields.
fn parse_csv_row_with_spans(input: &str, base_offset: usize) -> Vec<FieldSpan<'_>> {
    let mut fields = Vec::new();
    let mut in_quotes = false;
    let mut field_start = 0;
    let bytes = input.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => {
                in_quotes = !in_quotes;
            }
            b',' if !in_quotes => {
                let field = &input[field_start..i];
                let (value, value_offset) = unquote_field_with_offset(field, field_start);
                fields.push(FieldSpan {
                    value,
                    offset: base_offset + value_offset,
                    len: value.len(),
                });
                field_start = i + 1;
            }
            _ => {}
        }
    }

    // Add the last field
    let field = &input[field_start..];
    let (value, value_offset) = unquote_field_with_offset(field, field_start);
    fields.push(FieldSpan {
        value,
        offset: base_offset + value_offset,
        len: value.len(),
    });

    fields
}

/// Remove surrounding quotes from a field if present, returning value and offset.
fn unquote_field_with_offset(field: &str, field_start: usize) -> (&str, usize) {
    let trim_start = field.len() - field.trim_start().len();
    let trimmed = field.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        // +1 for the opening quote
        (&trimmed[1..trimmed.len() - 1], field_start + trim_start + 1)
    } else {
        (trimmed, field_start + trim_start)
    }
}

/// Parse a scalar value with the given type hint.
fn parse_scalar_with_hint(value: &str, hint: ScalarTypeHint) -> ScalarValue<'_> {
    match hint {
        ScalarTypeHint::Bool => {
            let val = matches!(value, "true" | "TRUE" | "1" | "yes" | "YES");
            ScalarValue::Bool(val)
        }
        ScalarTypeHint::U8
        | ScalarTypeHint::U16
        | ScalarTypeHint::U32
        | ScalarTypeHint::U64
        | ScalarTypeHint::Usize => {
            if let Ok(n) = value.parse::<u64>() {
                ScalarValue::U64(n)
            } else {
                // Fall back to string if parsing fails
                ScalarValue::Str(Cow::Borrowed(value))
            }
        }
        ScalarTypeHint::U128 => {
            if let Ok(n) = value.parse::<u128>() {
                ScalarValue::U128(n)
            } else {
                ScalarValue::Str(Cow::Borrowed(value))
            }
        }
        ScalarTypeHint::I8
        | ScalarTypeHint::I16
        | ScalarTypeHint::I32
        | ScalarTypeHint::I64
        | ScalarTypeHint::Isize => {
            if let Ok(n) = value.parse::<i64>() {
                ScalarValue::I64(n)
            } else {
                ScalarValue::Str(Cow::Borrowed(value))
            }
        }
        ScalarTypeHint::I128 => {
            if let Ok(n) = value.parse::<i128>() {
                ScalarValue::I128(n)
            } else {
                ScalarValue::Str(Cow::Borrowed(value))
            }
        }
        ScalarTypeHint::F32 | ScalarTypeHint::F64 => {
            if let Ok(n) = value.parse::<f64>() {
                ScalarValue::F64(n)
            } else {
                ScalarValue::Str(Cow::Borrowed(value))
            }
        }
        ScalarTypeHint::String | ScalarTypeHint::Char => ScalarValue::Str(Cow::Borrowed(value)),
        ScalarTypeHint::Bytes => {
            // Bytes in CSV are typically base64 or hex encoded
            // For now, just return as string and let the deserializer handle it
            ScalarValue::Str(Cow::Borrowed(value))
        }
    }
}

impl<'de> FormatParser<'de> for CsvParser<'de> {
    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        // Return peeked event if available
        if let Some(event) = self.peeked.take() {
            return Ok(Some(event));
        }
        Ok(Some(self.generate_next_event()?))
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.generate_next_event()?);
        }
        Ok(self.peeked.clone())
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        // Skip the current field by advancing index
        if self.field_index < self.fields.len() {
            self.field_index += 1;
        }
        Ok(())
    }

    fn save(&mut self) -> SavePoint {
        // CSV is positional - save/restore not meaningful
        unimplemented!("save/restore not supported for CSV (positional format)")
    }

    fn restore(&mut self, _save_point: SavePoint) {
        unimplemented!("save/restore not supported for CSV (positional format)")
    }

    fn is_self_describing(&self) -> bool {
        // CSV is NOT self-describing in the facet-format sense:
        // - It doesn't have field names in the data
        // - It relies on position/order for field identification
        // This tells the deserializer to use hint_struct_fields/hint_scalar_type
        false
    }

    fn hint_struct_fields(&mut self, num_fields: usize) {
        self.pending_struct_fields = Some(num_fields);
        // Clear any peeked OrderedField placeholder
        if matches!(self.peeked, Some(ParseEvent::OrderedField)) {
            self.peeked = None;
        }
    }

    fn hint_scalar_type(&mut self, hint: ScalarTypeHint) {
        self.pending_scalar_type = Some(hint);
        // Clear any peeked OrderedField placeholder
        if matches!(self.peeked, Some(ParseEvent::OrderedField)) {
            self.peeked = None;
        }
    }

    fn current_span(&self) -> Option<Span> {
        Some(self.current_field_span())
    }
}
