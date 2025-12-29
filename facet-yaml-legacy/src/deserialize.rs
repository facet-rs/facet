//! YAML deserialization using saphyr-parser streaming events.
//!
//! This deserializer uses saphyr-parser's event-based API similar to how
//! facet-json uses a tokenizer - processing events on-demand and supporting
//! rewind via event indices for flatten deserialization.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use facet_core::{
    Characteristic, Def, Facet, Field, NumericType, PrimitiveType, Shape, ShapeLayout, StructKind,
    Type, UserType,
};
use facet_reflect::{HeapValue, Partial};
use saphyr_parser::{Event, Parser, ScalarStyle, Span as SaphyrSpan, SpannedEventReceiver};

use crate::error::{SpanExt, YamlError, YamlErrorKind};
use facet_reflect::{Span, is_spanned_shape};
use facet_solver::{Schema, Solver, VariantsByFormat, specificity_score};

type Result<T> = core::result::Result<T, YamlError>;

// ============================================================================
// Public API
// ============================================================================

/// Deserialize a YAML string into a value of type `T`.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies, config files read into a String).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead. For zero-copy deserialization into
/// borrowed types, use [`from_str_borrowed`].
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_yaml_legacy::from_str;
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
pub fn from_str<T: Facet<'static>>(yaml: &str) -> Result<T> {
    from_str_inner(yaml)
}

/// Inner implementation for owned deserialization.
///
/// Uses lifetime transmutation to work around the constraint that the deserializer
/// is parameterized by the input lifetime, while we want to produce a `T: Facet<'static>`.
fn from_str_inner<T: Facet<'static>>(yaml: &str) -> Result<T> {
    // We need to work around the lifetime constraints in the deserialization machinery.
    // The deserializer is parameterized by 'input (the input slice lifetime),
    // but we want to produce a T: Facet<'static> that doesn't borrow from input.
    //
    // The approach: Use an inner function parameterized by 'input that does all the work,
    // then transmute the result back to the 'static lifetime we need.
    //
    // SAFETY: This is safe because:
    // 1. T: Facet<'static> guarantees the type T itself contains no borrowed data
    // 2. YAML events already convert all strings to owned String values
    // 3. BORROW: false on Partial/HeapValue documents that no borrowing occurs
    // 4. The transmutes only affect phantom lifetime markers, not actual runtime data

    fn inner<'input, T: Facet<'static>>(yaml: &'input str) -> Result<T> {
        log::trace!(
            "from_str: parsing YAML for type {}",
            core::any::type_name::<T>()
        );

        let mut deserializer = YamlDeserializer::new(yaml)?;

        // Allocate a Partial<'static, false> - owned mode, no borrowing allowed.
        // We transmute to Partial<'input, false> to work with the deserializer.
        // SAFETY: We're only changing the lifetime marker. The Partial<_, false> doesn't
        // store any 'input references because:
        // - BORROW=false documents no borrowed data
        // - YAML events already convert all strings to owned values
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>()?,
            )
        };

        let partial = deserializer.deserialize_document(wip)?;

        // Check we consumed everything meaningful
        deserializer.expect_end()?;

        // Build the Partial into a HeapValue
        let heap_value = partial
            .build()
            .map_err(|e| YamlError::without_span(YamlErrorKind::Reflect(e)).with_source(yaml))?;

        // Transmute HeapValue<'input, false> to HeapValue<'static, false> so we can materialize to T
        // SAFETY: The HeapValue contains no borrowed data:
        // - BORROW=false documents no borrowed data
        // - YAML events already convert all strings to owned values
        // The transmute only affects the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(|e| YamlError::without_span(YamlErrorKind::Reflect(e)).with_source(yaml))
    }

    inner::<T>(yaml)
}

