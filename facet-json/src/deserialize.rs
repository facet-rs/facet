//! Recursive descent JSON deserializer using facet-reflect.

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{self, Display};

use alloc::collections::BTreeSet;

use facet_core::{
    Characteristic, Def, Facet, FieldFlags, KnownPointer, NumericType, PrimitiveType, SequenceType,
    Shape, ShapeLayout, StructKind, Type, UserType,
};
use facet_reflect::{Partial, ReflectError};
use facet_solver::{PathSegment, Schema, Solver};

use crate::span::{Span, Spanned};
use crate::tokenizer::{Token, TokenError, TokenErrorKind, Tokenizer};

/// Find the best matching field name from a list of expected fields.
/// Returns Some(suggestion) if a match with similarity >= 0.6 is found.
fn find_similar_field<'a>(unknown: &str, expected: &[&'a str]) -> Option<&'a str> {
    let mut best_match: Option<(&'a str, f64)> = None;

    for &candidate in expected {
        let similarity = strsim::jaro_winkler(unknown, candidate);
        if similarity >= 0.6 {
            if best_match.map_or(true, |(_, best_sim)| similarity > best_sim) {
                best_match = Some((candidate, similarity));
            }
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
                    start.start,
                    start.len,
                ));
            }
            if let Some(end) = object_end {
                labels.push(miette::LabeledSpan::new(
                    Some(format!("object ended without field `{field}`")),
                    end.start,
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
            span.start,
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

/// Specific error kinds for JSON deserialization
#[derive(Debug)]
pub enum JsonErrorKind {
    /// Tokenizer error
    Token(TokenErrorKind),
    /// Tokenizer error with type context (what type was being parsed)
    TokenWithContext {
        /// The underlying token error
        error: TokenErrorKind,
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
}

impl Display for JsonErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonErrorKind::Token(e) => write!(f, "{e}"),
            JsonErrorKind::TokenWithContext {
                error,
                expected_type,
            } => {
                write!(f, "{error} (while parsing {expected_type})")
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
        }
    }
}

impl JsonErrorKind {
    /// Get an error code for this kind of error.
    pub fn code(&self) -> &'static str {
        match self {
            JsonErrorKind::Token(_) => "json::token",
            JsonErrorKind::TokenWithContext { .. } => "json::token",
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
        }
    }

    /// Get a label describing where/what the error points to.
    pub fn label(&self) -> String {
        match self {
            JsonErrorKind::Token(e) => match e {
                TokenErrorKind::UnexpectedCharacter(c) => format!("unexpected '{c}'"),
                TokenErrorKind::UnexpectedEof(ctx) => format!("unexpected end of input {ctx}"),
                TokenErrorKind::InvalidUtf8(_) => "invalid UTF-8 here".into(),
                TokenErrorKind::NumberOutOfRange(_) => "number out of range".into(),
            },
            JsonErrorKind::TokenWithContext {
                error,
                expected_type,
            } => match error {
                TokenErrorKind::UnexpectedCharacter(c) => {
                    format!("unexpected '{c}', expected {expected_type}")
                }
                TokenErrorKind::UnexpectedEof(_) => {
                    format!("unexpected end of input, expected {expected_type}")
                }
                TokenErrorKind::InvalidUtf8(_) => "invalid UTF-8 here".into(),
                TokenErrorKind::NumberOutOfRange(_) => "number out of range".into(),
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
        }
    }
}

impl From<TokenError> for JsonError {
    fn from(err: TokenError) -> Self {
        JsonError {
            kind: JsonErrorKind::Token(err.kind),
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
// Helper Functions
// ============================================================================

/// Check if a shape represents `Spanned<T>`.
///
/// Returns `true` if the shape is a struct with exactly two fields:
/// - `value` (the inner value)
/// - `span` (for storing source location)
fn is_spanned_shape(shape: &Shape) -> bool {
    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        if struct_def.fields.len() == 2 {
            let has_value = struct_def.fields.iter().any(|f| f.name == "value");
            let has_span = struct_def.fields.iter().any(|f| f.name == "span");
            return has_value && has_span;
        }
    }
    false
}

// ============================================================================
// Deserializer
// ============================================================================

/// JSON deserializer using recursive descent.
pub struct JsonDeserializer<'input> {
    input: &'input [u8],
    tokenizer: Tokenizer<'input>,
    /// Peeked token (for lookahead)
    peeked: Option<Spanned<Token<'input>>>,
}

impl<'input> JsonDeserializer<'input> {
    /// Create a new deserializer for the given input.
    pub fn new(input: &'input [u8]) -> Self {
        JsonDeserializer {
            input,
            tokenizer: Tokenizer::new(input),
            peeked: None,
        }
    }

    /// Create a sub-deserializer starting from a specific byte offset.
    /// Used for replaying deferred values during flatten deserialization.
    fn from_offset(input: &'input [u8], offset: usize) -> Self {
        JsonDeserializer {
            input,
            tokenizer: Tokenizer::new(&input[offset..]),
            peeked: None,
        }
    }

    /// Peek at the next token without consuming it.
    fn peek(&mut self) -> Result<&Spanned<Token<'input>>> {
        if self.peeked.is_none() {
            self.peeked = Some(self.tokenizer.next_token()?);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    /// Consume and return the next token.
    fn next(&mut self) -> Result<Spanned<Token<'input>>> {
        if let Some(token) = self.peeked.take() {
            Ok(token)
        } else {
            Ok(self.tokenizer.next_token()?)
        }
    }

    /// Consume the next token with type context for better error messages.
    fn next_expecting(&mut self, expected_type: &'static str) -> Result<Spanned<Token<'input>>> {
        match self.next() {
            Ok(token) => Ok(token),
            Err(e) => {
                // If it's a plain token error, wrap it with context
                if let JsonErrorKind::Token(token_err) = e.kind {
                    Err(JsonError {
                        kind: JsonErrorKind::TokenWithContext {
                            error: token_err,
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
    fn expect(&mut self, _expected: &'static str) -> Result<Spanned<Token<'input>>> {
        let token = self.next()?;
        // For now, just return the token - caller validates
        Ok(token)
    }

    /// Skip a JSON value (for unknown fields).
    fn skip_value(&mut self) -> Result<Span> {
        let token = self.next()?;
        let start_span = token.span;

        match token.node {
            Token::LBrace => {
                // Skip object
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next()?;
                    match t.node {
                        Token::LBrace => depth += 1,
                        Token::RBrace => depth -= 1,
                        _ => {}
                    }
                }
                Ok(start_span)
            }
            Token::LBracket => {
                // Skip array
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next()?;
                    match t.node {
                        Token::LBracket => depth += 1,
                        Token::RBracket => depth -= 1,
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
                    got: format!("{}", token.node),
                    expected: "value",
                },
                token.span,
            )),
        }
    }

    /// Navigate a FieldPath from the solver and deserialize a value at that location.
    ///
    /// The FieldPath contains segments like:
    /// - `Field("name")` - navigate into struct field "name"
    /// - `Variant("field", "Variant")` - select enum variant (field already entered by prior Field segment)
    #[allow(dead_code)]
    fn deserialize_at_path(
        &mut self,
        wip: &mut Partial<'input>,
        field_info: &facet_solver::FieldInfo,
    ) -> Result<()> {
        let segments = field_info.path.segments();

        // Count only Field segments for the unwind depth
        // Variant segments select a variant but don't add to the stack
        let field_depth = segments
            .iter()
            .filter(|s| matches!(s, PathSegment::Field(_)))
            .count();

        // Check if the path ends with a Variant segment - this means we're deserializing
        // an externally tagged enum variant's content, not a regular field
        let ends_with_variant = segments
            .last()
            .is_some_and(|s| matches!(s, PathSegment::Variant(_, _)));

        // Navigate to the correct location
        for segment in segments {
            match segment {
                PathSegment::Field(name) => {
                    wip.begin_field(name)?;
                }
                PathSegment::Variant(_field_name, variant_name) => {
                    // Variant segment just selects the variant - the field was already
                    // entered by a prior Field segment. The field_name in the Variant
                    // segment is for display/debugging only.
                    wip.select_variant_named(variant_name)?;
                }
            }
        }

        // Deserialize the value at this location
        if ends_with_variant {
            // For externally tagged enum variants, after selecting the variant,
            // we need to deserialize the variant's content (struct fields).
            // The JSON value is the struct content, e.g., {"field1":"a","field2":"b"}
            self.deserialize_variant_struct_content(wip)?;
        } else {
            self.deserialize_into(wip)?;
        }

        // Unwind the stack (call end() only for Field segments)
        for _ in 0..field_depth {
            wip.end()?;
        }

        Ok(())
    }

    /// Check if a struct has any flattened fields.
    fn has_flatten_fields(struct_def: &facet_core::StructType) -> bool {
        struct_def
            .fields
            .iter()
            .any(|f| f.flags.contains(FieldFlags::FLATTEN))
    }

    /// Main deserialization entry point - deserialize into a Partial.
    pub fn deserialize_into(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        let shape = wip.shape();
        log::trace!("deserialize_into: shape={}", shape.type_identifier);

        // Check for Spanned<T> wrapper first
        if is_spanned_shape(shape) {
            return self.deserialize_spanned(wip);
        }

        // Check Def first for Option (which is also a Type::User::Enum)
        // Must come before the inner check since Option also has .inner() set
        if matches!(&shape.def, Def::Option(_)) {
            return self.deserialize_option(wip);
        }

        // Check for smart pointers (Box, Arc, Rc) BEFORE checking shape.inner,
        // since Arc<str>/Box<str>/etc have both .inner and Def::Pointer
        if matches!(&shape.def, Def::Pointer(_)) {
            return self.deserialize_pointer(wip);
        }

        // Check for transparent/inner wrapper types (like Bytes -> BytesMut, Utf8PathBuf, etc.)
        // These should deserialize as their inner type
        if shape.inner.is_some() {
            wip.begin_inner()?;
            self.deserialize_into(wip)?;
            wip.end()?;
            return Ok(());
        }

        // Check the Type - structs and enums are identified by Type, not Def
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                // Tuples are structs with StructKind::Tuple
                if struct_def.kind == StructKind::Tuple {
                    return self.deserialize_tuple(wip);
                }
                return self.deserialize_struct(wip);
            }
            Type::User(UserType::Enum(_)) => return self.deserialize_enum(wip),
            _ => {}
        }

        // Then check Def for containers and special types
        match &shape.def {
            Def::Scalar => self.deserialize_scalar(wip),
            Def::List(_) => self.deserialize_list(wip),
            Def::Map(_) => self.deserialize_map(wip),
            Def::Array(_) => self.deserialize_array(wip),
            Def::Set(_) => self.deserialize_set(wip),
            other => Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                message: format!("unsupported shape def: {other:?}"),
            })),
        }
    }

    /// Deserialize into a Spanned<T> wrapper.
    fn deserialize_spanned(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_spanned");

        // Peek to get the span of the value we're about to parse
        let value_span = self.peek()?.span;

        // Deserialize the inner value into the `value` field
        wip.begin_field("value")?;
        self.deserialize_into(wip)?;
        wip.end()?;

        // Set the span field
        wip.begin_field("span")?;
        // Span struct has offset and len fields
        wip.set_field("offset", value_span.start)?;
        wip.set_field("len", value_span.len)?;
        wip.end()?;

        Ok(())
    }

    /// Deserialize a scalar value.
    fn deserialize_scalar(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        let expected_type = wip.shape().type_identifier;
        let token = self.next_expecting(expected_type)?;
        log::trace!("deserialize_scalar: token={:?}", token.node);

        match token.node {
            Token::String(s) => {
                // Try parse_from_str first if the type supports it (e.g., chrono types)
                if wip.shape().vtable.parse.is_some() {
                    wip.parse_from_str(&s)?;
                } else if wip.shape().type_identifier == "Cow" {
                    // Zero-copy Cow<str>: preserve borrowed/owned status
                    wip.set(s)?;
                } else {
                    wip.set(s.into_owned())?;
                }
            }
            Token::True => {
                wip.set(true)?;
            }
            Token::False => {
                wip.set(false)?;
            }
            Token::Null => {
                // For scalars, null typically means default
                wip.set_default()?;
            }
            Token::F64(n) => {
                self.set_number_f64(wip, n, token.span)?;
            }
            Token::I64(n) => {
                self.set_number_i64(wip, n, token.span)?;
            }
            Token::U64(n) => {
                self.set_number_u64(wip, n, token.span)?;
            }
            Token::I128(n) => {
                self.set_number_i128(wip, n, token.span)?;
            }
            Token::U128(n) => {
                self.set_number_u128(wip, n, token.span)?;
            }
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{}", token.node),
                        expected: "scalar value",
                    },
                    token.span,
                ));
            }
        }
        Ok(())
    }

    /// Set a string value, handling &str, Cow<str>, and String appropriately.
    fn set_string_value(&mut self, wip: &mut Partial<'input>, s: Cow<'input, str>) -> Result<()> {
        let shape = wip.shape();

        // Check if target is &str (shared reference to str)
        if let Def::Pointer(ptr_def) = shape.def {
            if matches!(ptr_def.known, Some(KnownPointer::SharedReference))
                && ptr_def
                    .pointee()
                    .is_some_and(|p| p.type_identifier == "str")
            {
                match s {
                    Cow::Borrowed(borrowed) => {
                        wip.set(borrowed)?;
                        return Ok(());
                    }
                    Cow::Owned(_) => {
                        return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                            message: "cannot borrow &str from JSON string containing escape sequences - use String instead".into(),
                        }));
                    }
                }
            }
        }

        // Check if target is Cow<str>
        if shape.type_identifier == "Cow" {
            wip.set(s)?;
            return Ok(());
        }

        // Default: convert to owned String
        wip.set(s.into_owned())?;
        Ok(())
    }

    /// Set a numeric value, handling type conversions.
    fn set_number_f64(&mut self, wip: &mut Partial<'input>, n: f64, span: Span) -> Result<()> {
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
                        wip.set(n as f32)?;
                    }
                    8 => {
                        wip.set(n)?;
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
                    self.set_number_i64(wip, n as i64, span)?;
                } else {
                    self.set_number_u64(wip, n as u64, span)?;
                }
            }
        }
        Ok(())
    }

    fn set_number_i64(&mut self, wip: &mut Partial<'input>, n: i64, span: Span) -> Result<()> {
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
                        wip.set(v)?;
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
                        wip.set(v)?;
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
                        wip.set(v)?;
                    }
                    8 => {
                        wip.set(n)?;
                    }
                    16 => {
                        wip.set(n as i128)?;
                    }
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::InvalidValue {
                                message: format!("unexpected integer size: {size}"),
                            },
                            span,
                        ));
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
                self.set_number_u64(wip, n as u64, span)?;
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => match size {
                4 => {
                    wip.set(n as f32)?;
                }
                8 => {
                    wip.set(n as f64)?;
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
        Ok(())
    }

    fn set_number_u64(&mut self, wip: &mut Partial<'input>, n: u64, span: Span) -> Result<()> {
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
                        wip.set(v)?;
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
                        wip.set(v)?;
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
                        wip.set(v)?;
                    }
                    8 => {
                        wip.set(n)?;
                    }
                    16 => {
                        wip.set(n as u128)?;
                    }
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::InvalidValue {
                                message: format!("unexpected integer size: {size}"),
                            },
                            span,
                        ));
                    }
                }
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
                // Convert unsigned to signed if it fits
                self.set_number_i64(wip, n as i64, span)?;
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => match size {
                4 => {
                    wip.set(n as f32)?;
                }
                8 => {
                    wip.set(n as f64)?;
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
        Ok(())
    }

    fn set_number_i128(&mut self, wip: &mut Partial<'input>, n: i128, span: Span) -> Result<()> {
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
            wip.set(n)?;
        } else {
            // Try to fit in smaller type
            if let Ok(n64) = i64::try_from(n) {
                self.set_number_i64(wip, n64, span)?;
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
        Ok(())
    }

    fn set_number_u128(&mut self, wip: &mut Partial<'input>, n: u128, span: Span) -> Result<()> {
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
            wip.set(n)?;
        } else {
            // Try to fit in smaller type
            if let Ok(n64) = u64::try_from(n) {
                self.set_number_u64(wip, n64, span)?;
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
        Ok(())
    }

    /// Deserialize a struct from a JSON object.
    fn deserialize_struct(&mut self, wip: &mut Partial<'input>) -> Result<()> {
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
    fn deserialize_struct_simple(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        // Expect opening brace and track its span
        let open_token = self.next()?;
        let object_start_span = match open_token.node {
            Token::LBrace => open_token.span,
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{}", open_token.node),
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
        let mut object_end_span: Option<Span> = None;

        // Check if the struct has a default attribute (all missing fields use defaults)
        let struct_has_default = wip.shape().has_default_attr();
        // Check if the struct denies unknown fields
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Parse fields until closing brace
        loop {
            let token = self.peek()?;
            match &token.node {
                Token::RBrace => {
                    let close_token = self.next()?; // consume the brace
                    object_end_span = Some(close_token.span);
                    break;
                }
                Token::String(_) => {
                    // Parse field name
                    let key_token = self.next()?;
                    let key = match key_token.node {
                        Token::String(s) => s,
                        _ => unreachable!(),
                    };
                    let _key_span = key_token.span;

                    // Expect colon
                    let colon = self.next()?;
                    if !matches!(colon.node, Token::Colon) {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{}", colon.node),
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
                        wip.begin_field(field.name)?;
                        // Check if field has custom deserialization
                        if field.vtable.deserialize_with.is_some() {
                            wip.begin_custom_deserialization()?;
                            self.deserialize_into(wip)?;
                            wip.end()?; // Calls deserialize_with function
                        } else {
                            self.deserialize_into(wip)?;
                        }
                        wip.end()?;
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
                        log::trace!("skipping unknown field: {}", key);
                        self.skip_value()?;
                    }

                    // Check for comma or end
                    let next = self.peek()?;
                    if matches!(next.node, Token::Comma) {
                        self.next()?; // consume comma
                    }
                }
                _ => {
                    let span = token.span;
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", token.node),
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
            // 1. Field has FieldFlags::DEFAULT (explicit #[facet(default)] on field)
            // 2. Field has a default_fn in vtable
            // 3. Struct has #[facet(default)] and field type implements Default
            let field_has_default_flag = field.flags.contains(FieldFlags::DEFAULT);
            let field_has_default_fn = field.vtable.default_fn.is_some();
            let field_type_has_default = field.shape().is(Characteristic::Default);

            if field_has_default_fn || field_has_default_flag {
                // Use set_nth_field_to_default which handles both default_fn and Default impl
                wip.set_nth_field_to_default(idx)?;
            } else if struct_has_default && field_type_has_default {
                // Struct-level #[facet(default)] - use the field type's Default
                wip.set_nth_field_to_default(idx)?;
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

        Ok(())
    }

    /// Deserialize a struct with flattened fields using facet-solver.
    ///
    /// This uses a two-pass approach:
    /// 1. Peek mode: Scan all keys, feed to solver, record value positions
    /// 2. Deserialize: Use the resolved Configuration to deserialize with proper path handling
    fn deserialize_struct_with_flatten(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!(
            "deserialize_struct_with_flatten: {}",
            wip.shape().type_identifier
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
        let mut field_positions: Vec<(&'static str, usize)> = Vec::new();

        // Expect opening brace
        let token = self.next()?;
        match token.node {
            Token::LBrace => {}
            _ => {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{}", token.node),
                        expected: "'{'",
                    },
                    token.span,
                ));
            }
        }

        // ========== PASS 1: Peek mode - scan all keys, feed to solver ==========
        loop {
            let token = self.peek()?;
            match &token.node {
                Token::RBrace => {
                    self.next()?; // consume the brace
                    break;
                }
                Token::String(_) => {
                    // Parse field name
                    let key_token = self.next()?;
                    let key = match &key_token.node {
                        Token::String(s) => s.clone(),
                        _ => unreachable!(),
                    };

                    // Expect colon
                    let colon = self.next()?;
                    if !matches!(colon.node, Token::Colon) {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{}", colon.node),
                                expected: "':'",
                            },
                            colon.span,
                        ));
                    }

                    // Leak the key for 'static lifetime (fine for deserialization)
                    let key_static: &'static str = Box::leak(key.into_owned().into_boxed_str());

                    // Record the value position before skipping
                    let value_start = self.peek()?.span.start;

                    // Feed key to solver (decision not used in peek mode)
                    let _decision = solver.see_key(key_static);

                    field_positions.push((key_static, value_start));

                    // Skip the value
                    self.skip_value()?;

                    // Check for comma
                    let next = self.peek()?;
                    if matches!(next.node, Token::Comma) {
                        self.next()?;
                    }
                }
                _ => {
                    let span = token.span;
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", token.node),
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
        let config = solver
            .finish()
            .map_err(|e| JsonError::without_span(JsonErrorKind::Solver(format!("{e}"))))?;

        // ========== PASS 2: Deserialize with proper path handling ==========
        // Sort fields by path depth (deepest first within each prefix group)
        // This ensures we set all fields at a given nesting level before closing it
        let mut fields_to_process: Vec<_> = field_positions
            .iter()
            .filter_map(|(key, offset)| config.field(key).map(|info| (info, *offset)))
            .collect();

        // Sort by path to group nested fields together
        // We want to process in an order that allows proper begin/end management
        fields_to_process.sort_by(|(a, _), (b, _)| a.path.segments().cmp(b.path.segments()));

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
                let (_, is_option) = open_segments.pop().unwrap();
                if is_option {
                    wip.end()?; // End the inner Some value
                }
                wip.end()?; // End the field
            }

            // Open new segments
            for &segment in &field_segments[common_len..] {
                wip.begin_field(segment)?;
                // Check if we just entered an Option field - if so, initialize it as Some
                let is_option = matches!(wip.shape().def, Def::Option(_));
                if is_option {
                    wip.begin_some()?;
                }
                open_segments.push((segment, is_option));
            }

            // Handle variant selection if the path ends with a Variant segment
            let ends_with_variant = segments
                .last()
                .is_some_and(|s| matches!(s, PathSegment::Variant(_, _)));

            if ends_with_variant {
                if let Some(PathSegment::Variant(_, variant_name)) = segments.last() {
                    wip.select_variant_named(variant_name)?;
                }
            }

            // Create sub-deserializer and deserialize the value
            let mut sub = Self::from_offset(self.input, offset);

            if ends_with_variant {
                sub.deserialize_variant_struct_content(wip)?;
            } else {
                // Pop the last segment since we're about to deserialize into it
                // The deserialize_into will set the value directly
                if !open_segments.is_empty() {
                    let (_, is_option) = open_segments.pop().unwrap();
                    sub.deserialize_into(wip)?;
                    wip.end()?;
                    if is_option {
                        wip.end()?; // End the Option field itself
                    }
                } else {
                    sub.deserialize_into(wip)?;
                }
            }
        }

        // Close any remaining open segments
        while let Some((_, is_option)) = open_segments.pop() {
            if is_option {
                wip.end()?; // End the inner Some value
            }
            wip.end()?; // End the field
        }

        // Handle missing optional fields - for flattened Option<T> fields,
        // we need to set the Option to None when all inner fields are missing.
        //
        // Collect first field segments from fields we DID process
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

        // Collect first field segments from MISSING optional fields
        let missing_first_segments: BTreeSet<&str> = config
            .missing_optional_fields(&seen_keys)
            .filter_map(|info| {
                if let Some(PathSegment::Field(name)) = info.path.segments().first() {
                    Some(*name)
                } else {
                    None
                }
            })
            .collect();

        // For each missing first segment that we didn't process, check if it's Option
        for first_field in missing_first_segments {
            if processed_first_segments.contains(first_field) {
                // We processed some fields under this, so the field was already handled
                continue;
            }

            log::trace!(
                "setting default for flattened Option field: {}",
                first_field
            );

            wip.begin_field(first_field)?;
            if matches!(wip.shape().def, Def::Option(_)) {
                // This is a flattened Option field with ALL inner fields missing, set to None
                wip.set_default()?;
            }
            wip.end()?;
        }

        Ok(())
    }

    /// Deserialize an enum.
    ///
    /// Supports externally tagged representation: `{"VariantName": data}` or `"UnitVariant"`
    fn deserialize_enum(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_enum: {}", wip.shape().type_identifier);

        let token = self.peek()?;

        match &token.node {
            // String = unit variant (externally tagged unit)
            Token::String(s) => {
                let variant_name = s.clone();
                self.next()?; // consume

                wip.select_variant_named(&variant_name)?;
                // Unit variants don't need further deserialization
                Ok(())
            }
            // Object = externally tagged variant with data
            Token::LBrace => {
                self.next()?; // consume brace

                // Get the variant name (first key)
                let key_token = self.next()?;
                let key = match &key_token.node {
                    Token::String(s) => s.clone(),
                    Token::RBrace => {
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
                                got: format!("{}", key_token.node),
                                expected: "variant name",
                            },
                            key_token.span,
                        ));
                    }
                };

                // Expect colon
                let colon = self.next()?;
                if !matches!(colon.node, Token::Colon) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", colon.node),
                            expected: "':'",
                        },
                        colon.span,
                    ));
                }

                // Select the variant
                wip.select_variant_named(&key)?;

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
                        if !matches!(tok.node, Token::Null) {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedToken {
                                    got: format!("{}", tok.node),
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
                            if matches!(tok.node, Token::Null) {
                                self.next()?;
                            }
                        } else if num_fields == 1 {
                            // Single-element tuple: value directly (e.g., {"X": 123})
                            let field = &variant.data.fields[0];
                            wip.begin_nth_field(0)?;
                            // Check if field has custom deserialization
                            if field.vtable.deserialize_with.is_some() {
                                wip.begin_custom_deserialization()?;
                                self.deserialize_into(wip)?;
                                wip.end()?; // Calls deserialize_with function
                            } else {
                                self.deserialize_into(wip)?;
                            }
                            wip.end()?;
                        } else {
                            // Multi-element tuple: array (e.g., {"Y": ["hello", true]})
                            let tok = self.next()?;
                            if !matches!(tok.node, Token::LBracket) {
                                return Err(JsonError::new(
                                    JsonErrorKind::UnexpectedToken {
                                        got: format!("{}", tok.node),
                                        expected: "'[' for tuple variant",
                                    },
                                    tok.span,
                                ));
                            }

                            for i in 0..num_fields {
                                let field = &variant.data.fields[i];
                                wip.begin_nth_field(i)?;
                                // Check if field has custom deserialization
                                if field.vtable.deserialize_with.is_some() {
                                    wip.begin_custom_deserialization()?;
                                    self.deserialize_into(wip)?;
                                    wip.end()?; // Calls deserialize_with function
                                } else {
                                    self.deserialize_into(wip)?;
                                }
                                wip.end()?;

                                // Check for comma or closing bracket
                                let next = self.peek()?;
                                if matches!(next.node, Token::Comma) {
                                    self.next()?;
                                }
                            }

                            let close = self.next()?;
                            if !matches!(close.node, Token::RBracket) {
                                return Err(JsonError::new(
                                    JsonErrorKind::UnexpectedToken {
                                        got: format!("{}", close.node),
                                        expected: "']'",
                                    },
                                    close.span,
                                ));
                            }
                        }
                    }
                    StructKind::Struct => {
                        // Struct variant: object with named fields
                        self.deserialize_variant_struct_content(wip)?;
                    }
                }

                // Expect closing brace for the outer object
                let close = self.next()?;
                if !matches!(close.node, Token::RBrace) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", close.node),
                            expected: "'}'",
                        },
                        close.span,
                    ));
                }

                Ok(())
            }
            _ => {
                let span = token.span;
                Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{}", token.node),
                        expected: "string or object for enum",
                    },
                    span,
                ))
            }
        }
    }

    /// Deserialize the content of an enum variant in a flattened context.
    /// Handles both struct variants and tuple variants.
    fn deserialize_variant_struct_content(&mut self, wip: &mut Partial<'input>) -> Result<()> {
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
            wip.begin_nth_field(0)?;
            // Check if field has custom deserialization
            if field.vtable.deserialize_with.is_some() {
                wip.begin_custom_deserialization()?;
                self.deserialize_into(wip)?;
                wip.end()?;
            } else {
                self.deserialize_into(wip)?;
            }
            wip.end()?;
            Ok(())
        } else {
            // Multi-element tuple variant: [val1, val2, ...]
            self.deserialize_variant_tuple_fields(wip)
        }
    }

    /// Deserialize struct fields of a variant.
    fn deserialize_variant_struct_fields(
        &mut self,
        wip: &mut Partial<'input>,
        fields: &[facet_core::Field],
    ) -> Result<()> {
        let token = self.next()?;
        if !matches!(token.node, Token::LBrace) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", token.node),
                    expected: "'{' for struct variant",
                },
                token.span,
            ));
        }

        loop {
            let token = self.peek()?;
            if matches!(token.node, Token::RBrace) {
                self.next()?;
                break;
            }

            let key_token = self.next()?;
            let field_name = match &key_token.node {
                Token::String(s) => s.clone(),
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", key_token.node),
                            expected: "field name",
                        },
                        key_token.span,
                    ));
                }
            };

            let colon = self.next()?;
            if !matches!(colon.node, Token::Colon) {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{}", colon.node),
                        expected: "':'",
                    },
                    colon.span,
                ));
            }

            // Find the field in the variant's fields to check for custom deserialization
            let field_info = fields.iter().find(|f| f.name == field_name.as_ref());

            if let Some(field) = field_info {
                wip.begin_field(field.name)?;
                // Check if field has custom deserialization
                if field.vtable.deserialize_with.is_some() {
                    wip.begin_custom_deserialization()?;
                    self.deserialize_into(wip)?;
                    wip.end()?; // Calls deserialize_with function
                } else {
                    self.deserialize_into(wip)?;
                }
                wip.end()?;
            } else {
                // Unknown field, skip its value
                self.skip_value()?;
            }

            let next = self.peek()?;
            if matches!(next.node, Token::Comma) {
                self.next()?;
            }
        }

        Ok(())
    }

    /// Deserialize tuple fields of a variant.
    fn deserialize_variant_tuple_fields(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        let token = self.next()?;
        if !matches!(token.node, Token::LBracket) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", token.node),
                    expected: "'[' for tuple variant",
                },
                token.span,
            ));
        }

        let mut idx = 0;
        loop {
            let token = self.peek()?;
            if matches!(token.node, Token::RBracket) {
                self.next()?;
                break;
            }

            // Deserialize into field "0", "1", "2", etc.
            let field_name = alloc::format!("{}", idx);
            wip.begin_field(&field_name)?;
            self.deserialize_into(wip)?;
            wip.end()?;

            idx += 1;
            let next = self.peek()?;
            if matches!(next.node, Token::Comma) {
                self.next()?;
            }
        }

        Ok(())
    }

    /// Deserialize a list/Vec.
    fn deserialize_list(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_list");

        let token = self.next()?;
        if !matches!(token.node, Token::LBracket) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", token.node),
                    expected: "'['",
                },
                token.span,
            ));
        }

        wip.begin_list()?;

        loop {
            let token = self.peek()?;
            if matches!(token.node, Token::RBracket) {
                self.next()?;
                break;
            }

            wip.begin_list_item()?;
            self.deserialize_into(wip)?;
            wip.end()?; // End the list item frame

            let next = self.peek()?;
            if matches!(next.node, Token::Comma) {
                self.next()?;
            }
        }

        // Note: begin_list() does not push a frame, so we don't call end() here
        Ok(())
    }

    /// Deserialize a map.
    fn deserialize_map(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_map");

        let token = self.next()?;
        if !matches!(token.node, Token::LBrace) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", token.node),
                    expected: "'{'",
                },
                token.span,
            ));
        }

        wip.begin_map()?;

        loop {
            let token = self.peek()?;
            if matches!(token.node, Token::RBrace) {
                self.next()?;
                break;
            }

            // Key
            let key_token = self.next()?;
            let key = match key_token.node {
                Token::String(s) => s,
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", key_token.node),
                            expected: "string key",
                        },
                        key_token.span,
                    ));
                }
            };

            // Colon
            let colon = self.next()?;
            if !matches!(colon.node, Token::Colon) {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{}", colon.node),
                        expected: "':'",
                    },
                    colon.span,
                ));
            }

            // Set key - begin_key pushes a frame for the key type
            wip.begin_key()?;
            // For transparent types (like UserId(String)), we need to use begin_inner
            // to set the inner String value
            if wip.shape().inner.is_some() {
                wip.begin_inner()?;
                self.set_string_value(wip, key)?;
                wip.end()?;
            } else {
                self.set_string_value(wip, key)?;
            }
            wip.end()?;

            // Value - begin_value pushes a frame
            wip.begin_value()?;
            self.deserialize_into(wip)?;
            wip.end()?;

            // Comma or end
            let next = self.peek()?;
            if matches!(next.node, Token::Comma) {
                self.next()?;
            }
        }

        // Note: begin_map() does not push a frame, so we don't call end() here
        Ok(())
    }

    /// Deserialize an Option.
    fn deserialize_option(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_option");

        let token = self.peek()?;
        if matches!(token.node, Token::Null) {
            self.next()?;
            wip.set_default()?; // None
        } else {
            wip.begin_some()?;
            self.deserialize_into(wip)?;
            wip.end()?;
        }
        Ok(())
    }

    /// Deserialize a smart pointer (Box, Arc, Rc) or reference (&str).
    fn deserialize_pointer(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_pointer");

        // Check what kind of pointer this is BEFORE calling begin_smart_ptr
        let (is_slice_pointer, is_reference, is_str_ref) =
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
                (is_slice, is_ref, is_str_ref)
            } else {
                (false, false, false)
            };

        // Special case: &str can borrow directly from input if no escaping needed
        if is_str_ref {
            let token = self.next()?;
            match token.node {
                Token::String(Cow::Borrowed(s)) => {
                    // Zero-copy: borrow directly from input
                    wip.set(s)?;
                    return Ok(());
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
                            got: format!("{}", token.node),
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
                    "cannot deserialize into reference type '{}' - references require borrowing from existing data",
                    wip.shape().type_identifier
                ),
            }));
        }

        // For smart pointers, push_smart_ptr will handle:
        // - Sized pointees: allocates space for the inner type
        // - str pointee: allocates a String that gets converted to Box<str>/Arc<str>/Rc<str>
        // - [T] pointee: sets up a slice builder for Arc<[T]>/Box<[T]>/Rc<[T]>
        wip.begin_smart_ptr()?;

        if is_slice_pointer {
            // This is a slice pointer like Arc<[T]> - deserialize as array
            let token = self.next()?;
            if !matches!(token.node, Token::LBracket) {
                return Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: format!("{}", token.node),
                        expected: "'['",
                    },
                    token.span,
                ));
            }

            // Peek to check for empty array
            let first = self.peek()?;
            if matches!(first.node, Token::RBracket) {
                self.next()?; // consume the RBracket
                wip.end()?;
                return Ok(());
            }

            // Deserialize elements
            loop {
                wip.begin_list_item()?;
                self.deserialize_into(wip)?;
                wip.end()?;

                let next = self.next()?;
                match next.node {
                    Token::Comma => continue,
                    Token::RBracket => break,
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{}", next.node),
                                expected: "',' or ']'",
                            },
                            next.span,
                        ));
                    }
                }
            }

            wip.end()?;
            return Ok(());
        }

        // For non-slice pointers, deserialize the inner type directly
        self.deserialize_into(wip)?;
        wip.end()?;
        Ok(())
    }

    /// Deserialize a fixed-size array.
    fn deserialize_array(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_array");

        let token = self.next()?;
        if !matches!(token.node, Token::LBracket) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", token.node),
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
                if !matches!(comma.node, Token::Comma) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", comma.node),
                            expected: "','",
                        },
                        comma.span,
                    ));
                }
            }

            wip.begin_nth_field(i)?;
            self.deserialize_into(wip)?;
            wip.end()?;
        }

        let close = self.next()?;
        if !matches!(close.node, Token::RBracket) {
            // If we got a comma, that means there are more elements than the fixed array can hold
            if matches!(close.node, Token::Comma) {
                return Err(JsonError::new(
                    JsonErrorKind::InvalidValue {
                        message: format!(
                            "Too many elements in array, maximum {} elements",
                            array_len
                        ),
                    },
                    close.span,
                ));
            }
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", close.node),
                    expected: "']'",
                },
                close.span,
            ));
        }

        Ok(())
    }

    /// Deserialize a set.
    fn deserialize_set(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_set");

        let token = self.next()?;
        if !matches!(token.node, Token::LBracket) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", token.node),
                    expected: "'['",
                },
                token.span,
            ));
        }

        wip.begin_set()?;

        loop {
            let token = self.peek()?;
            if matches!(token.node, Token::RBracket) {
                self.next()?;
                break;
            }

            wip.begin_set_item()?;
            self.deserialize_into(wip)?;
            wip.end()?; // End the set item frame

            let next = self.peek()?;
            if matches!(next.node, Token::Comma) {
                self.next()?;
            }
        }

        // Note: begin_set() does not push a frame, so we don't call end() here
        Ok(())
    }

    /// Deserialize a tuple.
    fn deserialize_tuple(&mut self, wip: &mut Partial<'input>) -> Result<()> {
        log::trace!("deserialize_tuple");

        let token = self.next()?;
        if !matches!(token.node, Token::LBracket) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", token.node),
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
                if !matches!(comma.node, Token::Comma) {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{}", comma.node),
                            expected: "','",
                        },
                        comma.span,
                    ));
                }
            }

            wip.begin_nth_field(i)?;
            self.deserialize_into(wip)?;
            wip.end()?;
        }

        let close = self.next()?;
        if !matches!(close.node, Token::RBracket) {
            return Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: format!("{}", close.node),
                    expected: "']'",
                },
                close.span,
            ));
        }

        Ok(())
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Deserialize JSON from a byte slice.
///
/// Note: For rich error diagnostics with source code display, prefer [`from_str`]
/// which can attach the source string to errors.
pub fn from_slice<'input, 'facet, T: Facet<'facet>>(input: &'input [u8]) -> Result<T>
where
    'input: 'facet,
{
    from_slice_inner(input, None)
}

