//! YAML deserialization using saphyr-parser streaming events.
//!
//! This deserializer uses saphyr-parser's event-based API similar to how
//! facet-json uses a tokenizer - processing events on-demand and supporting
//! rewind via event indices for flatten deserialization.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use facet_core::{
    Characteristic, Def, Facet, Field, FieldFlags, NumericType, PrimitiveType, ShapeLayout,
    StructKind, Type, UserType,
};
use facet_reflect::Partial;
use saphyr_parser::{Event, Parser, ScalarStyle, Span as SaphyrSpan, SpannedEventReceiver};

use crate::error::{SpanExt, YamlError, YamlErrorKind};
use facet_reflect::{Span, is_spanned_shape};

type Result<T> = core::result::Result<T, YamlError>;

// ============================================================================
// Public API
// ============================================================================

/// Deserialize a YAML string into a value of type `T`.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_yaml::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let yaml = "name: myapp\nport: 8080";
/// let config: Config = from_str(yaml).unwrap();
/// assert_eq!(config.name, "myapp");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_str<'input, 'facet, T>(yaml: &'input str) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!(
        "from_str: parsing YAML for type {}",
        core::any::type_name::<T>()
    );

    let mut deserializer = YamlDeserializer::new(yaml)?;
    let mut typed_partial = Partial::alloc::<T>()?;

    {
        let partial = typed_partial.inner_mut();
        deserializer.deserialize_document(partial)?;
    }

    // Check we consumed everything meaningful
    deserializer.expect_end()?;

    let result = typed_partial
        .build()
        .map_err(|e| YamlError::without_span(YamlErrorKind::Reflect(e)).with_source(yaml))?;

    Ok(*result)
}

// ============================================================================
// Event wrapper with owned strings
// ============================================================================

/// A YAML event with owned string data and span information.
/// We convert from saphyr's borrowed events to owned so we can store them.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some variants/fields reserved for future anchor/alias support
enum OwnedEvent {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    Alias(usize),
    Scalar {
        value: String,
        style: ScalarStyle,
        anchor: usize,
    },
    SequenceStart {
        anchor: usize,
    },
    SequenceEnd,
    MappingStart {
        anchor: usize,
    },
    MappingEnd,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // offset reserved for future flatten/rewind support
struct SpannedEvent {
    event: OwnedEvent,
    span: SaphyrSpan,
    /// Byte offset in the original input where this event's content starts.
    /// Used for rewind/replay during flatten deserialization.
    offset: usize,
}

// ============================================================================
// Event Collector
// ============================================================================

/// Collects all events from the parser upfront.
/// This is necessary because saphyr-parser doesn't support seeking/rewinding,
/// but we need to replay events for flatten deserialization.
struct EventCollector {
    events: Vec<SpannedEvent>,
}

impl EventCollector {
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl SpannedEventReceiver<'_> for EventCollector {
    fn on_event(&mut self, event: Event<'_>, span: SaphyrSpan) {
        let offset = span.start.index();
        let owned = match event {
            Event::StreamStart => OwnedEvent::StreamStart,
            Event::StreamEnd => OwnedEvent::StreamEnd,
            Event::DocumentStart(_) => OwnedEvent::DocumentStart,
            Event::DocumentEnd => OwnedEvent::DocumentEnd,
            Event::Alias(id) => OwnedEvent::Alias(id),
            Event::Scalar(value, style, anchor, _tag) => OwnedEvent::Scalar {
                value: value.into_owned(),
                style,
                anchor,
            },
            Event::SequenceStart(anchor, _tag) => OwnedEvent::SequenceStart { anchor },
            Event::SequenceEnd => OwnedEvent::SequenceEnd,
            Event::MappingStart(anchor, _tag) => OwnedEvent::MappingStart { anchor },
            Event::MappingEnd => OwnedEvent::MappingEnd,
            Event::Nothing => return, // Skip internal events
        };
        log::trace!("YAML event: {owned:?} at offset {offset}");
        self.events.push(SpannedEvent {
            event: owned,
            span,
            offset,
        });
    }
}

// ============================================================================
// Deserializer
// ============================================================================

/// YAML deserializer that processes events from a collected event stream.
struct YamlDeserializer<'input> {
    input: &'input str,
    events: Vec<SpannedEvent>,
    pos: usize,
}

