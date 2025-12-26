//! Recursive descent JSON deserializer using facet-reflect.

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{self, Display};
use core::ptr;

use alloc::collections::{BTreeMap, BTreeSet};

use facet_core::{
    Characteristic, Def, Facet, KnownPointer, NumericType, PrimitiveType, ScalarType, SequenceType,
    Shape, ShapeLayout, StructKind, Type, UserType,
};
use facet_reflect::{Partial, ReflectError, is_spanned_shape};
use facet_solver::{FieldInfo, PathSegment, Schema, Solver, VariantsByFormat, specificity_score};

use crate::RawJson;
use crate::adapter::{AdapterError, AdapterErrorKind, SliceAdapter, SpannedAdapterToken, Token};
use crate::scanner::ScanErrorKind;
use facet_reflect::Span;

/// Find the best matching field name from a list of expected fields.
/// Returns Some(suggestion) if a match with similarity >= 0.6 is found.
fn find_similar_field<'a>(unknown: &str, expected: &[&'a str]) -> Option<&'a str> {
    let mut best_match: Option<(&'a str, f64)> = None;

    for &candidate in expected {
        let similarity = strsim::jaro_winkler(unknown, candidate);
        if similarity >= 0.6 && best_match.is_none_or(|(_, best_sim)| similarity > best_sim) {
            best_match = Some((candidate, similarity));
        }
    }

    best_match.map(|(name, _)| name)
}

// ============================================================================
// Error Types
// ============================================================================

/// Error type for JSON deserialization.
#[derive(Debug)]
pub struct JsonError {
    /// The specific kind of error
    pub kind: JsonErrorKind,
    /// Source span where the error occurred
    pub span: Option<Span>,
    /// The source input (for diagnostics)
    pub source_code: Option<String>,
}

impl Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for JsonError {}

impl miette::Diagnostic for JsonError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(self.kind.code()))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.source_code
            .as_ref()
            .map(|s| s as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        // Handle MissingField with multiple spans
        if let JsonErrorKind::MissingField {
            field,
            object_start,
            object_end,
        } = &self.kind
        {
            let mut labels = Vec::new();
            if let Some(start) = object_start {
                labels.push(miette::LabeledSpan::new(
                    Some("object started here".into()),
                    start.offset,
                    start.len,
                ));
            }
            if let Some(end) = object_end {
                labels.push(miette::LabeledSpan::new(
                    Some(format!("object ended without field `{field}`")),
                    end.offset,
                    end.len,
                ));
            }
            if labels.is_empty() {
                return None;
            }
            return Some(Box::new(labels.into_iter()));
        }

        // Default: single span with label
        let span = self.span?;
        Some(Box::new(core::iter::once(miette::LabeledSpan::new(
            Some(self.kind.label()),
            span.offset,
            span.len,
        ))))
    }
}

impl JsonError {
    /// Create a new error with span information
    pub fn new(kind: JsonErrorKind, span: Span) -> Self {
        JsonError {
            kind,
            span: Some(span),
            source_code: None,
        }
    }

    /// Create an error without span information
    pub fn without_span(kind: JsonErrorKind) -> Self {
        JsonError {
            kind,
            span: None,
            source_code: None,
        }
    }

    /// Attach source code for rich diagnostics
    pub fn with_source(mut self, source: &str) -> Self {
        self.source_code = Some(source.to_string());
        self
    }
}

#[inline(never)]
#[cold]
fn attach_source_cold(mut err: JsonError, source: Option<&str>) -> JsonError {
    if let Some(src) = source {
        err.source_code = Some(src.to_string());
    }
    err
}

/// Specific error kinds for JSON deserialization
#[derive(Debug)]
pub enum JsonErrorKind {
    /// Scanner/adapter error
    Scan(ScanErrorKind),
    /// Scanner error with type context (what type was being parsed)
    ScanWithContext {
        /// The underlying scan error
        error: ScanErrorKind,
        /// The type that was being parsed
        expected_type: &'static str,
    },
    /// Unexpected token
    UnexpectedToken {
        /// The token that was found
        got: String,
        /// What was expected instead
        expected: &'static str,
    },
    /// Unexpected end of input
    UnexpectedEof {
        /// What was expected before EOF
        expected: &'static str,
    },
    /// Type mismatch
    TypeMismatch {
        /// The expected type
        expected: &'static str,
        /// The actual type found
        got: &'static str,
    },
    /// Unknown field in struct
    UnknownField {
        /// The unknown field name
        field: String,
        /// List of valid field names
        expected: Vec<&'static str>,
        /// Suggested field name (if similar to an expected field)
        suggestion: Option<&'static str>,
    },
    /// Missing required field
    MissingField {
        /// The name of the missing field
        field: &'static str,
        /// Span of the object start (opening brace)
        object_start: Option<Span>,
        /// Span of the object end (closing brace)
        object_end: Option<Span>,
    },
    /// Invalid value for type
    InvalidValue {
        /// Description of why the value is invalid
        message: String,
    },
    /// Reflection error from facet-reflect
    Reflect(ReflectError),
    /// Number out of range
    NumberOutOfRange {
        /// The numeric value that was out of range
        value: String,
        /// The target type that couldn't hold the value
        target_type: &'static str,
    },
    /// Duplicate key in object
    DuplicateKey {
        /// The key that appeared more than once
        key: String,
    },
    /// Invalid UTF-8 in string
    InvalidUtf8,
    /// Solver error (for flattened types)
    Solver(String),
    /// I/O error (for streaming deserialization)
    Io(String),
}

impl Display for JsonErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonErrorKind::Scan(e) => write!(f, "{e:?}"),
            JsonErrorKind::ScanWithContext {
                error,
                expected_type,
            } => {
                write!(f, "{error:?} (while parsing {expected_type})")
            }
            JsonErrorKind::UnexpectedToken { got, expected } => {
                write!(f, "unexpected token: got {got}, expected {expected}")
            }
            JsonErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
            JsonErrorKind::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            JsonErrorKind::UnknownField {
                field,
                expected,
                suggestion,
            } => {
                write!(f, "unknown field `{field}`, expected one of: {expected:?}")?;
                if let Some(suggested) = suggestion {
                    write!(f, " (did you mean `{suggested}`?)")?;
                }
                Ok(())
            }
            JsonErrorKind::MissingField { field, .. } => {
                write!(f, "missing required field `{field}`")
            }
            JsonErrorKind::InvalidValue { message } => {
                write!(f, "invalid value: {message}")
            }
            JsonErrorKind::Reflect(e) => write!(f, "reflection error: {e}"),
            JsonErrorKind::NumberOutOfRange { value, target_type } => {
                write!(f, "number `{value}` out of range for {target_type}")
            }
            JsonErrorKind::DuplicateKey { key } => {
                write!(f, "duplicate key `{key}`")
            }
            JsonErrorKind::InvalidUtf8 => write!(f, "invalid UTF-8 sequence"),
            JsonErrorKind::Solver(msg) => write!(f, "solver error: {msg}"),
            JsonErrorKind::Io(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl JsonErrorKind {
    /// Get an error code for this kind of error.
    pub fn code(&self) -> &'static str {
        match self {
            JsonErrorKind::Scan(_) => "json::scan",
            JsonErrorKind::ScanWithContext { .. } => "json::scan",
            JsonErrorKind::UnexpectedToken { .. } => "json::unexpected_token",
            JsonErrorKind::UnexpectedEof { .. } => "json::unexpected_eof",
            JsonErrorKind::TypeMismatch { .. } => "json::type_mismatch",
            JsonErrorKind::UnknownField { .. } => "json::unknown_field",
            JsonErrorKind::MissingField { .. } => "json::missing_field",
            JsonErrorKind::InvalidValue { .. } => "json::invalid_value",
            JsonErrorKind::Reflect(_) => "json::reflect",
            JsonErrorKind::NumberOutOfRange { .. } => "json::number_out_of_range",
            JsonErrorKind::DuplicateKey { .. } => "json::duplicate_key",
            JsonErrorKind::InvalidUtf8 => "json::invalid_utf8",
            JsonErrorKind::Solver(_) => "json::solver",
            JsonErrorKind::Io(_) => "json::io",
        }
    }

    /// Get a label describing where/what the error points to.
    pub fn label(&self) -> String {
        match self {
            JsonErrorKind::Scan(e) => match e {
                ScanErrorKind::UnexpectedChar(c) => format!("unexpected '{c}'"),
                ScanErrorKind::UnexpectedEof(ctx) => format!("unexpected end of input {ctx}"),
                ScanErrorKind::InvalidUtf8 => "invalid UTF-8 here".into(),
            },
            JsonErrorKind::ScanWithContext {
                error,
                expected_type,
            } => match error {
                ScanErrorKind::UnexpectedChar(c) => {
                    format!("unexpected '{c}', expected {expected_type}")
                }
                ScanErrorKind::UnexpectedEof(_) => {
                    format!("unexpected end of input, expected {expected_type}")
                }
                ScanErrorKind::InvalidUtf8 => "invalid UTF-8 here".into(),
            },
            JsonErrorKind::UnexpectedToken { got, expected } => {
                format!("expected {expected}, got '{got}'")
            }
            JsonErrorKind::UnexpectedEof { expected } => format!("expected {expected}"),
            JsonErrorKind::TypeMismatch { expected, got } => {
                format!("expected {expected}, got {got}")
            }
            JsonErrorKind::UnknownField {
                field, suggestion, ..
            } => {
                if let Some(suggested) = suggestion {
                    format!("unknown field '{field}' - did you mean '{suggested}'?")
                } else {
                    format!("unknown field '{field}'")
                }
            }
            JsonErrorKind::MissingField { field, .. } => format!("missing field '{field}'"),
            JsonErrorKind::InvalidValue { .. } => "invalid value".into(),
            JsonErrorKind::Reflect(_) => "reflection error".into(),
            JsonErrorKind::NumberOutOfRange { target_type, .. } => {
                format!("out of range for {target_type}")
            }
            JsonErrorKind::DuplicateKey { key } => format!("duplicate key '{key}'"),
            JsonErrorKind::InvalidUtf8 => "invalid UTF-8".into(),
            JsonErrorKind::Solver(_) => "solver error".into(),
            JsonErrorKind::Io(_) => "I/O error".into(),
        }
    }
}

impl From<AdapterError> for JsonError {
    fn from(err: AdapterError) -> Self {
        let kind = match err.kind {
            AdapterErrorKind::Scan(scan_err) => JsonErrorKind::Scan(scan_err),
            AdapterErrorKind::NeedMore => JsonErrorKind::UnexpectedEof {
                expected: "more data",
            },
        };
        JsonError {
            kind,
            span: Some(err.span),
            source_code: None,
        }
    }
}

impl From<ReflectError> for JsonError {
    fn from(err: ReflectError) -> Self {
        JsonError {
            kind: JsonErrorKind::Reflect(err),
            span: None,
            source_code: None,
        }
    }
}

/// Result type for JSON deserialization
pub type Result<T> = core::result::Result<T, JsonError>;

// ============================================================================
// Deserializer
// ============================================================================

use crate::adapter::TokenSource;

/// JSON deserializer using recursive descent.
///
/// Generic over a token source `A` which must implement `TokenSource<'input>`.
/// The const generic `BORROW` controls whether string data can be borrowed:
/// - `BORROW=true`: strings without escapes are borrowed from input (for slice-based parsing)
/// - `BORROW=false`: all strings are owned (for streaming or owned output)
///
/// For slice-based parsing, use `SliceAdapter<'input, BORROW>`.
/// For streaming parsing, use `StreamingAdapter` with `BORROW=false`.
pub struct JsonDeserializer<'input, const BORROW: bool, A: TokenSource<'input>> {
    adapter: A,
    /// Peeked token (for lookahead)
    peeked: Option<SpannedAdapterToken<'input>>,
}

