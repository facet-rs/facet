use alloc::string::{String, ToString};
use alloc::vec::Vec;
use facet_core::Facet;
use facet_deserialize_externally_driven::DeserializeError;
use facet_reflect::{HeapValue, Wip};
use log::trace;
use owo_colors::OwoColorize;

mod tokenizer;
pub use tokenizer::*;

mod error;
pub use error::*;

/// Deserializes a JSON string into a value of type `T` that implements `Facet`.
///
/// This function takes a JSON string representation and converts it into a Rust
/// value of the specified type `T`. The type must implement the `Facet` trait
/// to provide the necessary type information for deserialization.
pub fn from_str<'input, 'facet, T>(json: &'input str) -> Result<T, JsonError<'input>>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    from_slice(json.as_bytes())
}

/// Deserialize JSON from a slice
///
/// # Arguments
///
/// * `json` - A slice of bytes representing the JSON input.
///
/// # Returns
///
/// A result containing the deserialized value of type `T` or a `JsonParseErrorWithContext`.
pub fn from_slice<'input, 'facet, T>(json: &'input [u8]) -> Result<T, JsonError<'input>>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    let wip =
        Wip::alloc::<T>().map_err(|e| JsonError::new_reflect(e, json, Span::new(0, json.len())))?;
    let heap_value = from_slice_wip::<T>(wip, json)?;
    trace!("Materializing final result");
    Ok(heap_value.materialize().unwrap())
}

/// Deserialize a JSON string into a Wip object.
///
/// # Arguments
///
/// * `wip` - A mutable Wip object to deserialize into.
/// * `input` - A byte slice representing the JSON input.
///
/// # Returns
///
/// A result containing the updated `Wip` or a `JsonParseErrorWithContext`.
pub fn from_slice_wip<'input: 'facet, 'facet, T>(
    wip: Wip<'facet>,
    input: &'input [u8],
) -> Result<HeapValue<'facet>, JsonError<'input>>
where
    T: Facet<'facet>,
{
    trace!(
        "Starting JSON deserialization for type {}",
        core::any::type_name::<T>().blue()
    );
    let mut deserializer = JsonDeserializer::new(input);
    let result = facet_deserialize_externally_driven::deserialize_iterative::<JsonDeserializer, T>(
        wip,
        &mut deserializer,
    )
    .map_err(|e| match e {
        DeserializeError::Format(e) => e,
        DeserializeError::Reflect(e) => JsonError::new_reflect(e, input, deserializer.last_span),
        DeserializeError::UnknownField { field_name, shape } => JsonError::new(
            JsonErrorKind::UnknownField { field_name, shape },
            deserializer.input,
            deserializer.last_span,
        ),
        DeserializeError::Custom(s) => {
            JsonError::new(JsonErrorKind::InvalidUtf8(s), input, deserializer.last_span)
        }
    })?;
    trace!("Deserialization successful");
    Ok(result)
}

/// JSON-specific implementation of the Deserializer trait
struct JsonDeserializer<'a> {
    tokenizer: Tokenizer<'a>,
    last_span: Span,
    unread_token: Option<Spanned<Token>>,
    input: &'a [u8],
    current_field_name: Option<String>,
}

impl<'a> JsonDeserializer<'a> {
    fn new(input: &'a [u8]) -> Self {
        trace!(
            "Creating new JsonDeserializer with {} bytes of input",
            input.len()
        );
        Self {
            tokenizer: Tokenizer::new(input),
            last_span: Span::new(0, 0),
            unread_token: None,
            input,
            current_field_name: None,
        }
    }

    // fn current_span(&self) -> Span {
    //     self.last_span
    // }

    fn read_token(&mut self) -> Result<Spanned<Token>, JsonError<'a>> {
        if let Some(token) = self.unread_token.take() {
            trace!("Read cached token: {:?}", token.node);
            self.last_span = token.span;
            Ok(token)
        } else {
            match self.tokenizer.next_token() {
                Ok(token) => {
                    trace!("Read fresh token: {:?} at {:?}", token.node, token.span);
                    self.last_span = token.span;
                    Ok(token)
                }
                Err(e) => {
                    trace!("Token error: {:?} at {:?}", e.kind, e.span);
                    self.last_span = e.span;
                    Err(JsonError::new_syntax(e.kind, self.input, e.span))
                }
            }
        }
    }

    fn clear_unread_token(&mut self) {
        self.unread_token = None;
    }

    fn peek_token(&mut self) -> Result<Spanned<Token>, JsonError<'a>> {
        match self.unread_token.take() {
            Some(token) => {
                trace!("Peeked cached token: {token:?}");
                self.unread_token = Some(token.clone()); // put that back where it came from
                Ok(token)
            }
            None => {
                trace!("Peek requires reading a token...");
                let correct_span = self.last_span;
                let token = self.read_token()?;
                self.last_span = correct_span;
                self.unread_token = Some(token.clone());
                Ok(token)
            }
        }
    }
}

