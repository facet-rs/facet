use std::marker::PhantomData;

use facet_core::{Facet, Shape};
use facet_reflect::{HeapValue, Partial, Span};

use crate::{FormatParser, ParseEvent};

mod error;
pub use error::*;

/// Convert a ReflectError to a DeserializeError with span and path.
///
/// The path is extracted from the ReflectError (which now always contains one),
/// and the span comes from the deserializer's last_span.
///
/// # Usage
/// ```ignore
/// wip = reflect!(self, wip, begin_nth_field(0), "begin Raw's inner field");
/// wip = reflect!(self, wip, end(), "end Raw wrapper");
/// ```
macro_rules! reflect {
    ($self:expr, $wip:expr, $method:ident($($args:expr),*), $context:literal) => {{
        $wip.$method($($args),*).map_err(|e| {
            crate::DeserializeError {
                span: Some($self.last_span),
                path: Some(e.path),
                kind: crate::DeserializeErrorKind::Reflect {
                    kind: e.kind,
                    context: $context,
                },
            }
        })?
    }};
}

mod setters;

mod dynamic;
mod eenum;
mod entry;
// mod pointer;
// mod scalar_matches;
// mod struct_simple;
// mod struct_with_flatten;
// mod validate;

/// Generic deserializer that drives a format-specific parser directly into `Partial`.
///
/// The const generic `BORROW` controls whether string data can be borrowed:
/// - `BORROW=true`: strings without escapes are borrowed from input
/// - `BORROW=false`: all strings are owned
pub struct FormatDeserializer<'input, const BORROW: bool> {
    parser: Box<dyn FormatParser<'input>>,

    /// The span of the most recently consumed event (for error reporting).
    last_span: Span,

    _marker: PhantomData<&'input ()>,
}

impl<'input> FormatDeserializer<'input, true> {
    /// Create a new deserializer that can borrow strings from input.
    pub fn new(parser: impl FormatParser<'input> + 'static) -> Self {
        Self {
            parser: Box::new(parser),
            last_span: Span { offset: 0, len: 0 },
            _marker: PhantomData,
        }
    }
}

impl<'input> FormatDeserializer<'input, false> {
    /// Create a new deserializer that produces owned strings.
    pub fn new_owned(parser: impl FormatParser<'input> + 'static) -> Self {
        Self {
            parser: Box::new(parser),
            last_span: Span { offset: 0, len: 0 },
            _marker: PhantomData,
        }
    }
}

impl<'input, const BORROW: bool> FormatDeserializer<'input, BORROW> {
    /// Consume the facade and return the underlying parser.
    pub fn into_inner(self) -> Box<dyn FormatParser<'input>> {
        self.parser
    }

    /// Borrow the inner parser mutably.
    pub fn parser_mut(&mut self) -> &mut dyn FormatParser<'input> {
        &mut *self.parser
    }
}

impl<'input> FormatDeserializer<'input, true> {
    /// Deserialize the next value in the stream into `T`, allowing borrowed strings.
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'input>,
    {
        let wip: Partial<'input, true> = Partial::alloc::<T>()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "allocating partial"))?;
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, true> = partial
            .build()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "building heap value"))?;
        heap_value
            .materialize::<T>()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "materializing"))
    }

    /// Deserialize the next value in the stream into `T` (for backward compatibility).
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'input>,
    {
        self.deserialize()
    }

    /// Deserialize using deferred mode, allowing interleaved field initialization.
    ///
    /// This is required for formats like TOML that allow table reopening, where
    /// fields of a nested struct may be set, then fields of a sibling, then more
    /// fields of the original struct.
    pub fn deserialize_deferred<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'input>,
    {
        let wip: Partial<'input, true> = Partial::alloc::<T>()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "allocating partial"))?;
        let wip = wip
            .begin_deferred()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "beginning deferred"))?;

        let partial = self.deserialize_into(wip)?;
        let partial = partial
            .finish_deferred()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "finishing deferred"))?;

        let heap_value: HeapValue<'input, true> = partial
            .build()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "building heap value"))?;
        heap_value
            .materialize::<T>()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "materializing"))
    }
}