impl<'input> JsonDeserializer<'input, true, SliceAdapter<'input, true>> {
    /// Create a new deserializer for the given input.
    /// Strings without escapes will be borrowed from input.
    pub fn new(input: &'input [u8]) -> Self {
        JsonDeserializer {
            adapter: SliceAdapter::new(input),
            peeked: None,
        }
    }
}

impl<'input> JsonDeserializer<'input, false, SliceAdapter<'input, false>> {
    /// Create a new deserializer that produces owned strings.
    /// Use this when deserializing into owned types from temporary buffers.
    pub fn new_owned(input: &'input [u8]) -> Self {
        JsonDeserializer {
            adapter: SliceAdapter::new(input),
            peeked: None,
        }
    }
}

impl<'input, const BORROW: bool, A: TokenSource<'input>> JsonDeserializer<'input, BORROW, A> {
    /// Create a deserializer from an existing adapter.
    pub fn from_adapter(adapter: A) -> Self {
        JsonDeserializer {
            adapter,
            peeked: None,
        }
    }

    /// Peek at the next token without consuming it.
    fn peek(&mut self) -> Result<&SpannedAdapterToken<'input>> {
        if self.peeked.is_none() {
            self.peeked = Some(self.adapter.next_token()?);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    /// Consume and return the next token.
    fn next(&mut self) -> Result<SpannedAdapterToken<'input>> {
        if let Some(token) = self.peeked.take() {
            Ok(token)
        } else {
            Ok(self.adapter.next_token()?)
        }
    }

    /// Consume the next token with type context for better error messages.
    fn next_expecting(
        &mut self,
        expected_type: &'static str,
    ) -> Result<SpannedAdapterToken<'input>> {
        match self.next() {
            Ok(token) => Ok(token),
            Err(e) => {
                // If it's a plain scan error, wrap it with context
                if let JsonErrorKind::Scan(scan_err) = e.kind {
                    Err(JsonError {
                        kind: JsonErrorKind::ScanWithContext {
                            error: scan_err,
                            expected_type,
                        },
                        span: e.span,
                        source_code: e.source_code,
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Expect a specific token, consuming it.
    #[allow(dead_code)]
    fn expect(&mut self, _expected: &'static str) -> Result<SpannedAdapterToken<'input>> {
        let token = self.next()?;
        // For now, just return the token - caller validates
        Ok(token)
    }

    /// Skip a JSON value (for unknown fields).
    fn skip_value(&mut self) -> Result<Span> {
        let token = self.next()?;
        let start_span = token.span;

        match token.token {
            Token::ObjectStart => {
                // Skip object
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next()?;
                    match t.token {
                        Token::ObjectStart => depth += 1,
                        Token::ObjectEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(start_span)
            }
            Token::ArrayStart => {
                // Skip array
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next()?;
                    match t.token {
                        Token::ArrayStart => depth += 1,
                        Token::ArrayEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(start_span)
            }
            Token::String(_)
            | Token::F64(_)
            | Token::I64(_)
            | Token::U64(_)
            | Token::U128(_)
            | Token::I128(_)
            | Token::True
            | Token::False
            | Token::Null => Ok(start_span),
            _ => Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "value",
                },
                token.span,
            )),
        }
    }

    /// Capture a raw JSON value as a string slice.
    ///
    /// This skips the value while tracking its full span, then returns
    /// the raw JSON text.
    ///
    /// Note: This requires the adapter to provide input bytes (slice-based parsing).
    /// For streaming adapters, this will return an error.
    fn capture_raw_value(&mut self) -> Result<&'input str> {
        // Check if we have access to input bytes
        let input = self.adapter.input_bytes().ok_or_else(|| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: "RawJson capture is not supported in streaming mode".into(),
            })
        })?;

        let token = self.next()?;
        let start_offset = token.span.offset;

        let end_offset = match token.token {
            Token::ObjectStart => {
                // Capture object
                let mut depth = 1;
                let mut last_span = token.span;
                while depth > 0 {
                    let t = self.next()?;
                    last_span = t.span;
                    match t.token {
                        Token::ObjectStart => depth += 1,
                        Token::ObjectEnd => depth -= 1,
                        _ => {}
                    }
                }
                last_span.offset + last_span.len
            }
            Token::ArrayStart => {
                // Capture array
                let mut depth = 1;
                let mut last_span = token.span;
                while depth > 0 {
                    let t = self.next()?;
                    last_span = t.span;
                    match t.token {
                        Token::ArrayStart => depth += 1,
                        Token::ArrayEnd => depth -= 1,
                        _ => {}
                    }
                }
                last_span.offset + last_span.len
            }
            Token::String(_)
            | Token::F64(_)
            | Token::I64(_)
            | Token::U64(_)
            | Token::U128(_)
            | Token::I128(_)
            | Token::True
            | Token::False
            | Token::Null => token.span.offset + token.span.len,
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", token.token),
                        expected: "value",
                    },
                    token.span,
                ));
            }
        };

        // Extract the raw bytes and convert to str
        let raw_bytes = &input[start_offset..end_offset];
        core::str::from_utf8(raw_bytes).map_err(|e| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!("invalid UTF-8 in raw JSON: {e}"),
            })
        })
    }

    /// Check if a struct has any flattened fields.
    fn has_flatten_fields(struct_def: &facet_core::StructType) -> bool {
        struct_def.fields.iter().any(|f| f.is_flattened())
    }

    /// Main deserialization entry point - deserialize into a Partial.
    pub fn deserialize_into(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();
        log::trace!(
            "deserialize_into: shape={}, def={:?}",
            shape.type_identifier,
            std::mem::discriminant(&shape.def)
        );

        // Check for Spanned<T> wrapper first
        if is_spanned_shape(shape) {
            return self.deserialize_spanned(wip);
        }

        // Check for RawJson - capture raw bytes without parsing
        if shape == RawJson::SHAPE {
            let raw = self.capture_raw_value()?;
            wip = wip.set(RawJson::new(raw))?;
            return Ok(wip);
        }

        // Check for container-level proxy (applies to values inside Vec<T>, Option<T>, etc.)
        #[cfg(feature = "alloc")]
        {
            let (wip_returned, has_proxy) = wip.begin_custom_deserialization_from_shape()?;
            wip = wip_returned;
            if has_proxy {
                log::trace!(
                    "deserialize_into: using container-level proxy for {}",
                    shape.type_identifier
                );
                wip = self.deserialize_into(wip)?;
                return wip.end().map_err(Into::into);
            }
        }

        // Check Def first for Option (which is also a Type::User::Enum)
        // Must come before the inner check since Option also has .inner() set
        let is_option = matches!(&shape.def, Def::Option(_));
        log::trace!("deserialize_into: is_option={is_option}");
        if is_option {
            return self.deserialize_option(wip);
        }

        // Priority 1: Check for builder_shape (immutable collections like Bytes -> BytesMut)
        // These types need to build through a different type
        if shape.builder_shape.is_some() {
            wip = wip.begin_inner()?;
            // Check if field has custom deserialization
            if wip
                .parent_field()
                .and_then(|field| field.proxy_convert_in_fn())
                .is_some()
            {
                wip = wip.begin_custom_deserialization()?;
                wip = self.deserialize_into(wip)?;
                wip = wip.end()?;
            } else {
                wip = self.deserialize_into(wip)?;
            }
            wip = wip.end()?;
            return Ok(wip);
        }

        // Priority 2: Check for smart pointers (Box, Arc, Rc) before other Defs
        if matches!(&shape.def, Def::Pointer(_)) {
            return self.deserialize_pointer(wip);
        }

        // Priority 3: Check for .inner (transparent wrappers like NonZero)
        // Collections (List/Map/Set/Array) have .inner for variance but shouldn't use this path
        if shape.inner.is_some()
            && !matches!(
                &shape.def,
                Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
            )
        {
            wip = wip.begin_inner()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
            return Ok(wip);
        }

        // Priority 4: Check the Type - structs and enums are identified by Type, not Def
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                // Tuples and tuple structs both deserialize from JSON arrays.
                if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                    return self.deserialize_tuple(wip);
                }
                return self.deserialize_struct(wip);
            }
            Type::User(UserType::Enum(_)) => return self.deserialize_enum(wip),
            _ => {}
        }

        // Priority 5: Check Def for containers and special types
        match &shape.def {
            Def::Scalar => self.deserialize_scalar(wip),
            Def::List(_) => self.deserialize_list(wip),
            Def::Map(_) => self.deserialize_map(wip),
            Def::Array(_) => self.deserialize_array(wip),
            Def::Set(_) => self.deserialize_set(wip),
            Def::DynamicValue(_) => self.deserialize_dynamic_value(wip),
            _ => Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!("unsupported shape def: {:?}", shape.def),
            })),
        }
    }

    /// Deserialize into a type with span metadata (like `Spanned<T>`).
    ///
    /// This handles structs that have:
    /// - One or more non-metadata fields (the actual values to deserialize)
    /// - A field with `#[facet(metadata = span)]` to store source location
    fn deserialize_spanned(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_spanned");

        let shape = wip.shape();

        // Find the span metadata field and non-metadata fields
        let Type::User(UserType::Struct(struct_def)) = &shape.ty else {
            return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!(
                    "expected struct with span metadata, found {}",
                    shape.type_identifier
                ),
            }));
        };

        let span_field = struct_def
            .fields
            .iter()
            .find(|f| f.metadata_kind() == Some("span"))
            .ok_or_else(|| {
                JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: format!(
                        "expected struct with span metadata field, found {}",
                        shape.type_identifier
                    ),
                })
            })?;

        let value_fields: Vec<_> = struct_def
            .fields
            .iter()
            .filter(|f| !f.is_metadata())
            .collect();

        // Peek to get the span of the value we're about to parse
        let value_span = self.peek()?.span;

        // Deserialize all non-metadata fields
        // For the common case (Spanned<T> with a single "value" field), this is just one field
        for field in value_fields {
            wip = wip.begin_field(field.name)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }

        // Set the span metadata field
        // The span field should be of type Span with offset and len
        wip = wip.begin_field(span_field.name)?;
        wip = wip.set_field("offset", value_span.offset)?;
        wip = wip.set_field("len", value_span.len)?;
        wip = wip.end()?;

        Ok(wip)
    }

    /// Deserialize a scalar value.
    fn deserialize_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        let expected_type = wip.shape().type_identifier;
        let token = self.next_expecting(expected_type)?;
        log::trace!("deserialize_scalar: token={:?}", token.token);

        match token.token {
            Token::String(s) => {
                // Try parse_from_str first if the type supports it (e.g., chrono types)
                if wip.shape().vtable.has_parse() {
                    wip = wip.parse_from_str(&s)?;
                } else if wip.shape().type_identifier == "Cow" {
                    // Zero-copy Cow<str>: preserve borrowed/owned status
                    wip = wip.set(s)?;
                } else {
                    wip = wip.set(s.into_owned())?;
                }
            }
            Token::True => {
                wip = wip.set(true)?;
            }
            Token::False => {
                wip = wip.set(false)?;
            }
            Token::Null => {
                // For scalars, null typically means default
                wip = wip.set_default()?;
            }
            Token::F64(n) => {
                wip = self.set_number_f64(wip, n, token.span)?;
            }
            Token::I64(n) => {
                wip = self.set_number_i64(wip, n, token.span)?;
            }
            Token::U64(n) => {
                wip = self.set_number_u64(wip, n, token.span)?;
            }
            Token::I128(n) => {
                wip = self.set_number_i128(wip, n, token.span)?;
            }
            Token::U128(n) => {
                wip = self.set_number_u128(wip, n, token.span)?;
            }
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", token.token),
                        expected: "scalar value",
                    },
                    token.span,
                ));
            }
        }
        Ok(wip)
    }

    /// Deserialize any JSON value into a DynamicValue type.
    ///
    /// This handles all JSON value types: null, bool, number, string, array, and object.
    fn deserialize_dynamic_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        let token = self.peek()?;
        log::trace!("deserialize_dynamic_value: token={:?}", token.token);

        match token.token {
            Token::Null => {
                self.next()?; // consume the token
                wip = wip.set_default()?;
            }
            Token::True => {
                self.next()?;
                wip = wip.set(true)?;
            }
            Token::False => {
                self.next()?;
                wip = wip.set(false)?;
            }
            Token::I64(n) => {
                self.next()?;
                wip = wip.set(n)?;
            }
            Token::U64(n) => {
                self.next()?;
                // Store as i64 if it fits, otherwise as u64
                if n <= i64::MAX as u64 {
                    wip = wip.set(n as i64)?;
                } else {
                    wip = wip.set(n)?;
                }
            }
            Token::F64(n) => {
                self.next()?;
                wip = wip.set(n)?;
            }
            Token::I128(n) => {
                self.next()?;
                // Try to fit in i64
                if let Ok(n) = i64::try_from(n) {
                    wip = wip.set(n)?;
                } else {
                    return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                        message: format!("i128 value {n} doesn't fit in dynamic value"),
                    }));
                }
            }
            Token::U128(n) => {
                self.next()?;
                // Try to fit in i64 or u64
                if let Ok(n) = i64::try_from(n) {
                    wip = wip.set(n)?;
                } else if let Ok(n) = u64::try_from(n) {
                    wip = wip.set(n)?;
                } else {
                    return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                        message: format!("u128 value {n} doesn't fit in dynamic value"),
                    }));
                }
            }
            Token::String(ref _s) => {
                // Consume token and get owned string
                let token = self.next()?;
                if let Token::String(s) = token.token {
                    wip = wip.set(s.into_owned())?;
                }
            }
            Token::ArrayStart => {
                self.next()?; // consume '['
                wip = wip.begin_list()?;

                loop {
                    let token = self.peek()?;
                    if matches!(token.token, Token::ArrayEnd) {
                        self.next()?;
                        break;
                    }

                    wip = wip.begin_list_item()?;
                    wip = self.deserialize_dynamic_value(wip)?;
                    wip = wip.end()?;

                    let next = self.peek()?;
                    if matches!(next.token, Token::Comma) {
                        self.next()?;
                    }
                }
            }
            Token::ObjectStart => {
                self.next()?; // consume '{'
                wip = wip.begin_map()?; // Initialize as object

                loop {
                    let token = self.peek()?;
                    if matches!(token.token, Token::ObjectEnd) {
                        self.next()?;
                        break;
                    }

                    // Parse key (must be a string)
                    let key_token = self.next()?;
                    let key = match key_token.token {
                        Token::String(s) => s.into_owned(),
                        _ => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedToken {
                                    got: format!("{:?}", key_token.token),
                                    expected: "string key",
                                },
                                key_token.span,
                            ));
                        }
                    };

                    // Expect colon
                    let colon = self.next()?;
                    if !matches!(colon.token, Token::Colon) {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", colon.token),
                                expected: "':'",
                            },
                            colon.span,
                        ));
                    }

                    // Start object entry and deserialize value
                    wip = wip.begin_object_entry(&key)?;
                    wip = self.deserialize_dynamic_value(wip)?;
                    wip = wip.end()?;

                    // Check for comma or end
                    let next = self.peek()?;
                    if matches!(next.token, Token::Comma) {
                        self.next()?;
                    }
                }
            }
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", token.token),
                        expected: "any JSON value",
                    },
                    token.span,
                ));
            }
        }
        Ok(wip)
    }

    /// Set a string value, handling `&str`, `Cow<str>`, and `String` appropriately.
    fn set_string_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();

        // Check if target is &str (shared reference to str)
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            // In owned mode, we cannot borrow from input at all
            if !BORROW {
                return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: "cannot deserialize into &str when borrowing is disabled - use String or Cow<str> instead".into(),
                }));
            }
            match s {
                Cow::Borrowed(borrowed) => {
                    wip = wip.set(borrowed)?;
                    return Ok(wip);
                }
                Cow::Owned(_) => {
                    return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                        message: "cannot borrow &str from JSON string containing escape sequences - use String instead".into(),
                    }));
                }
            }
        }

        // Check if target is Cow<str>
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::Cow))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            wip = wip.set(s)?;
            return Ok(wip);
        }

        // Default: convert to owned String
        wip = wip.set(s.into_owned())?;
        Ok(wip)
    }

    /// Deserialize a map key from a JSON string.
    ///
    /// JSON only allows string keys, but the target map might have non-string key types
    /// (e.g., integers, enums). This function parses the string key into the appropriate type:
    /// - String types: set directly
    /// - Enum unit variants: use select_variant_named
    /// - Integer types: parse the string as a number
    /// - Transparent newtypes: descend into the inner type
    fn deserialize_map_key(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        key: Cow<'input, str>,
        span: Span,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();

        // For transparent types (like UserId(String)), we need to use begin_inner
        // to set the inner value. But NOT for pointer types like &str or Cow<str>
        // which are handled directly.
        let is_pointer = matches!(shape.def, Def::Pointer(_));
        if shape.inner.is_some() && !is_pointer {
            wip = wip.begin_inner()?;
            wip = self.deserialize_map_key(wip, key, span)?;
            wip = wip.end()?;
            return Ok(wip);
        }

        // Check if target is an enum - use select_variant_named for unit variants
        if let Type::User(UserType::Enum(_)) = &shape.ty {
            wip = wip.select_variant_named(&key)?;
            return Ok(wip);
        }

        // Check if target is a numeric type - parse the string key as a number
        if let Type::Primitive(PrimitiveType::Numeric(num_ty)) = &shape.ty {
            match num_ty {
                NumericType::Integer { signed } => {
                    if *signed {
                        let n: i64 = key.parse().map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::InvalidValue {
                                    message: format!(
                                        "cannot parse '{}' as integer for map key",
                                        key
                                    ),
                                },
                                span,
                            )
                        })?;
                        wip = self.set_number_i64(wip, n, span)?;
                    } else {
                        let n: u64 = key.parse().map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::InvalidValue {
                                    message: format!(
                                        "cannot parse '{}' as unsigned integer for map key",
                                        key
                                    ),
                                },
                                span,
                            )
                        })?;
                        wip = self.set_number_u64(wip, n, span)?;
                    }
                    return Ok(wip);
                }
                NumericType::Float => {
                    let n: f64 = key.parse().map_err(|_| {
                        JsonError::new(
                            JsonErrorKind::InvalidValue {
                                message: format!("cannot parse '{}' as float for map key", key),
                            },
                            span,
                        )
                    })?;
                    wip = self.set_number_f64(wip, n, span)?;
                    return Ok(wip);
                }
            }
        }

        // Default: treat as string
        wip = self.set_string_value(wip, key)?;
        Ok(wip)
    }

    /// Set a numeric value, handling type conversions.
    fn set_number_f64(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        n: f64,
        span: Span,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();
        let ty = match &shape.ty {
            Type::Primitive(PrimitiveType::Numeric(ty)) => ty,
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::TypeMismatch {
                        expected: shape.type_identifier,
                        got: "number",
                    },
                    span,
                ));
            }
        };

        match ty {
            NumericType::Float => {
                let size = match shape.layout {
                    ShapeLayout::Sized(layout) => layout.size(),
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::InvalidValue {
                                message: "unsized float".into(),
                            },
                            span,
                        ));
                    }
                };
                match size {
                    4 => {
                        wip = wip.set(n as f32)?;
                    }
                    8 => {
                        wip = wip.set(n)?;
                    }
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::InvalidValue {
                                message: format!("unexpected float size: {size}"),
                            },
                            span,
                        ));
                    }
                }
            }
            NumericType::Integer { signed } => {
                // Try to convert float to integer
                if n.fract() != 0.0 {
                    return Err(JsonError::new(
                        JsonErrorKind::TypeMismatch {
                            expected: shape.type_identifier,
                            got: "float with fractional part",
                        },
                        span,
                    ));
                }
                if *signed {
                    wip = self.set_number_i64(wip, n as i64, span)?;
                } else {
                    wip = self.set_number_u64(wip, n as u64, span)?;
                }
            }
        }
        Ok(wip)
    }

    fn set_number_i64(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        n: i64,
        span: Span,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();
        let size = match shape.layout {
            ShapeLayout::Sized(layout) => layout.size(),
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::InvalidValue {
                        message: "unsized integer".into(),
                    },
                    span,
                ));
            }
        };

        // Check type and convert
        match &shape.ty {
            Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
                match size {
                    1 => {
                        let v = i8::try_from(n).map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::NumberOutOfRange {
                                    value: n.to_string(),
                                    target_type: "i8",
                                },
                                span,
                            )
                        })?;
                        wip = wip.set(v)?;
                    }
                    2 => {
                        let v = i16::try_from(n).map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::NumberOutOfRange {
                                    value: n.to_string(),
                                    target_type: "i16",
                                },
                                span,
                            )
                        })?;
                        wip = wip.set(v)?;
                    }
                    4 => {
                        let v = i32::try_from(n).map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::NumberOutOfRange {
                                    value: n.to_string(),
                                    target_type: "i32",
                                },
                                span,
                            )
                        })?;
                        wip = wip.set(v)?;
                    }
                    8 => {
                        // Check if the target is isize (which has size 8 on 64-bit)
                        if shape.scalar_type() == Some(ScalarType::ISize) {
                            let v = isize::try_from(n).map_err(|_| {
                                JsonError::new(
                                    JsonErrorKind::NumberOutOfRange {
                                        value: n.to_string(),
                                        target_type: "isize",
                                    },
                                    span,
                                )
                            })?;
                            wip = wip.set(v)?;
                        } else {
                            wip = wip.set(n)?;
                        }
                    }
                    16 => {
                        wip = wip.set(n as i128)?;
                    }
                    _ => {
                        // Handle isize on 32-bit platforms (size 4)
                        if shape.scalar_type() == Some(ScalarType::ISize) {
                            let v = isize::try_from(n).map_err(|_| {
                                JsonError::new(
                                    JsonErrorKind::NumberOutOfRange {
                                        value: n.to_string(),
                                        target_type: "isize",
                                    },
                                    span,
                                )
                            })?;
                            wip = wip.set(v)?;
                        } else {
                            return Err(JsonError::new(
                                JsonErrorKind::InvalidValue {
                                    message: format!("unexpected integer size: {size}"),
                                },
                                span,
                            ));
                        }
                    }
                }
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
                if n < 0 {
                    return Err(JsonError::new(
                        JsonErrorKind::NumberOutOfRange {
                            value: n.to_string(),
                            target_type: shape.type_identifier,
                        },
                        span,
                    ));
                }
                wip = self.set_number_u64(wip, n as u64, span)?;
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => match size {
                4 => {
                    wip = wip.set(n as f32)?;
                }
                8 => {
                    wip = wip.set(n as f64)?;
                }
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::InvalidValue {
                            message: format!("unexpected float size: {size}"),
                        },
                        span,
                    ));
                }
            },
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::TypeMismatch {
                        expected: shape.type_identifier,
                        got: "integer",
                    },
                    span,
                ));
            }
        }
        Ok(wip)
    }

    fn set_number_u64(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        n: u64,
        span: Span,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();
        let size = match shape.layout {
            ShapeLayout::Sized(layout) => layout.size(),
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::InvalidValue {
                        message: "unsized integer".into(),
                    },
                    span,
                ));
            }
        };

        match &shape.ty {
            Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
                match size {
                    1 => {
                        let v = u8::try_from(n).map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::NumberOutOfRange {
                                    value: n.to_string(),
                                    target_type: "u8",
                                },
                                span,
                            )
                        })?;
                        wip = wip.set(v)?;
                    }
                    2 => {
                        let v = u16::try_from(n).map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::NumberOutOfRange {
                                    value: n.to_string(),
                                    target_type: "u16",
                                },
                                span,
                            )
                        })?;
                        wip = wip.set(v)?;
                    }
                    4 => {
                        let v = u32::try_from(n).map_err(|_| {
                            JsonError::new(
                                JsonErrorKind::NumberOutOfRange {
                                    value: n.to_string(),
                                    target_type: "u32",
                                },
                                span,
                            )
                        })?;
                        wip = wip.set(v)?;
                    }
                    8 => {
                        // Check if the target is usize (which has size 8 on 64-bit)
                        if shape.scalar_type() == Some(ScalarType::USize) {
                            let v = usize::try_from(n).map_err(|_| {
                                JsonError::new(
                                    JsonErrorKind::NumberOutOfRange {
                                        value: n.to_string(),
                                        target_type: "usize",
                                    },
                                    span,
                                )
                            })?;
                            wip = wip.set(v)?;
                        } else {
                            wip = wip.set(n)?;
                        }
                    }
                    16 => {
                        wip = wip.set(n as u128)?;
                    }
                    _ => {
                        // Handle usize on 32-bit platforms (size 4)
                        if shape.scalar_type() == Some(ScalarType::USize) {
                            let v = usize::try_from(n).map_err(|_| {
                                JsonError::new(
                                    JsonErrorKind::NumberOutOfRange {
                                        value: n.to_string(),
                                        target_type: "usize",
                                    },
                                    span,
                                )
                            })?;
                            wip = wip.set(v)?;
                        } else {
                            return Err(JsonError::new(
                                JsonErrorKind::InvalidValue {
                                    message: format!("unexpected integer size: {size}"),
                                },
                                span,
                            ));
                        }
                    }
                }
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
                // Convert unsigned to signed if it fits
                wip = self.set_number_i64(wip, n as i64, span)?;
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => match size {
                4 => {
                    wip = wip.set(n as f32)?;
                }
                8 => {
                    wip = wip.set(n as f64)?;
                }
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::InvalidValue {
                            message: format!("unexpected float size: {size}"),
                        },
                        span,
                    ));
                }
            },
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::TypeMismatch {
                        expected: shape.type_identifier,
                        got: "unsigned integer",
                    },
                    span,
                ));
            }
        }
        Ok(wip)
    }

    fn set_number_i128(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        n: i128,
        span: Span,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();
        let size = match shape.layout {
            ShapeLayout::Sized(layout) => layout.size(),
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::InvalidValue {
                        message: "unsized integer".into(),
                    },
                    span,
                ));
            }
        };

        if size == 16 {
            wip = wip.set(n)?;
        } else {
            // Try to fit in smaller type
            if let Ok(n64) = i64::try_from(n) {
                wip = self.set_number_i64(wip, n64, span)?;
            } else {
                return Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange {
                        value: n.to_string(),
                        target_type: shape.type_identifier,
                    },
                    span,
                ));
            }
        }
        Ok(wip)
    }

    fn set_number_u128(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        n: u128,
        span: Span,
    ) -> Result<Partial<'input, BORROW>> {
        let shape = wip.shape();
        let size = match shape.layout {
            ShapeLayout::Sized(layout) => layout.size(),
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::InvalidValue {
                        message: "unsized integer".into(),
                    },
                    span,
                ));
            }
        };

        if size == 16 {
            wip = wip.set(n)?;
        } else {
            // Try to fit in smaller type
            if let Ok(n64) = u64::try_from(n) {
                wip = self.set_number_u64(wip, n64, span)?;
            } else {
                return Err(JsonError::new(
                    JsonErrorKind::NumberOutOfRange {
                        value: n.to_string(),
                        target_type: shape.type_identifier,
                    },
                    span,
                ));
            }
        }
        Ok(wip)
    }

    /// Deserialize a struct from a JSON object.
    fn deserialize_struct(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_struct: {}", wip.shape().type_identifier);

        // Get struct fields to check for flatten
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(s)) => s,
            _ => {
                return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: "expected struct type".into(),
                }));
            }
        };

        // Check if this struct has any flattened fields - if so, use the solver
        if Self::has_flatten_fields(struct_def) {
            return self.deserialize_struct_with_flatten(wip);
        }

        // Simple path: no flattened fields
        self.deserialize_struct_simple(wip)
    }

    /// Deserialize a struct without flattened fields (simple case).
    fn deserialize_struct_simple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        // Expect opening brace and track its span
        let open_token = self.next()?;
        let object_start_span = match open_token.token {
            Token::ObjectStart => open_token.span,
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", open_token.token),
                        expected: "'{'",
                    },
                    open_token.span,
                ));
            }
        };

        // Get struct fields
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(s)) => s,
            _ => {
                return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: "expected struct type".into(),
                }));
            }
        };

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];

        // Track the end of the object for error reporting
        #[allow(unused_assignments)]
        let mut object_end_span: Option<Span> = None;

        // Check if the struct has a default attribute (all missing fields use defaults)
        let struct_has_default = wip.shape().has_default_attr();
        // Check if the struct denies unknown fields
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Parse fields until closing brace
        loop {
            let token = self.peek()?;
            match &token.token {
                Token::ObjectEnd => {
                    let close_token = self.next()?; // consume the brace
                    object_end_span = Some(close_token.span);
                    break;
                }
                Token::String(_) => {
                    // Parse field name
                    let key_token = self.next()?;
                    let key = match key_token.token {
                        Token::String(s) => s,
                        _ => unreachable!(),
                    };
                    let _key_span = key_token.span;

                    // Expect colon
                    let colon = self.next()?;
                    if !matches!(colon.token, Token::Colon) {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", colon.token),
                                expected: "':'",
                            },
                            colon.span,
                        ));
                    }

                    // Find the field by name and index
                    let field_info = struct_def
                        .fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| f.name == key.as_ref());

                    if let Some((idx, field)) = field_info {
                        wip = wip.begin_field(field.name)?;
                        // Check if field has custom deserialization
                        if field.proxy_convert_in_fn().is_some() {
                            wip = wip.begin_custom_deserialization()?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end()?; // Calls deserialize_with function
                        } else {
                            wip = self.deserialize_into(wip)?;
                        }
                        wip = wip.end()?;
                        fields_set[idx] = true;
                    } else {
                        // Unknown field
                        if deny_unknown_fields {
                            let expected_fields: Vec<&'static str> =
                                struct_def.fields.iter().map(|f| f.name).collect();
                            let suggestion = find_similar_field(&key, &expected_fields);
                            return Err(JsonError::new(
                                JsonErrorKind::UnknownField {
                                    field: key.into_owned(),
                                    expected: expected_fields,
                                    suggestion,
                                },
                                _key_span,
                            ));
                        }
                        log::trace!("skipping unknown field: {key}");
                        self.skip_value()?;
                    }

                    // Check for comma or end
                    let next = self.peek()?;
                    if matches!(next.token, Token::Comma) {
                        self.next()?; // consume comma
                    }
                }
                _ => {
                    let span = token.span;
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", token.token),
                            expected: "field name or '}'",
                        },
                        span,
                    ));
                }
            }
        }

        // Apply defaults for missing fields and detect required but missing fields
        for (idx, field) in struct_def.fields.iter().enumerate() {
            if fields_set[idx] {
                continue; // Field was already set from JSON
            }

            // Check if the field has a default available:
            // 1. Field has #[facet(default)] or #[facet(default = expr)]
            // 2. Struct has #[facet(default)] and field type implements Default
            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default {
                // Use set_nth_field_to_default which handles both default_fn and Default impl
                wip = wip.set_nth_field_to_default(idx)?;
            } else if struct_has_default && field_type_has_default {
                // Struct-level #[facet(default)] - use the field type's Default
                wip = wip.set_nth_field_to_default(idx)?;
            } else if field_is_option {
                // Option<T> fields should default to None even without struct-level defaults
                wip = wip.begin_field(field.name)?;
                wip = wip.set_default()?;
                wip = wip.end()?;
            } else {
                // Required field is missing - raise our own error with spans
                return Err(JsonError {
                    kind: JsonErrorKind::MissingField {
                        field: field.name,
                        object_start: Some(object_start_span),
                        object_end: object_end_span,
                    },
                    span: None, // We use custom labels instead
                    source_code: None,
                });
            }
        }

        Ok(wip)
    }

    /// Deserialize a struct with flattened fields using facet-solver.
    ///
    /// This uses a two-pass approach:
    /// 1. Peek mode: Scan all keys, feed to solver, record value positions
    /// 2. Deserialize: Use the resolved Configuration to deserialize with proper path handling
    fn deserialize_struct_with_flatten(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!(
            "deserialize_struct_with_flatten: {wip}",
            wip = wip.shape().type_identifier
        );

        // Build the schema for this type with auto-detection of enum representations
        // This respects #[facet(tag = "...", content = "...")] and #[facet(untagged)] attributes
        let schema = Schema::build_auto(wip.shape()).map_err(|e| {
            JsonError::without_span(JsonErrorKind::Solver(format!(
                "failed to build schema: {e}"
            )))
        })?;

        // Create the solver
        let mut solver = Solver::new(&schema);

        // Track where values start so we can re-read them in pass 2
        let mut field_positions: Vec<(Cow<'input, str>, usize)> = Vec::new();

        // Expect opening brace
        let token = self.next()?;
        match token.token {
            Token::ObjectStart => {}
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", token.token),
                        expected: "'{'",
                    },
                    token.span,
                ));
            }
        }

        // ========== PASS 1: Peek mode - scan all keys, feed to solver ==========
        loop {
            let token = self.peek()?;
            match &token.token {
                Token::ObjectEnd => {
                    self.next()?; // consume the brace
                    break;
                }
                Token::String(_) => {
                    // Parse field name
                    let key_token = self.next()?;
                    let key = match &key_token.token {
                        Token::String(s) => s.clone(),
                        _ => unreachable!(),
                    };

                    // Expect colon
                    let colon = self.next()?;
                    if !matches!(colon.token, Token::Colon) {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", colon.token),
                                expected: "':'",
                            },
                            colon.span,
                        ));
                    }

                    // Record the value position before skipping
                    let value_start = self.peek()?.span.offset;

                    // Feed key to solver (decision not used in peek mode)
                    let _decision = solver.see_key(key.clone());

                    field_positions.push((key, value_start));

                    // Skip the value
                    self.skip_value()?;

                    // Check for comma
                    let next = self.peek()?;
                    if matches!(next.token, Token::Comma) {
                        self.next()?;
                    }
                }
                _ => {
                    let span = token.span;
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", token.token),
                            expected: "field name or '}'",
                        },
                        span,
                    ));
                }
            }
        }

        // ========== Get the resolved Configuration ==========
        // Get seen keys before finish() consumes the solver
        let seen_keys = solver.seen_keys().clone();
        let config_handle = solver
            .finish()
            .map_err(|e| JsonError::without_span(JsonErrorKind::Solver(format!("{e}"))))?;
        let config = config_handle.resolution();

        // ========== PASS 2: Deserialize with proper path handling ==========
        // Sort fields by path depth (deepest first within each prefix group)
        // This ensures we set all fields at a given nesting level before closing it
        let mut fields_to_process: Vec<_> = field_positions
            .iter()
            .filter_map(|(key, offset)| config.field(key.as_ref()).map(|info| (info, *offset)))
            .collect();

        // Sort by path to group nested fields together
        // We want to process in an order that allows proper begin/end management
        fields_to_process.sort_by(|(a, _), (b, _)| a.path.segments().cmp(b.path.segments()));

        // Determine which optional fields are missing before PASS 2
        let missing_optional_fields: Vec<&FieldInfo> =
            config.missing_optional_fields(&seen_keys).collect();

        let mut defaults_by_first_segment: BTreeMap<&str, Vec<&FieldInfo>> = BTreeMap::new();
        for info in &missing_optional_fields {
            if let Some(PathSegment::Field(name)) = info.path.segments().first() {
                defaults_by_first_segment
                    .entry(name)
                    .or_default()
                    .push(*info);
            }
        }

        let processed_first_segments: BTreeSet<&str> = fields_to_process
            .iter()
            .filter_map(|(info, _)| {
                if let Some(PathSegment::Field(name)) = info.path.segments().first() {
                    Some(*name)
                } else {
                    None
                }
            })
            .collect();

        let missing_first_segments: BTreeSet<&str> =
            defaults_by_first_segment.keys().copied().collect();

        for first_field in &missing_first_segments {
            if processed_first_segments.contains(first_field) {
                continue;
            }

            wip = wip.begin_field(first_field)?;
            if matches!(wip.shape().def, Def::Option(_)) {
                wip = wip.set_default()?;
                defaults_by_first_segment.remove(first_field);
            }
            wip = wip.end()?;
        }

        // Track currently open path segments: (field_name, is_option)
        let mut open_segments: Vec<(&str, bool)> = Vec::new();

        for (field_info, offset) in &fields_to_process {
            let segments = field_info.path.segments();
            let offset = *offset;

            // Extract just the field names from the path (ignoring Variant segments for now)
            let field_segments: Vec<&str> = segments
                .iter()
                .filter_map(|s| match s {
                    PathSegment::Field(name) => Some(*name),
                    PathSegment::Variant(_, _) => None,
                })
                .collect();

            // Find common prefix with currently open segments
            let common_len = open_segments
                .iter()
                .zip(field_segments.iter())
                .take_while(|((name, _), b)| *name == **b)
                .count();

            // Close segments that are no longer needed (in reverse order)
            while open_segments.len() > common_len {
                let (segment_name, is_option) = open_segments.pop().unwrap();
                wip = self.apply_defaults_for_segment(
                    wip,
                    segment_name,
                    &mut defaults_by_first_segment,
                )?;
                if is_option {
                    wip = wip.end()?; // End the inner Some value
                }
                wip = wip.end()?; // End the field
            }

            // Open new segments
            for &segment in &field_segments[common_len..] {
                wip = wip.begin_field(segment)?;
                // Check if we just entered an Option field - if so, initialize it as Some
                let is_option = matches!(wip.shape().def, Def::Option(_));
                if is_option {
                    wip = wip.begin_some()?;
                }
                open_segments.push((segment, is_option));
            }

            // Handle variant selection if the path ends with a Variant segment
            let ends_with_variant = segments
                .last()
                .is_some_and(|s| matches!(s, PathSegment::Variant(_, _)));

            if ends_with_variant
                && let Some(PathSegment::Variant(_, variant_name)) = segments.last()
            {
                wip = wip.select_variant_named(variant_name)?;
            }

            // Create sub-deserializer and deserialize the value
            // Note: This requires the adapter to support at_offset (slice-based parsing).
            // For streaming adapters, flatten is not supported.
            let sub_adapter = self.adapter.at_offset(offset).ok_or_else(|| {
                JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: "flatten is not supported in streaming mode".into(),
                })
            })?;
            let mut sub = Self::from_adapter(sub_adapter);

            if ends_with_variant {
                wip = sub.deserialize_variant_struct_content(wip)?;
            } else {
                // Pop the last segment since we're about to deserialize into it
                // The deserialize_into will set the value directly
                if !open_segments.is_empty() {
                    let (segment_name, is_option) = open_segments.pop().unwrap();
                    wip = self.apply_defaults_for_segment(
                        wip,
                        segment_name,
                        &mut defaults_by_first_segment,
                    )?;
                    wip = sub.deserialize_into(wip)?;
                    wip = wip.end()?;
                    if is_option {
                        wip = wip.end()?; // End the Option field itself
                    }
                } else {
                    wip = sub.deserialize_into(wip)?;
                }
            }
        }

        // Close any remaining open segments
        while let Some((segment_name, is_option)) = open_segments.pop() {
            wip =
                self.apply_defaults_for_segment(wip, segment_name, &mut defaults_by_first_segment)?;
            if is_option {
                wip = wip.end()?; // End the inner Some value
            }
            wip = wip.end()?; // End the field
        }
        for infos in defaults_by_first_segment.into_values() {
            for info in infos {
                wip = self.set_missing_field_default(wip, info, false)?;
            }
        }

        Ok(wip)
    }

    /// Deserialize an enum.
    ///
    /// Supports externally tagged representation: `{"VariantName": data}` or `"UnitVariant"`
    fn deserialize_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_enum: {}", wip.shape().type_identifier);

        // Check if this is an untagged enum
        if wip.shape().is_untagged() {
            return self.deserialize_untagged_enum(wip);
        }

        let token = self.peek()?;

        match &token.token {
            // String = unit variant (externally tagged unit)
            Token::String(s) => {
                let variant_name = s.clone();
                self.next()?; // consume

                wip = wip.select_variant_named(&variant_name)?;
                // Unit variants don't need further deserialization
                Ok(wip)
            }
            // Object = externally tagged variant with data
            Token::ObjectStart => {
                self.next()?; // consume brace

                // Get the variant name (first key)
                let key_token = self.next()?;
                let key = match &key_token.token {
                    Token::String(s) => s.clone(),
                    Token::ObjectEnd => {
                        // Empty object - error
                        return Err(JsonError::new(
                            JsonErrorKind::InvalidValue {
                                message: "empty object cannot represent enum variant".into(),
                            },
                            key_token.span,
                        ));
                    }
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", key_token.token),
                                expected: "variant name",
                            },
                            key_token.span,
                        ));
                    }
                };

                // Expect colon
                let colon = self.next()?;
                if !matches!(colon.token, Token::Colon) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", colon.token),
                            expected: "':'",
                        },
                        colon.span,
                    ));
                }

                // Select the variant
                wip = wip.select_variant_named(&key)?;

                // Get the selected variant info to determine how to deserialize
                let variant = wip.selected_variant().ok_or_else(|| {
                    JsonError::without_span(JsonErrorKind::InvalidValue {
                        message: "failed to get selected variant".into(),
                    })
                })?;

                // Deserialize based on variant kind
                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant in object form like {"Unit": null}
                        // We should consume some token (null, empty object, etc.)
                        let tok = self.next()?;
                        if !matches!(tok.token, Token::Null) {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedToken {
                                    got: format!("{:?}", tok.token),
                                    expected: "null for unit variant",
                                },
                                tok.span,
                            ));
                        }
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        let num_fields = variant.data.fields.len();
                        if num_fields == 0 {
                            // Zero-field tuple variant, treat like unit
                            let tok = self.peek()?;
                            if matches!(tok.token, Token::Null) {
                                self.next()?;
                            }
                        } else if num_fields == 1 {
                            // Single-element tuple: value directly (e.g., {"X": 123})
                            let field = &variant.data.fields[0];
                            wip = wip.begin_nth_field(0)?;
                            // Check if field has custom deserialization
                            if field.proxy_convert_in_fn().is_some() {
                                wip = wip.begin_custom_deserialization()?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end()?; // Calls deserialize_with function
                            } else {
                                wip = self.deserialize_into(wip)?;
                            }
                            wip = wip.end()?;
                        } else {
                            // Multi-element tuple: array (e.g., {"Y": ["hello", true]})
                            let tok = self.next()?;
                            if !matches!(tok.token, Token::ArrayStart) {
                                return Err(JsonError::new(
                                    JsonErrorKind::UnexpectedToken {
                                        got: format!("{:?}", tok.token),
                                        expected: "'[' for tuple variant",
                                    },
                                    tok.span,
                                ));
                            }

                            for i in 0..num_fields {
                                let field = &variant.data.fields[i];
                                wip = wip.begin_nth_field(i)?;
                                // Check if field has custom deserialization
                                if field.proxy_convert_in_fn().is_some() {
                                    wip = wip.begin_custom_deserialization()?;
                                    wip = self.deserialize_into(wip)?;
                                    wip = wip.end()?; // Calls deserialize_with function
                                } else {
                                    wip = self.deserialize_into(wip)?;
                                }
                                wip = wip.end()?;

                                // Check for comma or closing bracket
                                let next = self.peek()?;
                                if matches!(next.token, Token::Comma) {
                                    self.next()?;
                                }
                            }

                            let close = self.next()?;
                            if !matches!(close.token, Token::ArrayEnd) {
                                return Err(JsonError::new(
                                    JsonErrorKind::UnexpectedToken {
                                        got: format!("{:?}", close.token),
                                        expected: "']'",
                                    },
                                    close.span,
                                ));
                            }
                        }
                    }
                    StructKind::Struct => {
                        // Struct variant: object with named fields
                        wip = self.deserialize_variant_struct_content(wip)?;
                    }
                }

                // Expect closing brace for the outer object
                let close = self.next()?;
                if !matches!(close.token, Token::ObjectEnd) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", close.token),
                            expected: "'}'",
                        },
                        close.span,
                    ));
                }

                Ok(wip)
            }
            _ => {
                let span = token.span;
                Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", token.token),
                        expected: "string or object for enum",
                    },
                    span,
                ))
            }
        }
    }

    /// Deserialize an untagged enum using the Solver to determine which variant matches.
    ///
    /// For untagged enums, we use facet-solver to:
    /// 1. Record the start position of the object
    /// 2. Scan all JSON keys, feed them to the solver to narrow down candidates
    /// 3. Use finish() to determine which variant's required fields are satisfied
    /// 4. Rewind to start position and deserialize the whole object into the matched variant
    fn deserialize_untagged_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_untagged_enum: {}", wip.shape().type_identifier);

        let shape = wip.shape();

        // Build schema - this creates one resolution per variant for untagged enums
        let schema = Schema::build_auto(shape).map_err(|e| {
            JsonError::without_span(JsonErrorKind::Solver(format!(
                "failed to build schema: {e}"
            )))
        })?;

        // Create the solver
        let mut solver = Solver::new(&schema);

        // Expect opening brace (struct variants) or handle other cases
        let token = self.peek()?;
        match &token.token {
            Token::ObjectStart => {
                // Record start position for rewinding after we determine the variant
                let start_offset = token.span.offset;

                self.next()?; // consume the brace

                // ========== PASS 1: Scan all keys, feed to solver ==========
                loop {
                    let token = self.peek()?;
                    match &token.token {
                        Token::ObjectEnd => {
                            self.next()?;
                            break;
                        }
                        Token::String(_) => {
                            let key_token = self.next()?;
                            let key = match &key_token.token {
                                Token::String(s) => s.clone(),
                                _ => unreachable!(),
                            };

                            let colon = self.next()?;
                            if !matches!(colon.token, Token::Colon) {
                                return Err(JsonError::new(
                                    JsonErrorKind::UnexpectedToken {
                                        got: format!("{:?}", colon.token),
                                        expected: "':'",
                                    },
                                    colon.span,
                                ));
                            }

                            // Feed key to solver
                            let _decision = solver.see_key(key);

                            // Skip the value
                            self.skip_value()?;

                            // Check for comma
                            let next = self.peek()?;
                            if matches!(next.token, Token::Comma) {
                                self.next()?;
                            }
                        }
                        _ => {
                            let span = token.span;
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedToken {
                                    got: format!("{:?}", token.token),
                                    expected: "field name or '}'",
                                },
                                span,
                            ));
                        }
                    }
                }

                // ========== Get the resolved variant ==========
                let config_handle = solver
                    .finish()
                    .map_err(|e| JsonError::without_span(JsonErrorKind::Solver(format!("{e}"))))?;
                let config = config_handle.resolution();

                // Extract the variant name from the resolution
                let variant_name = config
                    .variant_selections()
                    .first()
                    .map(|vs| vs.variant_name)
                    .ok_or_else(|| {
                        JsonError::without_span(JsonErrorKind::InvalidValue {
                            message: "solver returned resolution with no variant selection".into(),
                        })
                    })?;

                // Select the variant
                wip = wip.select_variant_named(variant_name)?;

                // ========== PASS 2: Rewind and deserialize ==========
                // Create a new deserializer at the start of the object
                let rewound_adapter = self.adapter.at_offset(start_offset).ok_or_else(|| {
                    JsonError::without_span(JsonErrorKind::InvalidValue {
                        message: "untagged enums not supported in streaming mode".into(),
                    })
                })?;
                let mut rewound_deser = Self::from_adapter(rewound_adapter);

                // Deserialize the object into the selected variant
                wip = rewound_deser.deserialize_variant_struct_content(wip)?;

                Ok(wip)
            }
            Token::ArrayStart => {
                // Tuple variants - match by arity
                self.deserialize_untagged_tuple_variant(wip, shape)
            }
            Token::Null => {
                // Unit variants - select the first unit variant
                self.deserialize_untagged_unit_variant(wip, shape)
            }
            Token::String(_)
            | Token::I64(_)
            | Token::U64(_)
            | Token::I128(_)
            | Token::U128(_)
            | Token::F64(_)
            | Token::True
            | Token::False => {
                // Scalar variants - select based on value type
                self.deserialize_untagged_scalar_variant(wip, shape)
            }
            _ => Err(JsonError::new(
                JsonErrorKind::InvalidValue {
                    message: format!("unexpected token {:?} for untagged enum", token.token),
                },
                token.span,
            )),
        }
    }

    /// Deserialize an untagged enum from a null value.
    /// Selects the first unit variant.
    fn deserialize_untagged_unit_variant(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>> {
        let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: "expected enum shape for untagged deserialization".into(),
            })
        })?;

        if variants_by_format.unit_variants.is_empty() {
            return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!(
                    "no unit variants in untagged enum {} for null value",
                    shape.type_identifier
                ),
            }));
        }

        // Consume the null token
        self.next()?;

        // Select the first unit variant (like serde does)
        let variant = variants_by_format.unit_variants[0];
        wip = wip.select_variant_named(variant.name)?;

        Ok(wip)
    }

    /// Deserialize an untagged enum from a scalar value (string, number, bool).
    /// Selects the variant based on the value type.
    fn deserialize_untagged_scalar_variant(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>> {
        let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: "expected enum shape for untagged deserialization".into(),
            })
        })?;

        // Check if we have a string token that matches a unit variant name
        let token = self.peek()?.clone();
        if let Token::String(ref s) = token.token {
            // Try to find a unit variant with this name
            for variant in &variants_by_format.unit_variants {
                if variant.name == s.as_ref() {
                    // This is a unit variant - consume the string and select it
                    self.next()?;
                    wip = wip.select_variant_named(variant.name)?;
                    return Ok(wip);
                }
            }
        }

        // Not a unit variant - fall back to newtype scalar variant handling
        if variants_by_format.scalar_variants.is_empty() {
            return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!(
                    "no scalar-accepting variants in untagged enum {}",
                    shape.type_identifier
                ),
            }));
        }

        // Select the variant based on the token type
        let variant_name = self.select_scalar_variant(&variants_by_format, &token)?;

        wip = wip.select_variant_named(variant_name)?;
        wip = wip.begin_nth_field(0)?;
        wip = self.deserialize_into(wip)?;
        wip = wip.end()?;

        Ok(wip)
    }

    /// Select which scalar variant to use based on the JSON token.
    fn select_scalar_variant(
        &self,
        variants: &VariantsByFormat,
        token: &SpannedAdapterToken,
    ) -> Result<&'static str> {
        // Sort by specificity (most specific first)
        let mut candidates: Vec<_> = variants.scalar_variants.clone();
        candidates.sort_by_key(|(_, inner_shape)| specificity_score(inner_shape));

        match &token.token {
            Token::True | Token::False => {
                // Find a bool variant
                for (variant, inner_shape) in &candidates {
                    if inner_shape.scalar_type() == Some(ScalarType::Bool) {
                        return Ok(variant.name);
                    }
                }
            }
            Token::I64(n) => {
                // Find the smallest integer type that fits
                let n = *n;
                for (variant, inner_shape) in &candidates {
                    let fits = match inner_shape.scalar_type() {
                        Some(ScalarType::U8) => n >= 0 && n <= u8::MAX as i64,
                        Some(ScalarType::U16) => n >= 0 && n <= u16::MAX as i64,
                        Some(ScalarType::U32) => n >= 0 && n <= u32::MAX as i64,
                        Some(ScalarType::U64) => n >= 0,
                        Some(ScalarType::I8) => n >= i8::MIN as i64 && n <= i8::MAX as i64,
                        Some(ScalarType::I16) => n >= i16::MIN as i64 && n <= i16::MAX as i64,
                        Some(ScalarType::I32) => n >= i32::MIN as i64 && n <= i32::MAX as i64,
                        Some(ScalarType::I64) => true,
                        Some(ScalarType::F32) | Some(ScalarType::F64) => true,
                        _ => false,
                    };
                    if fits {
                        return Ok(variant.name);
                    }
                }
            }
            Token::U64(n) => {
                let n = *n;
                for (variant, inner_shape) in &candidates {
                    let fits = match inner_shape.scalar_type() {
                        Some(ScalarType::U8) => n <= u8::MAX as u64,
                        Some(ScalarType::U16) => n <= u16::MAX as u64,
                        Some(ScalarType::U32) => n <= u32::MAX as u64,
                        Some(ScalarType::U64) => true,
                        Some(ScalarType::I8) => n <= i8::MAX as u64,
                        Some(ScalarType::I16) => n <= i16::MAX as u64,
                        Some(ScalarType::I32) => n <= i32::MAX as u64,
                        Some(ScalarType::I64) => n <= i64::MAX as u64,
                        Some(ScalarType::F32) | Some(ScalarType::F64) => true,
                        _ => false,
                    };
                    if fits {
                        return Ok(variant.name);
                    }
                }
            }
            Token::I128(n) => {
                let n = *n;
                for (variant, inner_shape) in &candidates {
                    let fits = match inner_shape.scalar_type() {
                        Some(ScalarType::I128) => true,
                        Some(ScalarType::U128) => n >= 0,
                        _ => false,
                    };
                    if fits {
                        return Ok(variant.name);
                    }
                }
            }
            Token::U128(n) => {
                let n = *n;
                for (variant, inner_shape) in &candidates {
                    let fits = match inner_shape.scalar_type() {
                        Some(ScalarType::U128) => true,
                        Some(ScalarType::I128) => n <= i128::MAX as u128,
                        _ => false,
                    };
                    if fits {
                        return Ok(variant.name);
                    }
                }
            }
            Token::F64(_) => {
                // Find a float variant
                for (variant, inner_shape) in &candidates {
                    if matches!(
                        inner_shape.scalar_type(),
                        Some(ScalarType::F32) | Some(ScalarType::F64)
                    ) {
                        return Ok(variant.name);
                    }
                }
            }
            Token::String(_) => {
                // Find a string-like variant
                for (variant, inner_shape) in &candidates {
                    if matches!(
                        inner_shape.scalar_type(),
                        Some(ScalarType::String) | Some(ScalarType::Str) | Some(ScalarType::CowStr)
                    ) || inner_shape.scalar_type().is_none()
                    {
                        return Ok(variant.name);
                    }
                }
            }
            _ => {}
        }

        // Fall back to the first scalar variant if no specific match
        if let Some((variant, _)) = candidates.first() {
            return Ok(variant.name);
        }

        Err(JsonError::new(
            JsonErrorKind::InvalidValue {
                message: format!("no matching scalar variant for token {:?}", token.token),
            },
            token.span,
        ))
    }

    /// Deserialize an untagged enum from an array (tuple variant).
    fn deserialize_untagged_tuple_variant(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>> {
        let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: "expected enum shape for untagged deserialization".into(),
            })
        })?;

        if variants_by_format.tuple_variants.is_empty() {
            return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!(
                    "no tuple variants in untagged enum {} for array value",
                    shape.type_identifier
                ),
            }));
        }

        // Record start position for rewinding
        let start_token = self.peek()?;
        let start_offset = start_token.span.offset;

        // Count the array elements
        self.next()?; // consume ArrayStart
        let mut arity = 0;
        loop {
            let token = self.peek()?;
            match &token.token {
                Token::ArrayEnd => {
                    self.next()?;
                    break;
                }
                _ => {
                    arity += 1;
                    self.skip_value()?;
                    // Skip comma if present
                    let next = self.peek()?;
                    if matches!(next.token, Token::Comma) {
                        self.next()?;
                    }
                }
            }
        }

        // Find variants with matching arity
        let matching_variants = variants_by_format.tuple_variants_with_arity(arity);
        if matching_variants.is_empty() {
            return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!(
                    "no tuple variant with arity {} in untagged enum {}",
                    arity, shape.type_identifier
                ),
            }));
        }

        // Select the first matching variant
        let variant = matching_variants[0];
        wip = wip.select_variant_named(variant.name)?;
        let is_newtype = variant.data.fields.len() == 1;

        // Rewind and deserialize
        let rewound_adapter = self.adapter.at_offset(start_offset).ok_or_else(|| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: "untagged tuple variants not supported in streaming mode".into(),
            })
        })?;
        let mut rewound_deser = Self::from_adapter(rewound_adapter);

        if is_newtype {
            // Deserialize the entire array into the inner tuple value
            wip = wip.begin_nth_field(0)?;
            wip = rewound_deser.deserialize_into(wip)?;
            wip = wip.end()?;
        } else {
            // Consume ArrayStart
            rewound_deser.next()?;

            // Deserialize each field
            for i in 0..arity {
                wip = wip.begin_nth_field(i)?;
                wip = rewound_deser.deserialize_into(wip)?;
                wip = wip.end()?;

                // Skip comma if present
                let next = rewound_deser.peek()?;
                if matches!(next.token, Token::Comma) {
                    rewound_deser.next()?;
                }
            }

            debug_assert_eq!(
                variant.data.fields.len(),
                arity,
                "tuple variant arity should match array length"
            );

            // Consume ArrayEnd
            rewound_deser.next()?;
        }

        Ok(wip)
    }

    /// Deserialize the content of an enum variant in a flattened context.
    /// Handles both struct variants and tuple variants.
    fn deserialize_variant_struct_content(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        // Check what kind of variant we have
        let variant = wip.selected_variant().ok_or_else(|| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: "no variant selected".into(),
            })
        })?;

        let is_struct_variant = variant
            .data
            .fields
            .first()
            .map(|f| !f.name.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or(true);

        if is_struct_variant {
            // Struct variant: {"field1": ..., "field2": ...}
            self.deserialize_variant_struct_fields(wip, variant.data.fields)
        } else if variant.data.fields.len() == 1 {
            // Single-element tuple variant: just the value (not wrapped)
            let field = &variant.data.fields[0];
            wip = wip.begin_nth_field(0)?;
            // Check if field has custom deserialization
            if field.proxy_convert_in_fn().is_some() {
                wip = wip.begin_custom_deserialization()?;
                wip = self.deserialize_into(wip)?;
                wip = wip.end()?;
            } else {
                wip = self.deserialize_into(wip)?;
            }
            wip = wip.end()?;
            Ok(wip)
        } else {
            // Multi-element tuple variant: [val1, val2, ...]
            self.deserialize_variant_tuple_fields(wip)
        }
    }

    /// Deserialize struct fields of a variant.
    fn deserialize_variant_struct_fields(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        fields: &[facet_core::Field],
    ) -> Result<Partial<'input, BORROW>> {
        let token = self.next()?;
        if !matches!(token.token, Token::ObjectStart) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "'{' for struct variant",
                },
                token.span,
            ));
        }

        loop {
            let token = self.peek()?;
            if matches!(token.token, Token::ObjectEnd) {
                self.next()?;
                break;
            }

            let key_token = self.next()?;
            let field_name = match &key_token.token {
                Token::String(s) => s.clone(),
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", key_token.token),
                            expected: "field name",
                        },
                        key_token.span,
                    ));
                }
            };

            let colon = self.next()?;
            if !matches!(colon.token, Token::Colon) {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", colon.token),
                        expected: "':'",
                    },
                    colon.span,
                ));
            }

            // Find the field in the variant's fields to check for custom deserialization
            let field_info = fields.iter().find(|f| f.name == field_name.as_ref());

            if let Some(field) = field_info {
                wip = wip.begin_field(field.name)?;
                // Check if field has custom deserialization
                if field.proxy_convert_in_fn().is_some() {
                    wip = wip.begin_custom_deserialization()?;
                    wip = self.deserialize_into(wip)?;
                    wip = wip.end()?; // Calls deserialize_with function
                } else {
                    wip = self.deserialize_into(wip)?;
                }
                wip = wip.end()?;
            } else {
                // Unknown field, skip its value
                self.skip_value()?;
            }

            let next = self.peek()?;
            if matches!(next.token, Token::Comma) {
                self.next()?;
            }
        }

        Ok(wip)
    }

    /// Deserialize tuple fields of a variant.
    fn deserialize_variant_tuple_fields(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        let token = self.next()?;
        if !matches!(token.token, Token::ArrayStart) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "'[' for tuple variant",
                },
                token.span,
            ));
        }

        let mut idx = 0;
        loop {
            let token = self.peek()?;
            if matches!(token.token, Token::ArrayEnd) {
                self.next()?;
                break;
            }

            // Deserialize into field "0", "1", "2", etc.
            let field_name = alloc::format!("{idx}");
            wip = wip.begin_field(&field_name)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;

            idx += 1;
            let next = self.peek()?;
            if matches!(next.token, Token::Comma) {
                self.next()?;
            }
        }

        Ok(wip)
    }

    /// Deserialize a list/Vec.
    fn deserialize_list(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_list");

        let token = self.next()?;
        if !matches!(token.token, Token::ArrayStart) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "'['",
                },
                token.span,
            ));
        }

        wip = wip.begin_list()?;

        loop {
            let token = self.peek()?;
            if matches!(token.token, Token::ArrayEnd) {
                self.next()?;
                break;
            }

            wip = wip.begin_list_item()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?; // End the list item frame

            let next = self.peek()?;
            if matches!(next.token, Token::Comma) {
                self.next()?;
            }
        }

        // Note: begin_list() does not push a frame, so we don't call end() here
        Ok(wip)
    }

    /// Deserialize a map.
    fn deserialize_map(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_map");

        let token = self.next()?;
        if !matches!(token.token, Token::ObjectStart) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "'{'",
                },
                token.span,
            ));
        }

        wip = wip.begin_map()?;

        loop {
            let token = self.peek()?;
            if matches!(token.token, Token::ObjectEnd) {
                self.next()?;
                break;
            }

            // Key
            let key_token = self.next()?;
            let key = match key_token.token {
                Token::String(s) => s,
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", key_token.token),
                            expected: "string key",
                        },
                        key_token.span,
                    ));
                }
            };

            // Colon
            let colon = self.next()?;
            if !matches!(colon.token, Token::Colon) {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", colon.token),
                        expected: "':'",
                    },
                    colon.span,
                ));
            }

            // Set key - begin_key pushes a frame for the key type
            wip = wip.begin_key()?;
            wip = self.deserialize_map_key(wip, key, key_token.span)?;
            wip = wip.end()?;

            // Value - begin_value pushes a frame
            wip = wip.begin_value()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;

            // Comma or end
            let next = self.peek()?;
            if matches!(next.token, Token::Comma) {
                self.next()?;
            }
        }

        // Note: begin_map() does not push a frame, so we don't call end() here
        Ok(wip)
    }

    /// Deserialize an Option.
    fn deserialize_option(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_option");

        let token = self.peek()?;
        if matches!(token.token, Token::Null) {
            self.next()?;
            wip = wip.set_default()?; // None
        } else {
            log::trace!("deserialize_option: calling begin_some");
            wip = wip.begin_some()?;
            log::trace!("deserialize_option: begin_some succeeded, calling deserialize_into");
            wip = self.deserialize_into(wip)?;
            log::trace!("deserialize_option: deserialize_into succeeded, calling end");
            wip = wip.end()?;
            log::trace!("deserialize_option: end succeeded");
        }
        Ok(wip)
    }

    /// Deserialize a smart pointer (Box, Arc, Rc) or reference (&str).
    fn deserialize_pointer(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_pointer");

        // Check what kind of pointer this is BEFORE calling begin_smart_ptr
        let (is_slice_pointer, is_reference, is_str_ref, is_cow_str) =
            if let Def::Pointer(ptr_def) = wip.shape().def {
                let is_slice = if let Some(pointee) = ptr_def.pointee() {
                    matches!(pointee.ty, Type::Sequence(SequenceType::Slice(_)))
                } else {
                    false
                };
                let is_ref = matches!(
                    ptr_def.known,
                    Some(KnownPointer::SharedReference | KnownPointer::ExclusiveReference)
                );
                // Special case: &str can be deserialized by borrowing from input
                let is_str_ref = matches!(ptr_def.known, Some(KnownPointer::SharedReference))
                    && ptr_def
                        .pointee()
                        .is_some_and(|p| p.type_identifier == "str");
                // Special case: Cow<str> can borrow or own depending on whether escaping was needed
                let is_cow_str = matches!(ptr_def.known, Some(KnownPointer::Cow))
                    && ptr_def
                        .pointee()
                        .is_some_and(|p| p.type_identifier == "str");
                (is_slice, is_ref, is_str_ref, is_cow_str)
            } else {
                (false, false, false, false)
            };

        // Special case: Cow<str> can be deserialized directly from string tokens
        // preserving borrowed/owned status based on whether escaping was needed
        if is_cow_str {
            let token = self.next()?;
            match token.token {
                Token::String(s) => {
                    // Zero-copy Cow<str>: preserve borrowed/owned status
                    wip = wip.set(s)?;
                    return Ok(wip);
                }
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", token.token),
                            expected: "string",
                        },
                        token.span,
                    ));
                }
            }
        }

        // Special case: &str can borrow directly from input if no escaping needed
        if is_str_ref {
            // In owned mode, we cannot borrow from input at all
            if !BORROW {
                return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: "cannot deserialize into &str when borrowing is disabled - use String or Cow<str> instead".into(),
                }));
            }
            let token = self.next()?;
            match token.token {
                Token::String(Cow::Borrowed(s)) => {
                    // Zero-copy: borrow directly from input
                    wip = wip.set(s)?;
                    return Ok(wip);
                }
                Token::String(Cow::Owned(_)) => {
                    return Err(JsonError::new(
                        JsonErrorKind::InvalidValue {
                            message: "cannot borrow &str from JSON string containing escape sequences - use String instead".into(),
                        },
                        token.span,
                    ));
                }
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", token.token),
                            expected: "string",
                        },
                        token.span,
                    ));
                }
            }
        }

        // Other references (&T, &mut T) cannot be deserialized - they require borrowing from
        // existing data, which isn't possible when deserializing from owned JSON
        if is_reference {
            return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!(
                    "cannot deserialize into reference type '{wip}' - references require borrowing from existing data",
                    wip = wip.shape().type_identifier
                ),
            }));
        }

        // For smart pointers, push_smart_ptr will handle:
        // - Sized pointees: allocates space for the inner type
        // - str pointee: allocates a String that gets converted to Box<str>/Arc<str>/Rc<str>
        // - [T] pointee: sets up a slice builder for Arc<[T]>/Box<[T]>/Rc<[T]>
        wip = wip.begin_smart_ptr()?;

        if is_slice_pointer {
            // This is a slice pointer like Arc<[T]> - deserialize as array
            let token = self.next()?;
            if !matches!(token.token, Token::ArrayStart) {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{:?}", token.token),
                        expected: "'['",
                    },
                    token.span,
                ));
            }

            // Peek to check for empty array
            let first = self.peek()?;
            if matches!(first.token, Token::ArrayEnd) {
                self.next()?; // consume the RBracket
                wip = wip.end()?;
                return Ok(wip);
            }

            // Deserialize elements
            loop {
                wip = wip.begin_list_item()?;
                wip = self.deserialize_into(wip)?;
                wip = wip.end()?;

                let next = self.next()?;
                match next.token {
                    Token::Comma => continue,
                    Token::ArrayEnd => break,
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", next.token),
                                expected: "',' or ']'",
                            },
                            next.span,
                        ));
                    }
                }
            }

            wip = wip.end()?;
            return Ok(wip);
        }

        // For non-slice pointers, deserialize the inner type directly
        wip = self.deserialize_into(wip)?;
        wip = wip.end()?;
        Ok(wip)
    }

    /// Deserialize a fixed-size array.
    fn deserialize_array(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_array");

        let token = self.next()?;
        if !matches!(token.token, Token::ArrayStart) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "'['",
                },
                token.span,
            ));
        }

        // Get array length from the Def
        let array_len = match &wip.shape().def {
            Def::Array(arr) => arr.n,
            _ => {
                return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: "expected array type".into(),
                }));
            }
        };

        // Deserialize each element by index
        for i in 0..array_len {
            if i > 0 {
                let comma = self.next()?;
                if !matches!(comma.token, Token::Comma) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", comma.token),
                            expected: "','",
                        },
                        comma.span,
                    ));
                }
            }

            wip = wip.begin_nth_field(i)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }

        let close = self.next()?;
        if !matches!(close.token, Token::ArrayEnd) {
            // If we got a comma, that means there are more elements than the fixed array can hold
            if matches!(close.token, Token::Comma) {
                return Err(JsonError::new(
                    JsonErrorKind::InvalidValue {
                        message: format!(
                            "Too many elements in array, maximum {array_len} elements"
                        ),
                    },
                    close.span,
                ));
            }
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", close.token),
                    expected: "']'",
                },
                close.span,
            ));
        }

        Ok(wip)
    }

    /// Deserialize a set.
    fn deserialize_set(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_set");

        let token = self.next()?;
        if !matches!(token.token, Token::ArrayStart) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "'['",
                },
                token.span,
            ));
        }

        wip = wip.begin_set()?;

        loop {
            let token = self.peek()?;
            if matches!(token.token, Token::ArrayEnd) {
                self.next()?;
                break;
            }

            wip = wip.begin_set_item()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?; // End the set item frame

            let next = self.peek()?;
            if matches!(next.token, Token::Comma) {
                self.next()?;
            }
        }

        // Note: begin_set() does not push a frame, so we don't call end() here
        Ok(wip)
    }

    /// Deserialize a tuple.
    fn deserialize_tuple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!("deserialize_tuple");

        let token = self.next()?;
        if !matches!(token.token, Token::ArrayStart) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", token.token),
                    expected: "'['",
                },
                token.span,
            ));
        }

        // Get tuple info from the struct definition
        let tuple_len = match &wip.shape().ty {
            Type::User(UserType::Struct(struct_def)) => struct_def.fields.len(),
            _ => {
                return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: "expected tuple type".into(),
                }));
            }
        };

        for i in 0..tuple_len {
            if i > 0 {
                let comma = self.next()?;
                if !matches!(comma.token, Token::Comma) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", comma.token),
                            expected: "','",
                        },
                        comma.span,
                    ));
                }
            }

            wip = wip.begin_nth_field(i)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }

        let close = self.next()?;
        if !matches!(close.token, Token::ArrayEnd) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", close.token),
                    expected: "']'",
                },
                close.span,
            ));
        }

        Ok(wip)
    }

    fn set_missing_field_default(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        field_info: &FieldInfo,
        skip_first_segment: bool,
    ) -> Result<Partial<'input, BORROW>> {
        log::trace!(
            "Initializing missing optional field '{}' via solver path {:?}",
            field_info.serialized_name,
            field_info.path
        );
        let segments = field_info.path.segments();
        if segments.is_empty() {
            return Self::apply_default_for_field(wip, field_info.field);
        }

        #[allow(unused_mut, unused_variables)]
        let mut guards: Vec<PathGuard> = Vec::new();

        for (idx, segment) in segments
            .iter()
            .take(segments.len().saturating_sub(1))
            .enumerate()
        {
            if skip_first_segment && idx == 0 {
                continue;
            }
            match segment {
                PathSegment::Field(name) => {
                    wip = wip.begin_field(name)?;
                    let is_option = matches!(wip.shape().def, Def::Option(_));
                    if is_option {
                        wip = wip.begin_some()?;
                    }
                    guards.push(PathGuard::Field {
                        had_option: is_option,
                    });
                }
                PathSegment::Variant(_, variant_name) => {
                    wip = wip.select_variant_named(variant_name)?;
                    guards.push(PathGuard::Variant);
                }
            }
        }

        wip = Self::apply_default_for_field(wip, field_info.field)?;

        while let Some(guard) = guards.pop() {
            match guard {
                PathGuard::Field { had_option } => {
                    if had_option {
                        wip = wip.end()?; // Close the inner Some value
                    }
                    wip = wip.end()?; // Close the field itself
                }
                PathGuard::Variant => {}
            }
        }

        Ok(wip)
    }

    fn apply_defaults_for_segment(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        segment_name: &str,
        defaults_by_first_segment: &mut BTreeMap<&str, Vec<&FieldInfo>>,
    ) -> Result<Partial<'input, BORROW>> {
        if let Some(entries) = defaults_by_first_segment.remove(segment_name) {
            for info in entries {
                wip = self.set_missing_field_default(wip, info, true)?;
            }
        }
        Ok(wip)
    }

    fn apply_default_for_field(
        mut wip: Partial<'input, BORROW>,
        target_field: &'static facet_core::Field,
    ) -> Result<Partial<'input, BORROW>> {
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                    message: format!(
                        "expected struct while setting default for field '{}'",
                        target_field.name
                    ),
                }));
            }
        };

        let Some(idx) = struct_def
            .fields
            .iter()
            .position(|field| ptr::eq(field, target_field))
        else {
            return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!(
                    "could not find field '{}' while applying default",
                    target_field.name
                ),
            }));
        };

        if target_field.has_default() {
            wip = wip.set_nth_field_to_default(idx)?;
        } else if matches!(target_field.shape().def, Def::Option(_)) {
            // Option<T> fields can always default to None.
            wip = wip.begin_nth_field(idx)?;
            wip = wip.set_default()?;
            wip = wip.end()?;
        } else {
            // Fall back to the field type's Default (this will error if unavailable).
            wip = wip.set_nth_field_to_default(idx)?;
        }

        Ok(wip)
    }
}