impl<'a> facet_deserialize_externally_driven::Deserializer for JsonDeserializer<'a> {
    type Error = JsonError<'a>;

    fn deserialize_u8(&mut self) -> Result<u8, JsonError<'a>> {
        trace!("Deserializing u8");
        let token = self.read_token()?;
        match token.node {
            Token::U64(n) if n <= u8::MAX as u64 => {
                trace!("Parsed u8: {n}");
                Ok(n as u8)
            }
            Token::I64(n) if n >= 0 && n <= u8::MAX as i64 => {
                trace!("Parsed u8 from i64: {n}");
                Ok(n as u8)
            }
            _ => {
                trace!("Number out of range for u8: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_u16(&mut self) -> Result<u16, JsonError<'a>> {
        trace!("Deserializing u16");
        let token = self.read_token()?;
        match token.node {
            Token::U64(n) if n <= u16::MAX as u64 => {
                trace!("Parsed u16: {n}");
                Ok(n as u16)
            }
            Token::I64(n) if n >= 0 && n <= u16::MAX as i64 => {
                trace!("Parsed u16 from i64: {n}");
                Ok(n as u16)
            }
            _ => {
                trace!("Number out of range for u16: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_u32(&mut self) -> Result<u32, JsonError<'a>> {
        trace!("Deserializing u32");
        let token = self.read_token()?;
        match token.node {
            Token::U64(n) if n <= u32::MAX as u64 => {
                trace!("Parsed u32: {n}");
                Ok(n as u32)
            }
            Token::I64(n) if n >= 0 && n <= u32::MAX as i64 => {
                trace!("Parsed u32 from i64: {n}");
                Ok(n as u32)
            }
            _ => {
                trace!("Number out of range for u32: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_u64(&mut self) -> Result<u64, JsonError<'a>> {
        trace!("Deserializing u64");
        let token = self.read_token()?;
        match token.node {
            Token::U64(n) => {
                trace!("Parsed u64: {n}");
                Ok(n)
            }
            Token::I64(n) if n >= 0 => {
                trace!("Parsed u64 from i64: {n}");
                Ok(n as u64)
            }
            _ => {
                trace!("Number out of range for u64: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_u128(&mut self) -> Result<u128, JsonError<'a>> {
        trace!("Deserializing u128");
        let token = self.read_token()?;
        match token.node {
            Token::U64(n) => {
                trace!("Parsed u128 from u64: {n}");
                Ok(n as u128)
            }
            Token::I64(n) if n >= 0 => {
                trace!("Parsed u128 from i64: {n}");
                Ok(n as u128)
            }
            _ => {
                trace!("Number out of range for u128: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_usize(&mut self) -> Result<usize, JsonError<'a>> {
        trace!("Deserializing usize");
        let token = self.read_token()?;
        match token.node {
            Token::U64(n) if n <= usize::MAX as u64 => {
                trace!("Parsed usize from u64: {n}");
                Ok(n as usize)
            }
            Token::I64(n) if n >= 0 && n <= usize::MAX as i64 => {
                trace!("Parsed usize from i64: {n}");
                Ok(n as usize)
            }
            _ => {
                trace!("Number out of range for usize: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_i8(&mut self) -> Result<i8, JsonError<'a>> {
        trace!("Deserializing i8");
        let token = self.read_token()?;
        match token.node {
            Token::I64(n) if n >= i8::MIN as i64 && n <= i8::MAX as i64 => {
                trace!("Parsed i8 from i64: {n}");
                Ok(n as i8)
            }
            Token::U64(n) if n <= i8::MAX as u64 => {
                trace!("Parsed i8 from u64: {n}");
                Ok(n as i8)
            }
            _ => {
                trace!("Number out of range for i8: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_i16(&mut self) -> Result<i16, JsonError<'a>> {
        trace!("Deserializing i16");
        let token = self.read_token()?;
        match token.node {
            Token::I64(n) if n >= i16::MIN as i64 && n <= i16::MAX as i64 => {
                trace!("Parsed i16 from i64: {n}");
                Ok(n as i16)
            }
            Token::U64(n) if n <= i16::MAX as u64 => {
                trace!("Parsed i16 from u64: {n}");
                Ok(n as i16)
            }
            _ => {
                trace!("Number out of range for i16: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_i32(&mut self) -> Result<i32, JsonError<'a>> {
        trace!("Deserializing i32");
        let token = self.read_token()?;
        match token.node {
            Token::I64(n) if n >= i32::MIN as i64 && n <= i32::MAX as i64 => {
                trace!("Parsed i32 from i64: {n}");
                Ok(n as i32)
            }
            Token::U64(n) if n <= i32::MAX as u64 => {
                trace!("Parsed i32 from u64: {n}");
                Ok(n as i32)
            }
            Token::I64(i) => {
                trace!("Number `{i}` out of range for i32: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(i as f64),
                    self.input,
                    self.last_span,
                ))
            }
            Token::U64(n) => {
                trace!("Number `{n}` out of range for i32: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(n as f64),
                    self.input,
                    self.last_span,
                ))
            }
            _ => {
                trace!("Unexpected token for i32: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "in value",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_i64(&mut self) -> Result<i64, JsonError<'a>> {
        trace!("Deserializing i64");
        let token = self.read_token()?;
        match token.node {
            Token::I64(n) => {
                trace!("Parsed i64: {n}");
                Ok(n)
            }
            Token::U64(n) if n <= i64::MAX as u64 => {
                trace!("Parsed i64 from u64: {n}");
                Ok(n as i64)
            }
            Token::U64(n) => {
                trace!("Number `{n}`out of range for i64: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
            _ => {
                trace!("Unexpected token for i64: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "in value",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_i128(&mut self) -> Result<i128, JsonError<'a>> {
        trace!("Deserializing i128");
        let token = self.read_token()?;
        match token.node {
            Token::I64(n) => {
                trace!("Parsed i128 from i64: {n}");
                Ok(n as i128)
            }
            Token::U64(n) => {
                trace!("Parsed i128 from u64: {n}");
                Ok(n as i128)
            }
            _ => {
                trace!("Number out of range for i128: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_isize(&mut self) -> Result<isize, JsonError<'a>> {
        trace!("Deserializing isize");
        let token = self.read_token()?;
        match token.node {
            Token::I64(n) if n >= isize::MIN as i64 && n <= isize::MAX as i64 => {
                trace!("Parsed isize from i64: {n}");
                Ok(n as isize)
            }
            Token::U64(n) if n <= isize::MAX as u64 => {
                trace!("Parsed isize from u64: {n}");
                Ok(n as isize)
            }
            _ => {
                trace!("Number out of range for isize: {:?}", token.node);
                Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange(0.0),
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_f32(&mut self) -> Result<f32, JsonError<'a>> {
        trace!("Deserializing f32");
        let token = self.read_token()?;
        match token.node {
            Token::F64(n) => {
                trace!("Parsed f32 from f64: {n}");
                Ok(n as f32)
            }
            Token::I64(n) => {
                trace!("Parsed f32 from i64: {n}");
                Ok(n as f32)
            }
            Token::U64(n) => {
                trace!("Parsed f32 from u64: {n}");
                Ok(n as f32)
            }
            _ => {
                trace!("Expected number for f32, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "number",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_f64(&mut self) -> Result<f64, JsonError<'a>> {
        trace!("Deserializing f64");
        let token = self.read_token()?;
        match token.node {
            Token::F64(n) => {
                trace!("Parsed f64: {n}");
                Ok(n)
            }
            Token::I64(n) => {
                trace!("Parsed f64 from i64: {n}");
                Ok(n as f64)
            }
            Token::U64(n) => {
                trace!("Parsed f64 from u64: {n}");
                Ok(n as f64)
            }
            _ => {
                trace!("Expected number for f64, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "number",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_bool(&mut self) -> Result<bool, JsonError<'a>> {
        trace!("Deserializing bool");
        let token = self.read_token()?;
        match token.node {
            Token::True => {
                trace!("Parsed bool: true");
                Ok(true)
            }
            Token::False => {
                trace!("Parsed bool: false");
                Ok(false)
            }
            _ => {
                trace!("Expected boolean, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "boolean",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_char(&mut self) -> Result<char, JsonError<'a>> {
        trace!("Deserializing char");
        let token = self.read_token()?;
        match token.node {
            Token::String(s) if s.chars().count() == 1 => {
                let ch = s.chars().next().unwrap();
                trace!("Parsed char: '{ch}'");
                Ok(ch)
            }
            _ => {
                trace!("Expected character, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "character",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_string(&mut self) -> Result<String, JsonError<'a>> {
        trace!("Deserializing String");
        let token = self.read_token()?;
        match token.node {
            Token::String(s) => {
                trace!("Parsed string: `{s}`");
                Ok(s.to_string())
            }
            _ => {
                trace!("Expected string, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "string",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn deserialize_bytes(&mut self) -> Result<Vec<u8>, JsonError<'a>> {
        trace!("Deserializing bytes");
        // In JSON, bytes are typically represented as base64-encoded strings
        let token = self.read_token()?;
        match token.node {
            Token::String(s) => {
                trace!("Parsing bytes from string of length {}", s.len());
                // This is a simplified implementation
                Ok(s.as_bytes().to_vec())
            }
            _ => {
                trace!("Expected string for bytes, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "string",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn is_none(&mut self) -> Result<bool, JsonError<'a>> {
        let token = self.peek_token()?;
        let is_none = token.node == Token::Null;
        if is_none {
            self.read_token()?;
        }
        trace!("Checking if Option is None: {is_none}");
        Ok(is_none)
    }

    fn deserialize_unit(&mut self) -> Result<(), JsonError<'a>> {
        trace!("Deserializing unit type ()");
        let token = self.read_token()?;
        match token.node {
            Token::Null => {
                trace!("Parsed unit from null");
                Ok(())
            }
            Token::LBracket => {
                // Check if this is an empty array []
                trace!("Checking if [] represents unit");
                let next = self.read_token()?;
                if next.node == Token::RBracket {
                    trace!("Parsed unit from empty array []");
                    Ok(())
                } else {
                    trace!("Expected empty array for unit, got non-empty array");
                    Err(JsonError::new_unexpected(
                        next.node,
                        "null or empty array",
                        self.input,
                        self.last_span,
                    ))
                }
            }
            _ => {
                trace!("Expected null or [] for unit, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "null",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn get_variant(&mut self) -> Result<String, JsonError<'a>> {
        trace!("Getting enum variant");
        // In JSON, enum variants are typically represented as strings or objects
        // with a single field where the field name is the variant name
        let token = self.peek_token()?;

        match token.node {
            Token::String(variant_name) => {
                // This is a unit variant represented as a string
                trace!("Found string variant: {variant_name:?}");
                self.read_token()?; // Consume the token
                // Return the variant name as a &'static str - the Facet deserializer
                // will handle looking up the correct variant index
                Ok(variant_name)
            }
            Token::LBrace => {
                trace!("Found object-based variant");
                // For tagged variants like {"VariantName": {...}}
                // Don't consume the token yet - we need it for start_object

                // Look ahead to get the variant name from the first field
                self.start_object()?;
                if let Some(variant_name) = self.next_field_name()? {
                    trace!("Extracted variant name from object: {variant_name:?}");
                    // Return the variant name - the index will be resolved by the Facet deserializer
                    // We don't consume tokens as they'll be processed by subsequent calls
                    Ok(variant_name)
                } else {
                    trace!("Empty object found when expecting variant");
                    Err(JsonError::new_unexpected(
                        Token::RBrace,
                        "variant field name",
                        self.input,
                        self.last_span,
                    ))
                }
            }
            _ => {
                trace!(
                    "Expected string or object for variant, got: {:?}",
                    token.node
                );
                Err(JsonError::new_unexpected(
                    token.node,
                    "string or object",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn start_object(&mut self) -> Result<Option<usize>, JsonError<'a>> {
        trace!("Starting object");
        let token = self.peek_token()?;
        match token.node {
            Token::LBrace => {
                trace!("Found opening brace for object");
                self.read_token()?;
                Ok(None) // Size unknown in JSON
            }
            _ => {
                trace!("Expected `{{` for object, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "{",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn end_object(&mut self) -> Result<(), JsonError<'a>> {
        trace!("Ending object");
        let token = self.read_token()?;
        match token.node {
            Token::RBrace => {
                trace!("Found closing brace for object");
                Ok(())
            }
            _ => {
                trace!("Expected `}}` for object end, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "}",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn start_array(&mut self) -> Result<Option<usize>, JsonError<'a>> {
        trace!("Starting array");
        let token = self.read_token()?;
        match token.node {
            Token::LBracket => {
                trace!("Found opening bracket for array");
                Ok(None) // Size unknown in JSON, I cry every time
            }
            _ => {
                trace!("Expected `[` for array, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "[",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn end_array(&mut self) -> Result<(), JsonError<'a>> {
        trace!("Ending array");
        let token = self.read_token()?;
        match token.node {
            Token::RBracket => {
                trace!("Found closing bracket for array");
                Ok(())
            }
            _ => {
                trace!("Expected `]` for array end, got: {:?}", token.node);
                Err(JsonError::new_unexpected(
                    token.node,
                    "]",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn start_map(&mut self) -> Result<Option<usize>, JsonError<'a>> {
        trace!("Starting map (as JSON object)");
        // Maps in JSON are objects
        self.start_object()
    }

    fn end_map(&mut self) -> Result<(), JsonError<'a>> {
        trace!("Ending map (as JSON object)");
        self.end_object()
    }

    fn next_field_name(&mut self) -> Result<Option<String>, JsonError<'a>> {
        trace!("Getting next field name");
        let token = self.peek_token()?;
        match token.node {
            Token::String(field_name) => {
                trace!("Found field name: {field_name:?}");
                self.read_token()?; // Consume the string token

                // Look for the colon
                let colon = self.peek_token()?;
                if colon.node != Token::Colon {
                    trace!("Expected : after field name, got: {:?}", colon.node);
                    return Err(JsonError::new_unexpected(
                        colon.node,
                        "colon",
                        self.input,
                        self.last_span,
                    ));
                }
                self.clear_unread_token(); // Consume the colon
                self.current_field_name = Some(field_name.clone());
                Ok(Some(field_name))
            }
            Token::RBrace => {
                trace!("End of object reached (}})");
                // End of object
                Ok(None)
            }
            _ => {
                trace!(
                    "Expected string or }} for field name, got: {:?}",
                    token.node
                );
                Err(JsonError::new_unexpected(
                    token.node,
                    "object key or closing brace",
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn has_next(&mut self) -> Result<bool, JsonError<'a>> {
        trace!("Checking if there are more items");
        let token = self.peek_token()?;
        match token.node {
            Token::RBrace | Token::RBracket => {
                trace!("End of collection reached");
                Ok(false)
            }
            Token::Comma => {
                trace!("Found comma, more items available");
                self.clear_unread_token(); // Consume the comma TODO THIS IS BAD
                Ok(true)
            }
            // First element doesn't have a comma
            _ if self.current_field_name.is_none() => {
                trace!("First element in collection (no comma needed)");
                Ok(true)
            }
            _ => {
                trace!("Expected comma or end of collection, got: {:?}", token.node);
                // Err(JsonError::new_unexpected(
                //     token.node,
                //     ", or end of collection",
                //     self.input,
                //     self.last_span,
                // ))
                Ok(true)
            }
        }
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        trace!("Skipping value");
        let token = self.peek_token()?;

        match token.node {
            // Skip an object
            Token::LBrace => {
                trace!("Skipping object");
                self.clear_unread_token(); // Consume `{`

                // Track nesting level
                let mut depth = 1;

                while depth > 0 {
                    let token = self.read_token()?;
                    match token.node {
                        Token::LBrace => depth += 1,
                        Token::RBrace => depth -= 1,
                        _ => {} // Skip other tokens
                    }
                }

                Ok(())
            }

            // Skip an array
            Token::LBracket => {
                trace!("Skipping array");
                self.clear_unread_token(); // Consume `[`

                // Track nesting level
                let mut depth = 1;

                while depth > 0 {
                    let token = self.read_token()?;
                    match token.node {
                        Token::LBracket => depth += 1,
                        Token::RBracket => depth -= 1,
                        _ => {} // Skip other tokens
                    }
                }

                Ok(())
            }

            // Skip a string key (for object fields)
            Token::String(_) if self.current_field_name.is_none() => {
                trace!("Skipping field key-value pair");
                self.clear_unread_token(); // Consume key

                // Skip the colon
                let colon = self.read_token()?;
                if colon.node != Token::Colon {
                    return Err(JsonError::new_unexpected(
                        colon.node,
                        "colon",
                        self.input,
                        self.last_span,
                    ));
                }

                // Recursively skip the value
                self.skip_value()
            }

            // Skip individual values (primitives or complex)
            _ => {
                trace!("Skipping individual value: {:?}", token.node);
                self.clear_unread_token(); // Consume the token
                Ok(())
            }
        }
    }
}