impl<'input> YamlDeserializer<'input> {
    /// Create a new deserializer by parsing the input and collecting all events.
    fn new(input: &'input str) -> Result<Self> {
        let mut collector = EventCollector::new();
        Parser::new_from_str(input)
            .load(&mut collector, true)
            .map_err(|e| {
                YamlError::without_span(YamlErrorKind::Parse(format!("{e}"))).with_source(input)
            })?;

        Ok(Self {
            input,
            events: collector.events,
            pos: 0,
        })
    }

    /// Create a sub-deserializer starting from a specific event index.
    /// Used for replaying events during flatten deserialization.
    #[allow(dead_code)]
    fn from_position(input: &'input str, events: Vec<SpannedEvent>, pos: usize) -> Self {
        Self { input, events, pos }
    }

    /// Peek at the current event without consuming it.
    fn peek(&self) -> Option<&SpannedEvent> {
        self.events.get(self.pos)
    }

    /// Consume and return the current event.
    fn next(&mut self) -> Option<&SpannedEvent> {
        if self.pos < self.events.len() {
            let event = &self.events[self.pos];
            self.pos += 1;
            Some(event)
        } else {
            None
        }
    }

    /// Get the current position (event index) for later replay.
    #[allow(dead_code)]
    fn position(&self) -> usize {
        self.pos
    }

    /// Clone events for creating sub-deserializers.
    #[allow(dead_code)]
    fn clone_events(&self) -> Vec<SpannedEvent> {
        self.events.clone()
    }

    /// Get the span of the current position.
    fn current_span(&self) -> Span {
        self.peek()
            .map(|e| Span::from_saphyr_span(&e.span))
            .unwrap_or(Span::new(self.input.len(), 0))
    }

    /// Create an error with the current span and attach source.
    fn error(&self, kind: YamlErrorKind) -> YamlError {
        YamlError::new(kind, self.current_span()).with_source(self.input)
    }

    /// Consume and return the next event, or return an EOF error.
    fn next_or_eof(&mut self, expected: &'static str) -> Result<&SpannedEvent> {
        // Compute the error span before mutating self
        let err_span = self.current_span();
        let input = self.input;
        self.next().ok_or_else(|| {
            YamlError::new(YamlErrorKind::UnexpectedEof { expected }, err_span).with_source(input)
        })
    }

    /// Check that we've reached the end of meaningful content.
    fn expect_end(&mut self) -> Result<()> {
        // Skip DocumentEnd and StreamEnd
        while let Some(event) = self.peek() {
            match &event.event {
                OwnedEvent::DocumentEnd | OwnedEvent::StreamEnd => {
                    self.next();
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// Deserialize a YAML document.
    fn deserialize_document<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_document: shape = {}", partial.shape());

        // Skip StreamStart and DocumentStart
        while let Some(event) = self.peek() {
            match &event.event {
                OwnedEvent::StreamStart | OwnedEvent::DocumentStart => {
                    self.next();
                }
                _ => break,
            }
        }

        // Deserialize the main value
        self.deserialize_value(partial)
    }

    /// Main deserialization dispatch based on shape.
    fn deserialize_value<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        let shape = partial.shape();
        log::trace!(
            "deserialize_value: shape = {}, path = {}",
            shape,
            partial.path()
        );

        // Check for Spanned<T> wrapper first
        if is_spanned_shape(shape) {
            return self.deserialize_spanned(partial);
        }

        // Check Def first for Option (which is also a Type::User::Enum)
        if matches!(&shape.def, Def::Option(_)) {
            return self.deserialize_option(partial);
        }

        // Check for smart pointers before other types
        if matches!(&shape.def, Def::Pointer(_)) {
            return self.deserialize_pointer(partial);
        }

        // Check Def for containers and scalars BEFORE transparent type handling
        // This ensures Arc<[T]> and similar smart pointer containers work correctly
        match &shape.def {
            Def::Scalar => return self.deserialize_scalar(partial),
            Def::List(_) | Def::Slice(_) => return self.deserialize_list(partial),
            Def::Map(_) => return self.deserialize_map(partial),
            Def::Array(_) => return self.deserialize_array(partial),
            Def::Set(_) => return self.deserialize_set(partial),
            _ => {}
        }

        // Handle transparent types (newtype wrappers) AFTER checking for concrete types
        if shape.inner.is_some() {
            log::trace!("Handling transparent type: {}", shape.type_identifier);
            partial.begin_inner()?;
            self.deserialize_value(partial)?;
            partial.end()?;
            return Ok(());
        }

        // Check the Type for structs and enums
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                if struct_def.kind == StructKind::Tuple {
                    return self.deserialize_tuple(partial);
                }
                return self.deserialize_struct(partial);
            }
            Type::User(UserType::Enum(_)) => {
                return self.deserialize_enum(partial);
            }
            _ => {}
        }

        Err(self.error(YamlErrorKind::Unsupported(format!(
            "unsupported def: {:?}",
            shape.def
        ))))
    }