#[derive(Debug)]
enum PathGuard {
    Field { had_option: bool },
    Variant,
}

// ============================================================================
// Public API
// ============================================================================

/// Deserialize JSON from a byte slice into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead. For zero-copy deserialization into
/// borrowed types, use [`from_slice_borrowed`].
///
/// Note: For rich error diagnostics with source code display, prefer [`from_str`]
/// which can attach the source string to errors.
pub fn from_slice<T: Facet<'static>>(input: &[u8]) -> Result<T> {
    from_slice_inner(input, None)
}

/// Deserialize JSON from a UTF-8 string slice into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead. For zero-copy deserialization into
/// borrowed types, use [`from_str_borrowed`].
///
/// Errors from this function include source code context for rich diagnostic display
/// when using [`miette`]'s reporting features.
pub fn from_str<T: Facet<'static>>(input: &str) -> Result<T> {
    let input_bytes = input.as_bytes();

    // Handle BOM
    if input_bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return from_slice_inner(&input_bytes[3..], Some(&input[3..]));
    }
    from_slice_inner(input_bytes, Some(input))
}

/// Deserialize JSON from a byte slice, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_slice`] which doesn't have lifetime requirements.
///
/// Note: For rich error diagnostics with source code display, prefer [`from_str_borrowed`]
/// which can attach the source string to errors.
pub fn from_slice_borrowed<'input, 'facet, T: Facet<'facet>>(input: &'input [u8]) -> Result<T>
where
    'input: 'facet,
{
    from_slice_borrowed_inner(input, None)
}