impl<'input> FormatDeserializer<'input, false> {
    /// Deserialize the next value in the stream into `T`, using owned strings.
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        // SAFETY: alloc_owned produces Partial<'static, false>, but our deserializer
        // expects 'input. Since BORROW=false means we never borrow from input anyway,
        // this is safe. We also transmute the HeapValue back to 'static before materializing.
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>()
                    .map_err(|e| DeserializeError::bug_from_reflect(e, "allocating owned"))?,
            )
        };

        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, false> = partial
            .build()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "building"))?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "materializing"))
    }

    /// Deserialize the next value in the stream into `T` (for backward compatibility).
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        self.deserialize()
    }

    /// Deserialize using deferred mode, allowing interleaved field initialization.
    ///
    /// This is required for formats like TOML that allow table reopening, where
    /// fields of a nested struct may be set, then fields of a sibling, then more
    /// fields of the original struct.
    pub fn deserialize_deferred<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        // SAFETY: alloc_owned produces Partial<'static, false>, but our deserializer
        // expects 'input. Since BORROW=false means we never borrow from input anyway,
        // this is safe. We also transmute the HeapValue back to 'static before materializing.
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>()
                    .map_err(|e| DeserializeError::bug_from_reflect(e, "allocating owned"))?,
            )
        };
        let wip = wip
            .begin_deferred()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "beginning deferred"))?;

        let partial = self.deserialize_into(wip)?;
        let partial = partial
            .finish_deferred()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "finishing deferred"))?;
        let heap_value: HeapValue<'input, false> = partial
            .build()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "building"))?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "materializing"))
    }

    /// Deserialize using an explicit source shape for parser hints.
    ///
    /// This is useful for non-self-describing formats like postcard where you need
    /// to decode data that was serialized using a specific type, but you only have
    /// the shape information at runtime (not the concrete type).
    ///
    /// The target type `T` should typically be a `DynamicValue` like `facet_value::Value`.
    pub fn deserialize_with_shape<T>(
        &mut self,
        source_shape: &'static Shape,
    ) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>().map_err(|e| {
                    DeserializeError::bug_from_reflect(e, "allocating partial value")
                })?,
            )
        };

        let partial = self.deserialize_into_with_shape(wip, source_shape)?;

        let heap_value: HeapValue<'input, false> = partial
            .build()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "building heap value"))?;

        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(|e| DeserializeError::bug_from_reflect(e, "materializing deserialized value"))
    }
}

impl<'input, const BORROW: bool> FormatDeserializer<'input, BORROW> {
    /// Read the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_event(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        let event = self.parser.next_event()?.ok_or_else(|| {
            DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)
        })?;
        trace!(?event, expected, "expect_event: got event");
        // Capture the span of the consumed event for error reporting
        if let Some(span) = self.parser.current_span() {
            self.last_span = span;
        }
        Ok(event)
    }

    /// Peek at the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_peek(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        let event = self.parser.peek_event()?.ok_or_else(|| {
            DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)
        })?;
        trace!(?event, expected, "expect_peek: peeked event");
        Ok(event)
    }

    /// Check if a field matches a given name by effective name or alias.
    fn field_matches(field: &facet_core::Field, name: &str) -> bool {
        field.effective_name() == name || field.alias.iter().any(|alias| *alias == name)
    }

    /// Make an error using the last span, the current path of the given wip.
    fn mk_err(
        &self,
        wip: &Partial<'input, BORROW>,
        kind: DeserializeErrorKind,
    ) -> DeserializeError {
        DeserializeError {
            span: Some(self.last_span),
            path: Some(wip.path()),
            kind,
        }
    }
}