/// Deserialize YAML from a string slice, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_str`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_yaml_legacy::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let yaml = "name: myapp\nport: 8080";
/// let config: Config = from_str_borrowed(yaml).unwrap();
/// assert_eq!(config.name, "myapp");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(yaml: &'input str) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!(
        "from_str_borrowed: parsing YAML for type {}",
        core::any::type_name::<T>()
    );

    let mut deserializer = YamlDeserializer::new(yaml)?;
    let partial = Partial::alloc::<T>()?;

    let partial = deserializer.deserialize_document(partial)?;

    // Check we consumed everything meaningful
    deserializer.expect_end()?;

    let result = partial
        .build()
        .map_err(|e| YamlError::without_span(YamlErrorKind::Reflect(e)).with_source(yaml))?
        .materialize()
        .map_err(|e| YamlError::without_span(YamlErrorKind::Reflect(e)).with_source(yaml))?;

    Ok(result)
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
    fn deserialize_document<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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
    fn deserialize_value<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        // Check for container-level proxy (applies to values inside Vec<T>, Option<T>, etc.)
        // This must come early because proxy types need to be deserialized through their proxy shape
        let (partial, has_proxy) = partial.begin_custom_deserialization_from_shape()?;
        if has_proxy {
            log::trace!(
                "deserialize_value: using container-level proxy for {}",
                shape.type_identifier
            );
            let partial = self.deserialize_value(partial)?;
            return partial.end().map_err(Into::into);
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
            let mut partial = partial;
            partial = partial.begin_inner()?;
            // Check if field has custom deserialization (field-level proxy)
            if partial
                .parent_field()
                .and_then(|field| field.proxy_convert_in_fn())
                .is_some()
            {
                partial = partial.begin_custom_deserialization()?;
                partial = self.deserialize_value(partial)?;
                partial = partial.end()?;
            } else {
                partial = self.deserialize_value(partial)?;
            }
            partial = partial.end()?;
            return Ok(partial);
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
    fn deserialize_spanned<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
        log::trace!("deserialize_spanned");

        // Peek to get the span of the value we're about to parse
        let value_span = self
            .peek()
            .map(|e| Span::from_saphyr_span(&e.span))
            .unwrap_or(Span::new(self.input.len(), 0));

        let mut partial = partial;
        // Deserialize the inner value into the `value` field
        partial = partial.begin_field("value")?;
        partial = self.deserialize_value(partial)?;
        partial = partial.end()?;

        // Set the span field
        partial = partial.begin_field("span")?;
        // Span struct has offset and len fields
        partial = partial.set_field("offset", value_span.offset)?;
        partial = partial.set_field("len", value_span.len)?;
        partial = partial.end()?;

        Ok(partial)
    }

    /// Deserialize a scalar value.
    fn deserialize_scalar<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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
    fn set_scalar_value<'facet, const BORROW: bool>(
        &self,
        partial: Partial<'facet, BORROW>,
        value: &str,
        span: Span,
    ) -> Result<Partial<'facet, BORROW>> {
        let mut partial = partial;
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
            partial = partial.set(n)?;
            return Ok(partial);
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
            partial = partial.set(n)?;
            return Ok(partial);
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
            partial = partial.set(b)?;
            return Ok(partial);
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
            partial = partial.set(c)?;
            return Ok(partial);
        }

        // String
        if shape.is_type::<String>() {
            partial = partial.set(value.to_string())?;
            return Ok(partial);
        }

        // Try parse_from_str for other types (IpAddr, DateTime, etc.)
        if partial.shape().vtable.has_parse() {
            partial = partial.parse_from_str(value).map_err(|e| {
                YamlError::new(YamlErrorKind::Reflect(e), span).with_source(self.input)
            })?;
            return Ok(partial);
        }

        // Last resort: try setting as string
        partial = partial
            .set(value.to_string())
            .map_err(|e| YamlError::new(YamlErrorKind::Reflect(e), span).with_source(self.input))?;

        Ok(partial)
    }

    /// Set a numeric value with proper type conversion.
    fn set_numeric_value<'facet, const BORROW: bool>(
        &self,
        partial: Partial<'facet, BORROW>,
        value: &str,
        numeric_type: NumericType,
        size: usize,
        span: Span,
    ) -> Result<Partial<'facet, BORROW>> {
        let mut partial = partial;
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
                        partial = partial.set(v)?;
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
                        partial = partial.set(v)?;
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
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(n)?;
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
                        partial = partial.set(n)?;
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
                        partial = partial.set(v)?;
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
                        partial = partial.set(v)?;
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
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(n)?;
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
                        partial = partial.set(n)?;
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
                    4 => partial = partial.set(f as f32)?,
                    8 => partial = partial.set(f)?,
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

        Ok(partial)
    }

    /// Deserialize an Option.
    fn deserialize_option<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
        log::trace!("deserialize_option at path = {}", partial.path());

        let mut partial = partial;
        // Check if the next value is null
        if let Some(event) = self.peek()
            && let OwnedEvent::Scalar { value, .. } = &event.event
            && is_yaml_null(value)
        {
            self.next(); // consume the null
            // Option stays as None (set_default)
            partial = partial.set_default()?;
            return Ok(partial);
        }

        // Non-null value: wrap in Some
        partial = partial.begin_some()?;
        partial = self.deserialize_value(partial)?;
        partial = partial.end()?;
        Ok(partial)
    }

    /// Deserialize a list/Vec.
    fn deserialize_list<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
        partial = partial.begin_list()?;

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

            partial = partial.begin_list_item()?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        }

        Ok(partial)
    }

    /// Deserialize a map.
    fn deserialize_map<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
        partial = partial.begin_map()?;

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
            partial = partial.begin_key()?;
            partial = partial.set(key)?;
            partial = partial.end()?;

            // Set the value
            partial = partial.begin_value()?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        }

        Ok(partial)
    }

    /// Deserialize a struct.
    fn deserialize_struct<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
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
                    partial = partial.begin_field(field.name)?;
                    partial = self.deserialize_value(partial)?;
                    partial = partial.end()?;
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

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || (struct_has_default && field_type_has_default) {
                partial = partial.set_nth_field_to_default(idx)?;
            } else if field_is_option {
                partial = partial.begin_field(field.name)?;
                partial = partial.set_default()?;
                partial = partial.end()?;
            }
        }

        Ok(partial)
    }

    /// Deserialize an enum (externally tagged by default).
    fn deserialize_enum<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
        log::trace!(
            "deserialize_enum: {} at path = {}",
            partial.shape(),
            partial.path()
        );

        // Check if this is an untagged enum
        if partial.shape().is_untagged() {
            return self.deserialize_untagged_enum(partial);
        }

        // Check for unit variant as plain string
        let mut partial = partial;
        let try_unit_variant = if let Some(event) = self.peek() {
            if let OwnedEvent::Scalar { value, .. } = &event.event {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(variant_name) = try_unit_variant {
            // Check if this variant exists first (to avoid moving partial if it doesn't)
            if let Some((_, variant)) = partial.find_variant(&variant_name) {
                // Check if it's a unit variant
                if variant.data.kind == StructKind::Unit {
                    // Select this as a unit variant
                    partial = partial.select_variant_named(&variant_name)?;
                    self.next(); // consume the scalar
                    return Ok(partial);
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
                partial = partial.select_variant_named(&variant_name)?;

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
                        if let Some(event) = self.peek()
                            && let OwnedEvent::Scalar { value, .. } = &event.event
                            && is_yaml_null(value)
                        {
                            self.next();
                        }
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        let num_fields = variant.data.fields.len();
                        if num_fields == 1 {
                            // Newtype variant: value directly
                            partial = partial.begin_nth_field(0)?;
                            partial = self.deserialize_value(partial)?;
                            partial = partial.end()?;
                        } else if num_fields > 1 {
                            // Multi-field tuple: sequence
                            partial = self.deserialize_tuple_variant_fields(partial, num_fields)?;
                        }
                    }
                    StructKind::Struct => {
                        // Struct variant: mapping with named fields
                        partial = self.deserialize_struct_variant_fields(partial)?;
                    }
                }

                // Expect MappingEnd
                let end_event = self.next_or_eof("mapping end")?;
                let end_event_span = end_event.span;
                let end_event_kind = end_event.event.clone();

                match &end_event_kind {
                    OwnedEvent::MappingEnd => Ok(partial),
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

    /// Deserialize an untagged enum by scanning keys and using the Solver
    /// to determine which variant matches.
    fn deserialize_untagged_enum<'facet, const BORROW: bool>(
        &mut self,
        mut partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
        log::trace!(
            "deserialize_untagged_enum: {} at path = {}",
            partial.shape(),
            partial.path()
        );

        let shape = partial.shape();

        // Build schema - this creates one resolution per variant for untagged enums
        let schema = Schema::build_auto(shape).map_err(|e| {
            self.error(YamlErrorKind::InvalidValue {
                message: format!("failed to build schema: {e}"),
            })
        })?;

        // Create the solver
        let mut solver = Solver::new(&schema);

        // Check what type of YAML value we have
        let event = self.peek().ok_or_else(|| {
            self.error(YamlErrorKind::UnexpectedEof {
                expected: "untagged enum value",
            })
        })?;
        let event_kind = event.event.clone();

        match &event_kind {
            OwnedEvent::MappingStart { .. } => {
                // Record start position for rewinding after we determine the variant
                let start_pos = self.position();

                self.next(); // consume MappingStart

                // ========== PASS 1: Collect event indices of keys ==========
                // We store indices into self.events to avoid cloning strings
                let mut key_indices: Vec<usize> = Vec::new();
                loop {
                    let event = self.peek().ok_or_else(|| {
                        self.error(YamlErrorKind::UnexpectedEof {
                            expected: "field name or mapping end",
                        })
                    })?;

                    if matches!(&event.event, OwnedEvent::MappingEnd) {
                        self.next(); // consume MappingEnd
                        break;
                    }

                    // Record the position of this key event
                    let key_pos = self.pos;

                    // Validate it's a scalar
                    let key_event = self.next_or_eof("field name")?;
                    match &key_event.event {
                        OwnedEvent::Scalar { .. } => {
                            key_indices.push(key_pos);
                        }
                        other => {
                            return Err(YamlError::new(
                                YamlErrorKind::UnexpectedEvent {
                                    got: format!("{other:?}"),
                                    expected: "field name (scalar)",
                                },
                                Span::from_saphyr_span(&key_event.span),
                            )
                            .with_source(self.input));
                        }
                    };

                    // Skip the value
                    self.skip_value()?;
                }

                // ========== Feed keys to solver (zero-copy via references) ==========
                for &idx in &key_indices {
                    if let OwnedEvent::Scalar { value, .. } = &self.events[idx].event {
                        let _decision = solver.see_key(value.as_str());
                    }
                }

                // ========== Get the resolved variant ==========
                let config_handle = solver.finish().map_err(|e| {
                    self.error(YamlErrorKind::InvalidValue {
                        message: format!("solver error: {e}"),
                    })
                })?;
                let config = config_handle.resolution();

                // Extract the variant name from the resolution
                let variant_name = config
                    .variant_selections()
                    .first()
                    .map(|vs| vs.variant_name)
                    .ok_or_else(|| {
                        self.error(YamlErrorKind::InvalidValue {
                            message: "solver returned resolution with no variant selection".into(),
                        })
                    })?;

                // Select the variant
                partial = partial.select_variant_named(variant_name)?;

                // ========== PASS 2: Rewind and deserialize ==========
                // Create a new deserializer at the start of the mapping
                let events = self.clone_events();
                let mut rewound_deser =
                    YamlDeserializer::from_position(self.input, events, start_pos);

                // Deserialize the variant content
                partial = rewound_deser.deserialize_untagged_variant_content(partial)?;

                Ok(partial)
            }
            OwnedEvent::SequenceStart { .. } => {
                // For sequences (tuple variants), match by arity and try each candidate
                self.deserialize_untagged_sequence_variant(partial, shape)
            }
            OwnedEvent::Scalar { value, style, .. } => {
                // For scalars (newtype variants wrapping scalar types)
                self.deserialize_untagged_scalar_variant(partial, shape, value, *style)
            }
            other => Err(YamlError::new(
                YamlErrorKind::UnexpectedEvent {
                    got: format!("{other:?}"),
                    expected: "mapping, sequence, or scalar for untagged enum",
                },
                self.current_span(),
            )
            .with_source(self.input)),
        }
    }

    /// Deserialize the content of an untagged enum variant.
    /// Handles struct variants and tuple variants.
    fn deserialize_untagged_variant_content<'facet, const BORROW: bool>(
        &mut self,
        mut partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
        // Get the selected variant info
        let variant = partial.selected_variant().ok_or_else(|| {
            self.error(YamlErrorKind::InvalidValue {
                message: "no variant selected".into(),
            })
        })?;

        // Determine if this is a struct variant or tuple variant
        let is_struct_variant = variant
            .data
            .fields
            .first()
            .map(|f| !f.name.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or(true);

        if is_struct_variant {
            // Struct variant: deserialize from mapping
            partial = self.deserialize_struct_variant_fields(partial)?;
        } else if variant.data.fields.len() == 1 {
            // Single-element tuple variant: just the value directly
            partial = partial.begin_nth_field(0)?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        } else {
            // Multi-element tuple variant: sequence
            let num_fields = variant.data.fields.len();
            partial = self.deserialize_tuple_variant_fields(partial, num_fields)?;
        }

        Ok(partial)
    }

    /// Deserialize an untagged enum from a scalar value.
    ///
    /// This handles newtype variants that wrap scalar types (String, i32, etc.).
    /// We determine the variant based on:
    /// 1. YAML scalar style (quoted strings are always strings)
    /// 2. YAML value type (number vs string vs bool)
    /// 3. For numbers: pick the most specific numeric type that fits
    /// 4. For strings with proxy types: try in order until one succeeds
    fn deserialize_untagged_scalar_variant<'facet, const BORROW: bool>(
        &mut self,
        mut partial: Partial<'facet, BORROW>,
        shape: &'static facet_core::Shape,
        scalar_value: &str,
        scalar_style: ScalarStyle,
    ) -> Result<Partial<'facet, BORROW>> {
        let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
            self.error(YamlErrorKind::InvalidValue {
                message: "expected enum shape for untagged deserialization".into(),
            })
        })?;

        // Check if the scalar value matches a unit variant name
        for variant in &variants_by_format.unit_variants {
            if variant.name == scalar_value {
                // This is a unit variant - select it without deserializing into a field
                partial = partial.select_variant_named(variant.name)?;
                return Ok(partial);
            }
        }

        // Not a unit variant - fall back to newtype scalar variant handling
        if variants_by_format.scalar_variants.is_empty() {
            return Err(self.error(YamlErrorKind::InvalidValue {
                message: format!(
                    "no scalar-accepting variants in untagged enum {} for value: {}",
                    shape.type_identifier, scalar_value
                ),
            }));
        }

        // Select variant and deserialize - handles trial parsing for multiple string-like candidates
        let variant_name =
            self.select_scalar_variant(&variants_by_format, scalar_value, scalar_style)?;

        partial = partial.select_variant_named(variant_name)?;
        partial = partial.begin_nth_field(0)?;
        partial = self.deserialize_value(partial)?;
        partial = partial.end()?;

        Ok(partial)
    }

    /// Select which scalar variant to use based on the YAML value.
    ///
    /// For quoted strings: always prefer string types.
    /// For numeric values: pick the smallest type that can hold the value.
    /// For string values: if only one string-like variant, use it; otherwise try in order.
    fn select_scalar_variant(
        &self,
        variants: &VariantsByFormat,
        scalar_value: &str,
        scalar_style: ScalarStyle,
    ) -> Result<&'static str> {
        use facet_core::ScalarType;

        // Sort by specificity (most specific first)
        let mut candidates: Vec<_> = variants.scalar_variants.clone();
        candidates.sort_by_key(|(_, inner_shape)| specificity_score(inner_shape));

        // If the YAML scalar is quoted, it's explicitly a string - skip numeric parsing
        let is_quoted = matches!(
            scalar_style,
            ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted
        );

        if is_quoted {
            // Find a string-like variant
            for (variant, shape) in &candidates {
                if matches!(
                    shape.scalar_type(),
                    Some(ScalarType::String) | Some(ScalarType::Str) | Some(ScalarType::CowStr)
                ) || shape.scalar_type().is_none()
                {
                    return Ok(variant.name);
                }
            }
            // No string variant found, fall through to use the value as-is
        }

        // Try to parse as different types and find the best match
        // Check if it's a boolean (YAML supports yes/no/on/off/y/n)
        if parse_yaml_bool(scalar_value).is_some() {
            for (variant, inner_shape) in &candidates {
                if inner_shape.scalar_type() == Some(ScalarType::Bool) {
                    return Ok(variant.name);
                }
            }
        }

        // Check if it's a number (integer or float)
        if let Ok(int_val) = scalar_value.parse::<i128>() {
            // Find the smallest integer type that fits
            for (variant, inner_shape) in &candidates {
                let fits = match inner_shape.scalar_type() {
                    Some(ScalarType::U8) => int_val >= 0 && int_val <= u8::MAX as i128,
                    Some(ScalarType::U16) => int_val >= 0 && int_val <= u16::MAX as i128,
                    Some(ScalarType::U32) => int_val >= 0 && int_val <= u32::MAX as i128,
                    Some(ScalarType::U64) => int_val >= 0 && int_val <= u64::MAX as i128,
                    Some(ScalarType::U128) => int_val >= 0,
                    Some(ScalarType::USize) => int_val >= 0 && int_val <= usize::MAX as i128,
                    Some(ScalarType::I8) => {
                        int_val >= i8::MIN as i128 && int_val <= i8::MAX as i128
                    }
                    Some(ScalarType::I16) => {
                        int_val >= i16::MIN as i128 && int_val <= i16::MAX as i128
                    }
                    Some(ScalarType::I32) => {
                        int_val >= i32::MIN as i128 && int_val <= i32::MAX as i128
                    }
                    Some(ScalarType::I64) => {
                        int_val >= i64::MIN as i128 && int_val <= i64::MAX as i128
                    }
                    Some(ScalarType::I128) => true,
                    Some(ScalarType::ISize) => {
                        int_val >= isize::MIN as i128 && int_val <= isize::MAX as i128
                    }
                    _ => false,
                };
                if fits {
                    return Ok(variant.name);
                }
            }
        }

        // Check if it's a float
        if scalar_value.parse::<f64>().is_ok() {
            for (variant, inner_shape) in &candidates {
                match inner_shape.scalar_type() {
                    Some(ScalarType::F32) | Some(ScalarType::F64) => {
                        return Ok(variant.name);
                    }
                    _ => {}
                }
            }
        }

        // Fall back to string-like types
        // Separate variants into:
        // 1. Parseable types (have vtable.parse) - try these first, they may fail
        // 2. String fallbacks (String/Str/CowStr) - always succeed
        let mut parseable_variants: Vec<_> = Vec::new();
        let mut string_fallbacks: Vec<_> = Vec::new();

        for (variant, shape) in &candidates {
            let is_plain_string = matches!(
                shape.scalar_type(),
                Some(ScalarType::String) | Some(ScalarType::Str) | Some(ScalarType::CowStr)
            );

            if is_plain_string {
                string_fallbacks.push((variant, *shape));
            } else if shape.vtable.has_parse() {
                // Has a parse function - could be IpAddr, custom proxy type, etc.
                parseable_variants.push((variant, *shape));
            }
        }

        // Try parseable types first (they're more specific than String)
        for (variant, shape) in &parseable_variants {
            if try_parse_scalar(scalar_value, shape) {
                return Ok(variant.name);
            }
        }

        // Fall back to String types
        if let Some((variant, _)) = string_fallbacks.first() {
            return Ok(variant.name);
        }

        // Just pick the first candidate as fallback
        if let Some((variant, _)) = candidates.first() {
            return Ok(variant.name);
        }

        Err(self.error(YamlErrorKind::InvalidValue {
            message: format!("no suitable variant found for scalar value: {scalar_value}"),
        }))
    }

    /// Deserialize an untagged enum from a sequence (tuple variant).
    ///
    /// We match by arity first. If only one variant matches, use it directly.
    /// If multiple variants have the same arity, we'd need type-based disambiguation.
    fn deserialize_untagged_sequence_variant<'facet, const BORROW: bool>(
        &mut self,
        mut partial: Partial<'facet, BORROW>,
        shape: &'static facet_core::Shape,
    ) -> Result<Partial<'facet, BORROW>> {
        let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
            self.error(YamlErrorKind::InvalidValue {
                message: "expected enum shape for untagged deserialization".into(),
            })
        })?;

        if variants_by_format.tuple_variants.is_empty() {
            return Err(self.error(YamlErrorKind::InvalidValue {
                message: format!(
                    "no tuple variants in untagged enum {} for sequence value",
                    shape.type_identifier
                ),
            }));
        }

        // Record start position
        let start_pos = self.position();

        // First pass: count sequence elements to get arity
        self.next(); // consume SequenceStart
        let mut arity = 0;
        loop {
            let event = self.peek().ok_or_else(|| {
                self.error(YamlErrorKind::UnexpectedEof {
                    expected: "sequence element or end",
                })
            })?;

            if matches!(&event.event, OwnedEvent::SequenceEnd) {
                self.next(); // consume SequenceEnd
                break;
            }

            arity += 1;
            self.skip_value()?;
        }

        // Find variants matching this arity
        let matching_variants = variants_by_format.tuple_variants_with_arity(arity);

        if matching_variants.is_empty() {
            return Err(self.error(YamlErrorKind::InvalidValue {
                message: format!(
                    "no tuple variant in untagged enum {} has arity {} (sequence has {} elements)",
                    shape.type_identifier, arity, arity
                ),
            }));
        }

        if matching_variants.len() > 1 {
            // Multiple variants with same arity - for now, just pick the first
            // TODO: Could do type-based disambiguation by examining element types
            log::warn!(
                "Multiple tuple variants with arity {} in untagged enum {}, picking first: {}",
                arity,
                shape.type_identifier,
                matching_variants[0].name
            );
        }

        let selected_variant = matching_variants[0];

        // Rewind and deserialize into the selected variant
        self.pos = start_pos;

        partial = partial.select_variant_named(selected_variant.name)?;

        if selected_variant.data.fields.len() == 1 {
            // Newtype tuple variant - deserialize the entire sequence as the inner value
            partial = partial.begin_nth_field(0)?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        } else {
            debug_assert_eq!(
                selected_variant.data.fields.len(),
                arity,
                "tuple variant arity should match sequence length"
            );
            partial =
                self.deserialize_tuple_variant_fields(partial, selected_variant.data.fields.len())?;
        }

        Ok(partial)
    }

    /// Deserialize tuple variant fields from a sequence.
    fn deserialize_tuple_variant_fields<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
        num_fields: usize,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
        for i in 0..num_fields {
            partial = partial.begin_nth_field(i)?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        }

        let end_event = self.next_or_eof("sequence end")?;
        let end_event_span = end_event.span;
        let end_event_kind = end_event.event.clone();

        match &end_event_kind {
            OwnedEvent::SequenceEnd => Ok(partial),
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
    fn deserialize_struct_variant_fields<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
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

            partial = partial.begin_field(&field_name)?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        }

        Ok(partial)
    }

    /// Deserialize a smart pointer (Box, Arc, Rc).
    fn deserialize_pointer<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
        partial = partial.begin_smart_ptr()?;

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

                partial = partial.begin_list_item()?;
                partial = self.deserialize_value(partial)?;
                partial = partial.end()?;
            }
        } else {
            // Regular pointer - deserialize the inner value
            partial = self.deserialize_value(partial)?;
        }

        partial = partial.end()?;
        Ok(partial)
    }

    /// Deserialize a fixed-size array.
    fn deserialize_array<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
        for i in 0..array_len {
            partial = partial.begin_nth_field(i)?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        }

        let end_event = self.next_or_eof("sequence end")?;
        let end_event_span = end_event.span;
        let end_event_kind = end_event.event.clone();

        match &end_event_kind {
            OwnedEvent::SequenceEnd => Ok(partial),
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
    fn deserialize_set<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
        partial = partial.begin_set()?;

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

            partial = partial.begin_set_item()?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        }

        Ok(partial)
    }

    /// Deserialize a tuple.
    fn deserialize_tuple<'facet, const BORROW: bool>(
        &mut self,
        partial: Partial<'facet, BORROW>,
    ) -> Result<Partial<'facet, BORROW>> {
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

        let mut partial = partial;
        for i in 0..tuple_len {
            partial = partial.begin_nth_field(i)?;
            partial = self.deserialize_value(partial)?;
            partial = partial.end()?;
        }

        let end_event = self.next_or_eof("sequence end")?;
        let end_event_span = end_event.span;
        let end_event_kind = end_event.event.clone();

        match &end_event_kind {
            OwnedEvent::SequenceEnd => Ok(partial),
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

/// Try to parse a scalar value into a type that has a vtable.parse function.
/// Returns true if parsing would succeed, false otherwise.
/// This is used for trial parsing when selecting between multiple string-like variants.
fn try_parse_scalar(value: &str, shape: &'static Shape) -> bool {
    use alloc::alloc::{Layout, alloc, dealloc};

    if !shape.vtable.has_parse() {
        return false;
    }

    // Get the layout for the type
    let layout = match shape.layout {
        ShapeLayout::Sized(layout) => layout,
        ShapeLayout::Unsized => return false,
    };

    // Don't allocate for zero-sized types
    if layout.size() == 0 {
        // For ZSTs, just try calling the parse function with a dangling pointer
        // SAFETY: For ZST, we use a non-null aligned pointer - alignment 1 works for all ZST
        #[allow(unsafe_code)]
        let ptr = facet_core::PtrMut::new(core::ptr::NonNull::<u8>::dangling().as_ptr());
        #[allow(unsafe_code)]
        let result = unsafe { shape.call_parse(value, ptr) };
        return matches!(result, Some(Ok(())));
    }

    // Allocate memory for the trial parse
    let rust_layout = Layout::from_size_align(layout.size(), layout.align()).unwrap();
    #[allow(unsafe_code)]
    let raw_ptr = unsafe { alloc(rust_layout) };
    if raw_ptr.is_null() {
        return false;
    }

    let ptr = facet_core::PtrMut::new(raw_ptr);

    // Try parsing
    #[allow(unsafe_code)]
    let result = unsafe { shape.call_parse(value, ptr) };
    let success = matches!(result, Some(Ok(())));

    // If successful, we need to drop the value properly before deallocating
    if success {
        // SAFETY: parse succeeded, so the memory is now initialized
        #[allow(unsafe_code)]
        unsafe {
            let _ = shape.call_drop_in_place(ptr);
        }
    }

    // Deallocate the memory
    #[allow(unsafe_code)]
    unsafe {
        dealloc(raw_ptr, rust_layout);
    }

    success
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
    if let Some(ext) = field.get_attr(Some("serde"), "rename")
        && let Some(Some(name)) = ext.get_as::<Option<&'static str>>()
    {
        return name;
    }
    // Default to the field name
    field.name
}
