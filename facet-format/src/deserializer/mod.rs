//! # Format Deserializer
//!
//! This module provides a generic deserializer that drives format-specific parsers
//! (JSON, TOML, etc.) directly into facet's `Partial` builder.
//!
//! ## Error Handling Philosophy
//!
//! Good error messages are critical for developer experience. When deserialization
//! fails, users need to know **where** the error occurred (both in the input and
//! in the type structure) and **what** went wrong. This module enforces several
//! invariants to ensure high-quality error messages.
//!
//! ### Always Include a Span
//!
//! Every error should include a `Span` pointing to the location in the input where
//! the error occurred. This allows tools to highlight the exact position:
//!
//! ```text
//! error: expected integer, got string
//!   --> config.toml:15:12
//!    |
//! 15 |     count = "not a number"
//!    |             ^^^^^^^^^^^^^^
//! ```
//!
//! The deserializer tracks `last_span` which is updated after consuming each event.
//! When constructing errors manually, always use `self.last_span`. The `SpanGuard`
//! RAII type sets a thread-local span that the `From<ReflectError>` impl uses
//! automatically.
//!
//! ### Always Include a Path
//!
//! Every error should include a `Path` showing the location in the type structure.
//! This is especially important for nested types where the span alone doesn't tell
//! you which field failed:
//!
//! ```text
//! error: missing required field `email`
//!   --> config.toml:10:5
//!    |
//! 10 |     [users.alice]
//!    |     ^^^^^^^^^^^^^
//!    |
//!    = path: config.users["alice"].contact
//! ```
//!
//! When constructing errors, use `wip.path()` to get the current path through the
//! type structure. The `Partial` tracks this automatically as you descend into
//! fields, list items, map keys, etc.
//!
//! ### Error Construction Patterns
//!
//! **For errors during deserialization (when `wip` is available):**
//!
//! ```ignore
//! return Err(DeserializeError {
//!     span: Some(self.last_span),
//!     path: Some(wip.path()),
//!     kind: DeserializeErrorKind::UnexpectedToken { ... },
//! });
//! ```
//!
//! **For errors from `Partial` methods (via `?` operator):**
//!
//! The `From<ReflectError>` impl automatically captures the span from the
//! thread-local `SpanGuard` and the path from the `ReflectError`. Just use `?`:
//!
//! ```ignore
//! let _guard = SpanGuard::new(self.last_span);
//! wip = wip.begin_field("name")?;  // Error automatically has span + path
//! ```
//!
//! **For errors with `DeserializeErrorKind::with_span()`:**
//!
//! When you only have an error kind and span (no `wip` for path):
//!
//! ```ignore
//! return Err(DeserializeErrorKind::UnexpectedEof { expected: "value" }
//!     .with_span(self.last_span));
//! ```
//!
//! Note: `with_span()` sets `path: None`. Prefer the full struct when you have a path.
//!
//! ### ReflectError Conversion
//!
//! Errors from `facet-reflect` (the `Partial` API) are converted via `From<ReflectError>`.
//! This impl:
//!
//! 1. Captures the span from the thread-local `CURRENT_SPAN` (set by `SpanGuard`)
//! 2. Preserves the path from `ReflectError::path`
//! 3. Wraps the error kind in `DeserializeErrorKind::Reflect`
//!
//! This means you must have an active `SpanGuard` when calling `Partial` methods
//! that might fail. The guard is typically created at the start of each
//! deserialization method:
//!
//! ```ignore
//! pub fn deserialize_struct(&mut self, wip: Partial) -> Result<...> {
//!     let _guard = SpanGuard::new(self.last_span);
//!     // ... Partial methods can now use ? and errors will have spans
//! }
//! ```
//!
//! ## Method Chaining with `.with()`
//!
//! The `Partial` API provides a `.with()` method for cleaner chaining when you
//! need to call deserializer methods (which take `&mut self`) in the middle of
//! a chain:
//!
//! ```ignore
//! // Instead of:
//! wip = wip.begin_field("name")?;
//! wip = self.deserialize_into(wip)?;
//! wip = wip.end()?;
//!
//! // Use:
//! wip = wip
//!     .begin_field("name")?
//!     .with(|w| self.deserialize_into(w))?
//!     .end()?;
//! ```
//!
//! This keeps the "begin/deserialize/end" pattern visually grouped and makes
//! the nesting structure clearer.

use std::marker::PhantomData;

use facet_core::{Facet, Shape};
use facet_reflect::{HeapValue, Partial, Span};

use crate::{FormatParser, ParseEvent};

mod error;
pub use error::*;

/// Convenience setters for string etc.
mod setters;

/// Entry point for deserialization
mod entry;

/// Deserialization of dynamic values
mod dynamic;

/// Enum handling
mod eenum;

/// Smart pointers (Box, Arc, etc.)
mod pointer;

/// Check if a scalar matches a target shape
mod scalar_matches;

/// Simple struct deserialization (no flatten)
mod struct_simple;

/// Not-so-simple struct deserialization (flatten)
mod struct_with_flatten;

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