/// Deserialize JSON from a UTF-8 string slice.
///
/// Errors from this function include source code context for rich diagnostic display
/// when using [`miette`]'s reporting features.
pub fn from_str<'input, 'facet, T: Facet<'facet>>(input: &'input str) -> Result<T>
where
    'input: 'facet,
{
    let input_bytes = input.as_bytes();

    // Handle BOM
    if input_bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return from_slice_inner(&input_bytes[3..], Some(&input[3..]));
    }
    from_slice_inner(input_bytes, Some(input))
}

fn from_slice_inner<'input, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
    source: Option<&str>,
) -> Result<T>
where
    'input: 'facet,
{
    let mut deserializer = JsonDeserializer::new(input);
    let mut wip = Partial::alloc::<T>()?;

    let result = deserializer.deserialize_into(wip.inner_mut());
    if let Err(mut e) = result {
        if let Some(src) = source {
            e.source_code = Some(src.to_string());
        }
        return Err(e);
    }

    // Check that we've consumed all input (no trailing data after the root value)
    let trailing = deserializer.peek()?;
    if !matches!(trailing.node, Token::Eof) {
        let mut err = JsonError::new(
            JsonErrorKind::UnexpectedToken {
                got: format!("{}", trailing.node),
                expected: "end of input",
            },
            trailing.span,
        );
        if let Some(src) = source {
            err.source_code = Some(src.to_string());
        }
        return Err(err);
    }

    wip.build().map(|b| *b).map_err(|e| {
        let mut err = JsonError::from(e);
        if let Some(src) = source {
            err.source_code = Some(src.to_string());
        }
        err
    })
}