/// Deserialize JSON from a UTF-8 string slice, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_str`] which doesn't have lifetime requirements.
///
/// Errors from this function include source code context for rich diagnostic display
/// when using [`miette`]'s reporting features.
pub fn from_str_borrowed<'input, 'facet, T: Facet<'facet>>(input: &'input str) -> Result<T>
where
    'input: 'facet,
{
    let input_bytes = input.as_bytes();

    // Handle BOM
    if input_bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return from_slice_borrowed_inner(&input_bytes[3..], Some(&input[3..]));
    }
    from_slice_borrowed_inner(input_bytes, Some(input))
}

fn from_slice_borrowed_inner<'input, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
    source: Option<&str>,
) -> Result<T>
where
    'input: 'facet,
{
    let mut deserializer = JsonDeserializer::new(input);
    let wip = Partial::alloc::<T>()?;

    let partial = match deserializer.deserialize_into(wip) {
        Ok(p) => p,
        Err(e) => return Err(attach_source_cold(e, source)),
    };

    // Check that we've consumed all input (no trailing data after the root value)
    let trailing = deserializer.peek()?;
    if !matches!(trailing.token, Token::Eof) {
        let mut err = JsonError::new(
            JsonErrorKind::UnexpectedToken {
                got: format!("{:?}", trailing.token),
                expected: "end of input",
            },
            trailing.span,
        );
        if let Some(src) = source {
            err.source_code = Some(src.to_string());
        }
        return Err(err);
    }

    // Build and materialize the Partial into the target type
    let heap_value = match partial.build() {
        Ok(v) => v,
        Err(e) => return Err(attach_source_cold(JsonError::from(e), source)),
    };

    match heap_value.materialize::<T>() {
        Ok(v) => Ok(v),
        Err(e) => Err(attach_source_cold(JsonError::from(e), source)),
    }
}