    /// Deserialize into a `Spanned<T>` wrapper.
    fn deserialize_spanned<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_spanned");

        // Peek to get the span of the value we're about to parse
        let value_span = self
            .peek()
            .map(|e| Span::from_saphyr_span(&e.span))
            .unwrap_or(Span::new(self.input.len(), 0));

        // Deserialize the inner value into the `value` field
        partial.begin_field("value")?;
        self.deserialize_value(partial)?;
        partial.end()?;

        // Set the span field
        partial.begin_field("span")?;
        // Span struct has offset and len fields
        partial.set_field("offset", value_span.offset)?;
        partial.set_field("len", value_span.len)?;
        partial.end()?;

        Ok(())
    }

    /// Deserialize a scalar value.
    fn deserialize_scalar<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        let shape = partial.shape();
        log::trace!("deserialize_scalar: shape = {shape}");

        let event = self.next_or_eof("scalar value")?;

        let (value, span) = match &event.event {
            OwnedEvent::Scalar { value, .. } => {
                (value.clone(), Span::from_saphyr_span(&event.span))
            }
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "scalar",
                    },
                    Span::from_saphyr_span(&event.span),
                )
                .with_source(self.input));
            }
        };

        self.set_scalar_value(partial, &value, span)
    }

    /// Set a scalar value on the partial based on its type.
    fn set_scalar_value<'facet>(
        &self,
        partial: &mut Partial<'facet>,
        value: &str,
        span: Span,
    ) -> Result<()> {
        let shape = partial.shape();

        // Handle usize and isize explicitly before other numeric types
        if shape.is_type::<usize>() {
            let n: usize = value.parse().map_err(|_| {
                YamlError::new(
                    YamlErrorKind::InvalidValue {
                        message: format!("cannot parse `{value}` as usize"),
                    },
                    span,
                )
                .with_source(self.input)
            })?;
            partial.set(n)?;
            return Ok(());
        }

        if shape.is_type::<isize>() {
            let n: isize = value.parse().map_err(|_| {
                YamlError::new(
                    YamlErrorKind::InvalidValue {
                        message: format!("cannot parse `{value}` as isize"),
                    },
                    span,
                )
                .with_source(self.input)
            })?;
            partial.set(n)?;
            return Ok(());
        }

        // Try other numeric types
        if let Type::Primitive(PrimitiveType::Numeric(numeric_type)) = shape.ty {
            let size = match shape.layout {
                ShapeLayout::Sized(layout) => layout.size(),
                ShapeLayout::Unsized => {
                    return Err(YamlError::new(
                        YamlErrorKind::InvalidValue {
                            message: "cannot assign to unsized type".into(),
                        },
                        span,
                    )
                    .with_source(self.input));
                }
            };

            return self.set_numeric_value(partial, value, numeric_type, size, span);
        }

        // Boolean
        if shape.is_type::<bool>() {
            let b = parse_yaml_bool(value).ok_or_else(|| {
                YamlError::new(
                    YamlErrorKind::InvalidValue {
                        message: format!("cannot parse `{value}` as boolean"),
                    },
                    span,
                )
                .with_source(self.input)
            })?;
            partial.set(b)?;
            return Ok(());
        }

        // Char
        if shape.is_type::<char>() {
            let mut chars = value.chars();
            let c = chars.next().ok_or_else(|| {
                YamlError::new(
                    YamlErrorKind::InvalidValue {
                        message: "empty string cannot be converted to char".into(),
                    },
                    span,
                )
                .with_source(self.input)
            })?;
            if chars.next().is_some() {
                return Err(YamlError::new(
                    YamlErrorKind::InvalidValue {
                        message: "string has more than one character".into(),
                    },
                    span,
                )
                .with_source(self.input));
            }
            partial.set(c)?;
            return Ok(());
        }

        // String
        if shape.is_type::<String>() {
            partial.set(value.to_string())?;
            return Ok(());
        }

        // Try parse_from_str for other types (IpAddr, DateTime, etc.)
        if partial.shape().vtable.parse.is_some() {
            partial.parse_from_str(value).map_err(|e| {
                YamlError::new(YamlErrorKind::Reflect(e), span).with_source(self.input)
            })?;
            return Ok(());
        }

        // Last resort: try setting as string
        partial
            .set(value.to_string())
            .map_err(|e| YamlError::new(YamlErrorKind::Reflect(e), span).with_source(self.input))?;

        Ok(())
    }

    /// Set a numeric value with proper type conversion.
    fn set_numeric_value<'facet>(
        &self,
        partial: &mut Partial<'facet>,
        value: &str,
        numeric_type: NumericType,
        size: usize,
        span: Span,
    ) -> Result<()> {
        match numeric_type {
            NumericType::Integer { signed: false } => {
                let n: u64 = value.parse().map_err(|_| {
                    YamlError::new(
                        YamlErrorKind::InvalidValue {
                            message: format!("cannot parse `{value}` as unsigned integer"),
                        },
                        span,
                    )
                    .with_source(self.input)
                })?;

                match size {
                    1 => {
                        let v = u8::try_from(n).map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::NumberOutOfRange {
                                    value: value.to_string(),
                                    target_type: "u8",
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(v)?;
                    }
                    2 => {
                        let v = u16::try_from(n).map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::NumberOutOfRange {
                                    value: value.to_string(),
                                    target_type: "u16",
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(v)?;
                    }
                    4 => {
                        let v = u32::try_from(n).map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::NumberOutOfRange {
                                    value: value.to_string(),
                                    target_type: "u32",
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(v)?;
                    }
                    8 => {
                        partial.set(n)?;
                    }
                    16 => {
                        let n: u128 = value.parse().map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::InvalidValue {
                                    message: format!("cannot parse `{value}` as u128"),
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(n)?;
                    }
                    _ => {
                        return Err(YamlError::new(
                            YamlErrorKind::Unsupported(format!("unsupported integer size: {size}")),
                            span,
                        )
                        .with_source(self.input));
                    }
                }
            }
            NumericType::Integer { signed: true } => {
                let n: i64 = value.parse().map_err(|_| {
                    YamlError::new(
                        YamlErrorKind::InvalidValue {
                            message: format!("cannot parse `{value}` as signed integer"),
                        },
                        span,
                    )
                    .with_source(self.input)
                })?;

                match size {
                    1 => {
                        let v = i8::try_from(n).map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::NumberOutOfRange {
                                    value: value.to_string(),
                                    target_type: "i8",
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(v)?;
                    }
                    2 => {
                        let v = i16::try_from(n).map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::NumberOutOfRange {
                                    value: value.to_string(),
                                    target_type: "i16",
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(v)?;
                    }
                    4 => {
                        let v = i32::try_from(n).map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::NumberOutOfRange {
                                    value: value.to_string(),
                                    target_type: "i32",
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(v)?;
                    }
                    8 => {
                        partial.set(n)?;
                    }
                    16 => {
                        let n: i128 = value.parse().map_err(|_| {
                            YamlError::new(
                                YamlErrorKind::InvalidValue {
                                    message: format!("cannot parse `{value}` as i128"),
                                },
                                span,
                            )
                            .with_source(self.input)
                        })?;
                        partial.set(n)?;
                    }
                    _ => {
                        return Err(YamlError::new(
                            YamlErrorKind::Unsupported(format!("unsupported integer size: {size}")),
                            span,
                        )
                        .with_source(self.input));
                    }
                }
            }
            NumericType::Float => {
                let f: f64 = value.parse().map_err(|_| {
                    YamlError::new(
                        YamlErrorKind::InvalidValue {
                            message: format!("cannot parse `{value}` as float"),
                        },
                        span,
                    )
                    .with_source(self.input)
                })?;

                match size {
                    4 => partial.set(f as f32)?,
                    8 => partial.set(f)?,
                    _ => {
                        return Err(YamlError::new(
                            YamlErrorKind::Unsupported(format!("unsupported float size: {size}")),
                            span,
                        )
                        .with_source(self.input));
                    }
                };
            }
        }

        Ok(())
    }

    /// Deserialize an Option.
    fn deserialize_option<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_option at path = {}", partial.path());

        // Check if the next value is null
        if let Some(event) = self.peek() {
            if let OwnedEvent::Scalar { value, .. } = &event.event {
                if is_yaml_null(value) {
                    self.next(); // consume the null
                    // Option stays as None (set_default)
                    partial.set_default()?;
                    return Ok(());
                }
            }
        }

        // Non-null value: wrap in Some
        partial.begin_some()?;
        self.deserialize_value(partial)?;
        partial.end()?;
        Ok(())
    }

    /// Deserialize a list/Vec.
    fn deserialize_list<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_list at path = {}", partial.path());

        // Expect SequenceStart
        let event = self.next_or_eof("sequence start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::SequenceStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "sequence start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        partial.begin_list()?;

        // Process items until SequenceEnd
        loop {
            if let Some(event) = self.peek() {
                if matches!(&event.event, OwnedEvent::SequenceEnd) {
                    self.next(); // consume SequenceEnd
                    break;
                }
            } else {
                return Err(self.error(YamlErrorKind::UnexpectedEof {
                    expected: "sequence item or end",
                }));
            }

            partial.begin_list_item()?;
            self.deserialize_value(partial)?;
            partial.end()?;
        }

        Ok(())
    }

    /// Deserialize a map.
    fn deserialize_map<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_map at path = {}", partial.path());

        // Expect MappingStart
        let event = self.next_or_eof("mapping start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::MappingStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "mapping start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        partial.begin_map()?;

        // Process key-value pairs until MappingEnd
        loop {
            if let Some(event) = self.peek() {
                if matches!(&event.event, OwnedEvent::MappingEnd) {
                    self.next(); // consume MappingEnd
                    break;
                }
            } else {
                return Err(self.error(YamlErrorKind::UnexpectedEof {
                    expected: "map key or end",
                }));
            }

            // Get the key
            let key_event = self.next_or_eof("map key")?;
            let key_event_span = key_event.span;
            let key_event_kind = key_event.event.clone();

            let key = match &key_event_kind {
                OwnedEvent::Scalar { value, .. } => value.clone(),
                other => {
                    return Err(YamlError::new(
                        YamlErrorKind::UnexpectedEvent {
                            got: format!("{other:?}"),
                            expected: "string key",
                        },
                        Span::from_saphyr_span(&key_event_span),
                    )
                    .with_source(self.input));
                }
            };

            // Set the key
            partial.begin_key()?;
            partial.set(key)?;
            partial.end()?;

            // Set the value
            partial.begin_value()?;
            self.deserialize_value(partial)?;
            partial.end()?;
        }

        Ok(())
    }

    /// Deserialize a struct.
    fn deserialize_struct<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!(
            "deserialize_struct: {} at path = {}",
            partial.shape(),
            partial.path()
        );

        let shape = partial.shape();
        let struct_def = match &shape.ty {
            Type::User(UserType::Struct(sd)) => sd,
            _ => {
                return Err(self.error(YamlErrorKind::TypeMismatch {
                    expected: "struct",
                    got: "non-struct",
                }));
            }
        };

        // Expect MappingStart
        let event = self.next_or_eof("mapping start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::MappingStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "mapping start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        let deny_unknown = shape.has_deny_unknown_fields_attr();
        let struct_has_default = shape.has_default_attr();

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];

        // Process fields until MappingEnd
        loop {
            if let Some(event) = self.peek() {
                if matches!(&event.event, OwnedEvent::MappingEnd) {
                    self.next(); // consume MappingEnd
                    break;
                }
            } else {
                return Err(self.error(YamlErrorKind::UnexpectedEof {
                    expected: "field name or mapping end",
                }));
            }

            // Get the field name
            let key_event = self.next_or_eof("field name")?;
            let key_event_span = key_event.span;
            let key_event_kind = key_event.event.clone();

            let (field_name, key_span) = match &key_event_kind {
                OwnedEvent::Scalar { value, .. } => {
                    (value.clone(), Span::from_saphyr_span(&key_event_span))
                }
                other => {
                    return Err(YamlError::new(
                        YamlErrorKind::UnexpectedEvent {
                            got: format!("{other:?}"),
                            expected: "field name",
                        },
                        Span::from_saphyr_span(&key_event_span),
                    )
                    .with_source(self.input));
                }
            };

            // Find the field by serialized name (respecting rename attributes)
            let field_info = struct_def
                .fields
                .iter()
                .enumerate()
                .find(|(_, f)| get_serialized_name(f) == field_name.as_str());

            match field_info {
                Some((idx, field)) => {
                    partial.begin_field(field.name)?;
                    self.deserialize_value(partial)?;
                    partial.end()?;
                    fields_set[idx] = true;
                }
                None => {
                    if deny_unknown {
                        let expected: Vec<&'static str> =
                            struct_def.fields.iter().map(|f| f.name).collect();
                        return Err(YamlError::new(
                            YamlErrorKind::UnknownField {
                                field: field_name,
                                expected,
                            },
                            key_span,
                        )
                        .with_source(self.input));
                    }
                    // Skip unknown field
                    log::trace!("Skipping unknown field: {field_name}");
                    self.skip_value()?;
                }
            }
        }

        // Apply defaults for missing fields
        for (idx, field) in struct_def.fields.iter().enumerate() {
            if fields_set[idx] {
                continue;
            }

            let field_has_default_flag = field.flags.contains(FieldFlags::DEFAULT);
            let field_has_default_fn = field.vtable.default_fn.is_some();
            let field_type_has_default = field.shape().is(Characteristic::Default);

            if field_has_default_fn
                || field_has_default_flag
                || (struct_has_default && field_type_has_default)
            {
                partial.set_nth_field_to_default(idx)?;
            }
        }

        Ok(())
    }

    /// Deserialize an enum (externally tagged by default).
    fn deserialize_enum<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!(
            "deserialize_enum: {} at path = {}",
            partial.shape(),
            partial.path()
        );

        // Check for unit variant as plain string
        if let Some(event) = self.peek() {
            if let OwnedEvent::Scalar { value, .. } = &event.event {
                // Try to select this as a unit variant
                let variant_name = value.clone();
                if partial.select_variant_named(&variant_name).is_ok() {
                    self.next(); // consume the scalar
                    return Ok(());
                }
            }
        }

        // Externally tagged: variant_name: data
        let event = self.next_or_eof("enum variant")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::MappingStart { .. } => {
                // Get the variant name (first key)
                let key_event = self.next_or_eof("variant name")?;
                let key_event_span = key_event.span;
                let key_event_kind = key_event.event.clone();

                let variant_name = match &key_event_kind {
                    OwnedEvent::Scalar { value, .. } => value.clone(),
                    other => {
                        return Err(YamlError::new(
                            YamlErrorKind::UnexpectedEvent {
                                got: format!("{other:?}"),
                                expected: "variant name",
                            },
                            Span::from_saphyr_span(&key_event_span),
                        )
                        .with_source(self.input));
                    }
                };

                // Select the variant
                partial.select_variant_named(&variant_name)?;

                // Get the selected variant info
                let variant = partial.selected_variant().ok_or_else(|| {
                    self.error(YamlErrorKind::InvalidValue {
                        message: "failed to get selected variant".into(),
                    })
                })?;

                // Deserialize based on variant kind
                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant: expect null or skip
                        if let Some(event) = self.peek() {
                            if let OwnedEvent::Scalar { value, .. } = &event.event {
                                if is_yaml_null(value) {
                                    self.next();
                                }
                            }
                        }
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        let num_fields = variant.data.fields.len();
                        if num_fields == 1 {
                            // Newtype variant: value directly
                            partial.begin_nth_field(0)?;
                            self.deserialize_value(partial)?;
                            partial.end()?;
                        } else if num_fields > 1 {
                            // Multi-field tuple: sequence
                            self.deserialize_tuple_variant_fields(partial, num_fields)?;
                        }
                    }
                    StructKind::Struct => {
                        // Struct variant: mapping with named fields
                        self.deserialize_struct_variant_fields(partial)?;
                    }
                }

                // Expect MappingEnd
                let end_event = self.next_or_eof("mapping end")?;
                let end_event_span = end_event.span;
                let end_event_kind = end_event.event.clone();

                match &end_event_kind {
                    OwnedEvent::MappingEnd => Ok(()),
                    other => Err(YamlError::new(
                        YamlErrorKind::UnexpectedEvent {
                            got: format!("{other:?}"),
                            expected: "mapping end",
                        },
                        Span::from_saphyr_span(&end_event_span),
                    )
                    .with_source(self.input)),
                }
            }
            other => Err(YamlError::new(
                YamlErrorKind::UnexpectedEvent {
                    got: format!("{other:?}"),
                    expected: "mapping (externally tagged enum)",
                },
                Span::from_saphyr_span(&event_span),
            )
            .with_source(self.input)),
        }
    }

    /// Deserialize tuple variant fields from a sequence.
    fn deserialize_tuple_variant_fields<'facet>(
        &mut self,
        partial: &mut Partial<'facet>,
        num_fields: usize,
    ) -> Result<()> {
        let event = self.next_or_eof("sequence start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::SequenceStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "sequence start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        for i in 0..num_fields {
            partial.begin_nth_field(i)?;
            self.deserialize_value(partial)?;
            partial.end()?;
        }

        let end_event = self.next_or_eof("sequence end")?;
        let end_event_span = end_event.span;
        let end_event_kind = end_event.event.clone();

        match &end_event_kind {
            OwnedEvent::SequenceEnd => Ok(()),
            other => Err(YamlError::new(
                YamlErrorKind::UnexpectedEvent {
                    got: format!("{other:?}"),
                    expected: "sequence end",
                },
                Span::from_saphyr_span(&end_event_span),
            )
            .with_source(self.input)),
        }
    }

    /// Deserialize struct variant fields from a mapping.
    fn deserialize_struct_variant_fields<'facet>(
        &mut self,
        partial: &mut Partial<'facet>,
    ) -> Result<()> {
        let event = self.next_or_eof("mapping start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::MappingStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "mapping start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        loop {
            if let Some(event) = self.peek() {
                if matches!(&event.event, OwnedEvent::MappingEnd) {
                    self.next();
                    break;
                }
            } else {
                return Err(self.error(YamlErrorKind::UnexpectedEof {
                    expected: "field name or mapping end",
                }));
            }

            let key_event = self.next_or_eof("field name")?;
            let key_event_span = key_event.span;
            let key_event_kind = key_event.event.clone();

            let field_name = match &key_event_kind {
                OwnedEvent::Scalar { value, .. } => value.clone(),
                other => {
                    return Err(YamlError::new(
                        YamlErrorKind::UnexpectedEvent {
                            got: format!("{other:?}"),
                            expected: "field name",
                        },
                        Span::from_saphyr_span(&key_event_span),
                    )
                    .with_source(self.input));
                }
            };

            partial.begin_field(&field_name)?;
            self.deserialize_value(partial)?;
            partial.end()?;
        }

        Ok(())
    }

    /// Deserialize a smart pointer (Box, Arc, Rc).
    fn deserialize_pointer<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_pointer at path = {}", partial.path());

        // Check what kind of pointer this is BEFORE calling begin_smart_ptr
        let is_slice_pointer = if let Def::Pointer(ptr_def) = partial.shape().def {
            if let Some(pointee) = ptr_def.pointee() {
                matches!(
                    pointee.ty,
                    Type::Sequence(facet_core::SequenceType::Slice(_))
                )
            } else {
                false
            }
        } else {
            false
        };

        partial.begin_smart_ptr()?;

        if is_slice_pointer {
            // This is a slice pointer like Arc<[T]> - deserialize as array
            let event = self.next_or_eof("sequence start")?;
            let event_span = event.span;
            let event_kind = event.event.clone();

            match &event_kind {
                OwnedEvent::SequenceStart { .. } => {}
                other => {
                    return Err(YamlError::new(
                        YamlErrorKind::UnexpectedEvent {
                            got: format!("{other:?}"),
                            expected: "sequence start",
                        },
                        Span::from_saphyr_span(&event_span),
                    )
                    .with_source(self.input));
                }
            }

            // Process items until SequenceEnd
            loop {
                if let Some(event) = self.peek() {
                    if matches!(&event.event, OwnedEvent::SequenceEnd) {
                        self.next(); // consume SequenceEnd
                        break;
                    }
                } else {
                    return Err(self.error(YamlErrorKind::UnexpectedEof {
                        expected: "sequence item or end",
                    }));
                }

                partial.begin_list_item()?;
                self.deserialize_value(partial)?;
                partial.end()?;
            }
        } else {
            // Regular pointer - deserialize the inner value
            self.deserialize_value(partial)?;
        }

        partial.end()?;
        Ok(())
    }

    /// Deserialize a fixed-size array.
    fn deserialize_array<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_array at path = {}", partial.path());

        let array_len = match &partial.shape().def {
            Def::Array(arr) => arr.n,
            _ => {
                return Err(self.error(YamlErrorKind::InvalidValue {
                    message: "expected array type".into(),
                }));
            }
        };

        let event = self.next_or_eof("sequence start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::SequenceStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "sequence start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        for i in 0..array_len {
            partial.begin_nth_field(i)?;
            self.deserialize_value(partial)?;
            partial.end()?;
        }

        let end_event = self.next_or_eof("sequence end")?;
        let end_event_span = end_event.span;
        let end_event_kind = end_event.event.clone();

        match &end_event_kind {
            OwnedEvent::SequenceEnd => Ok(()),
            other => Err(YamlError::new(
                YamlErrorKind::UnexpectedEvent {
                    got: format!("{other:?}"),
                    expected: "sequence end",
                },
                Span::from_saphyr_span(&end_event_span),
            )
            .with_source(self.input)),
        }
    }

    /// Deserialize a set.
    fn deserialize_set<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_set at path = {}", partial.path());

        let event = self.next_or_eof("sequence start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::SequenceStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "sequence start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        partial.begin_set()?;

        loop {
            if let Some(event) = self.peek() {
                if matches!(&event.event, OwnedEvent::SequenceEnd) {
                    self.next();
                    break;
                }
            } else {
                return Err(self.error(YamlErrorKind::UnexpectedEof {
                    expected: "set item or end",
                }));
            }

            partial.begin_set_item()?;
            self.deserialize_value(partial)?;
            partial.end()?;
        }

        Ok(())
    }

    /// Deserialize a tuple.
    fn deserialize_tuple<'facet>(&mut self, partial: &mut Partial<'facet>) -> Result<()> {
        log::trace!("deserialize_tuple at path = {}", partial.path());

        let tuple_len = match &partial.shape().ty {
            Type::User(UserType::Struct(struct_def)) => struct_def.fields.len(),
            _ => {
                return Err(self.error(YamlErrorKind::InvalidValue {
                    message: "expected tuple type".into(),
                }));
            }
        };

        let event = self.next_or_eof("sequence start")?;
        let event_span = event.span;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::SequenceStart { .. } => {}
            other => {
                return Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{other:?}"),
                        expected: "sequence start",
                    },
                    Span::from_saphyr_span(&event_span),
                )
                .with_source(self.input));
            }
        }

        for i in 0..tuple_len {
            partial.begin_nth_field(i)?;
            self.deserialize_value(partial)?;
            partial.end()?;
        }

        let end_event = self.next_or_eof("sequence end")?;
        let end_event_span = end_event.span;
        let end_event_kind = end_event.event.clone();

        match &end_event_kind {
            OwnedEvent::SequenceEnd => Ok(()),
            other => Err(YamlError::new(
                YamlErrorKind::UnexpectedEvent {
                    got: format!("{other:?}"),
                    expected: "sequence end",
                },
                Span::from_saphyr_span(&end_event_span),
            )
            .with_source(self.input)),
        }
    }

    /// Skip a value (for unknown fields).
    fn skip_value(&mut self) -> Result<()> {
        let event = self.next_or_eof("value to skip")?;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::Scalar { .. } | OwnedEvent::Alias(_) => {
                // Already consumed
                Ok(())
            }
            OwnedEvent::SequenceStart { .. } => {
                // Skip until SequenceEnd
                let mut depth = 1;
                while depth > 0 {
                    let e = self.next_or_eof("sequence end")?;
                    match &e.event {
                        OwnedEvent::SequenceStart { .. } | OwnedEvent::MappingStart { .. } => {
                            depth += 1
                        }
                        OwnedEvent::SequenceEnd | OwnedEvent::MappingEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(())
            }
            OwnedEvent::MappingStart { .. } => {
                // Skip until MappingEnd
                let mut depth = 1;
                while depth > 0 {
                    let e = self.next_or_eof("mapping end")?;
                    match &e.event {
                        OwnedEvent::SequenceStart { .. } | OwnedEvent::MappingStart { .. } => {
                            depth += 1
                        }
                        OwnedEvent::SequenceEnd | OwnedEvent::MappingEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

// ============================================================================
// YAML-specific parsing helpers
// ============================================================================

/// Parse a YAML boolean value (supports yes/no/on/off).
fn parse_yaml_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "yes" | "on" | "y" => Some(true),
        "false" | "no" | "off" | "n" => Some(false),
        _ => None,
    }
}

/// Check if a YAML value represents null.
fn is_yaml_null(value: &str) -> bool {
    matches!(
        value.to_lowercase().as_str(),
        "null" | "~" | "" | "nil" | "none"
    )
}

/// Get the serialized name of a field (respecting rename attributes).
fn get_serialized_name(field: &Field) -> &'static str {
    // Look for rename attribute using extension syntax: #[facet(serde::rename = "value")]
    if let Some(ext) = field.get_extension_attr("serde", "rename") {
        if let Some(opt_name) = ext.get_as::<Option<&'static str>>() {
            if let Some(name) = opt_name {
                return name;
            }
        }
    }
    // Default to the field name
    field.name
}