fn from_slice_inner<T: Facet<'static>>(input: &[u8], source: Option<&str>) -> Result<T> {
    // We need to work around the lifetime constraints in the deserialization machinery.
    // The deserializer and Partial are parameterized by 'input (the input slice lifetime),
    // but we want to produce a T: Facet<'static> that doesn't borrow from input.
    //
    // The approach: Use an inner function parameterized by 'input that does all the work,
    // then transmute the result back to the 'static lifetime we need.
    //
    // SAFETY: This is safe because:
    // 1. T: Facet<'static> guarantees the type T itself contains no borrowed data
    // 2. allow_borrow: false ensures we error before storing any borrowed references
    // 3. BORROW: false on Partial/HeapValue documents that no borrowing occurs
    // 4. The transmutes only affect phantom lifetime markers, not actual runtime data

    fn inner<'input, T: Facet<'static>>(input: &'input [u8], source: Option<&str>) -> Result<T> {
        let mut deserializer = JsonDeserializer::new_owned(input);

        // Allocate a Partial<'static, false> - owned mode, no borrowing allowed.
        // We transmute to Partial<'input, false> to work with the deserializer.
        // SAFETY: We're only changing the lifetime marker. The Partial<_, false> doesn't
        // store any 'input references because:
        // - BORROW=false documents no borrowed data
        // - allow_borrow=false on deserializer prevents runtime borrowing
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>()?,
            )
        };

        let partial = match deserializer.deserialize_into(wip) {
            Ok(p) => p,
            Err(e) => return Err(attach_source_cold(e, source)),
        };

        // Check that we've consumed all input (no trailing data after the root value)
        let trailing = deserializer.peek()?;
        if !matches!(trailing.token, Token::Eof) {
            let mut err = JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{:?}", trailing.token),
                    expected: "end of input",
                },
                trailing.span,
            );
            if let Some(src) = source {
                err.source_code = Some(src.to_string());
            }
            return Err(err);
        }

        // Build the Partial into a HeapValue
        let heap_value = match partial.build() {
            Ok(v) => v,
            Err(e) => return Err(attach_source_cold(JsonError::from(e), source)),
        };

        // Transmute HeapValue<'input, false> to HeapValue<'static, false> so we can materialize to T
        // SAFETY: The HeapValue contains no borrowed data:
        // - BORROW=false documents no borrowed data
        // - allow_borrow=false ensured this at runtime
        // The transmute only affects the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: facet_reflect::HeapValue<'static, false> = unsafe {
            core::mem::transmute::<
                facet_reflect::HeapValue<'input, false>,
                facet_reflect::HeapValue<'static, false>,
            >(heap_value)
        };

        match heap_value.materialize::<T>() {
            Ok(v) => Ok(v),
            Err(e) => Err(attach_source_cold(JsonError::from(e), source)),
        }
    }

    inner::<T>(input, source)
}
