extern crate alloc;

use alloc::borrow::Cow;
use alloc::format;

use facet_core::{Def, Facet, Shape, StructKind, Type, UserType};
pub use facet_path::{Path, PathStep};
use facet_reflect::{HeapValue, Partial};

use crate::{ContainerKind, DynParser, FormatParser, ParseEvent, ScalarTypeHint, ScalarValue};

mod error;
pub use error::*;

pub(crate) mod dyn_helpers;
mod dynamic;
mod eenum;
mod pointer;
mod scalar_matches;
mod setters;
mod struct_simple;
mod struct_with_flatten;
mod validate;

/// Type alias for a deserializer using dynamic dispatch.
///
/// This uses `&mut dyn DynParser<'input>` as the parser, which allows a single
/// monomorphization of `FormatDeserializer` to work with any format parser at runtime.
///
/// # Tradeoffs
///
/// - **Pros**: Reduces monomorphization bloat (one copy instead of N copies for N formats)
/// - **Cons**: Dynamic dispatch overhead (likely negligible), parser errors are stringified
///
/// # Example
///
/// ```ignore
/// use facet_json::JsonParser;
/// use facet_format::{DynDeserializer, DynParser, FormatDeserializer};
///
/// let input = r#"{"name": "Alice"}"#;
/// let mut parser = JsonParser::new(input.as_bytes());
/// let mut dyn_parser: &mut dyn DynParser = &mut parser;
/// let mut de: DynDeserializer = FormatDeserializer::new(dyn_parser);
/// let value: MyStruct = de.deserialize().unwrap();
/// ```
pub type DynDeserializer<'input, 'p> =
    FormatDeserializer<'input, true, &'p mut dyn DynParser<'input>>;

/// Type alias for a deserializer using dynamic dispatch (owned strings variant).
///
/// Same as [`DynDeserializer`] but produces owned strings instead of borrowing from input.
pub type DynDeserializerOwned<'input, 'p> =
    FormatDeserializer<'input, false, &'p mut dyn DynParser<'input>>;

/// Generic deserializer that drives a format-specific parser directly into `Partial`.
///
/// The const generic `BORROW` controls whether string data can be borrowed:
/// - `BORROW=true`: strings without escapes are borrowed from input
/// - `BORROW=false`: all strings are owned
pub struct FormatDeserializer<'input, const BORROW: bool, P> {
    parser: P,
    /// The span of the most recently consumed event (for error reporting).
    last_span: Option<facet_reflect::Span>,
    /// Current path through the type structure (for error reporting).
    current_path: Path,
    _marker: core::marker::PhantomData<&'input ()>,
}

impl<'input, P> FormatDeserializer<'input, true, P> {
    /// Create a new deserializer that can borrow strings from input.
    pub const fn new(parser: P) -> Self {
        Self {
            parser,
            last_span: None,
            current_path: Path::new(),
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'input, P> FormatDeserializer<'input, false, P> {
    /// Create a new deserializer that produces owned strings.
    pub const fn new_owned(parser: P) -> Self {
        Self {
            parser,
            last_span: None,
            current_path: Path::new(),
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P> {
    /// Consume the facade and return the underlying parser.
    pub fn into_inner(self) -> P {
        self.parser
    }

    /// Borrow the inner parser mutably.
    pub const fn parser_mut(&mut self) -> &mut P {
        &mut self.parser
    }
}

impl<'input, P> FormatDeserializer<'input, true, P>
where
    P: FormatParser<'input>,
{
    /// Deserialize the next value in the stream into `T`, allowing borrowed strings.
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'input>,
    {
        let wip: Partial<'input, true> = Partial::alloc::<T>()?;

        // Create a dyn-dispatched view for the actual deserialization work.
        // This ensures the deserialization logic is monomorphized only once (per BORROW value)
        // instead of once per parser type.
        let dyn_parser: &mut dyn DynParser<'input> = &mut self.parser;
        let mut dyn_deser = FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };

        let partial = dyn_deser.deserialize_into(wip)?;

        // Sync state back
        self.last_span = dyn_deser.last_span;

        let heap_value: HeapValue<'input, true> = partial.build()?;
        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
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
        let wip: Partial<'input, true> = Partial::alloc::<T>()?;
        let wip = wip.begin_deferred()?;

        // Create a dyn-dispatched view for the actual deserialization work.
        let dyn_parser: &mut dyn DynParser<'input> = &mut self.parser;
        let mut dyn_deser = FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };

        let partial = dyn_deser.deserialize_into(wip)?;

        // Sync state back
        self.last_span = dyn_deser.last_span;

        let partial = partial.finish_deferred()?;
        let heap_value: HeapValue<'input, true> = partial.build()?;
        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
    }
}

impl<'input, P> FormatDeserializer<'input, false, P>
where
    P: FormatParser<'input>,
{
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
                Partial::alloc_owned::<T>()?,
            )
        };

        // Create a dyn-dispatched view for the actual deserialization work.
        let dyn_parser: &mut dyn DynParser<'input> = &mut self.parser;
        let mut dyn_deser = FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };

        let partial = dyn_deser.deserialize_into(wip)?;

        // Sync state back
        self.last_span = dyn_deser.last_span;

        let heap_value: HeapValue<'input, false> = partial.build()?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
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
                Partial::alloc_owned::<T>()?,
            )
        };
        let wip = wip.begin_deferred()?;

        // Create a dyn-dispatched view for the actual deserialization work.
        let dyn_parser: &mut dyn DynParser<'input> = &mut self.parser;
        let mut dyn_deser = FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };

        let partial = dyn_deser.deserialize_into(wip)?;

        // Sync state back
        self.last_span = dyn_deser.last_span;

        let partial = partial.finish_deferred()?;
        let heap_value: HeapValue<'input, false> = partial.build()?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
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
                Partial::alloc_owned::<T>()?,
            )
        };

        // Create a dyn-dispatched view for the actual deserialization work.
        let dyn_parser: &mut dyn DynParser<'input> = &mut self.parser;
        let mut dyn_deser = FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };

        let partial = dyn_deser.deserialize_into_with_shape(wip, source_shape)?;

        // Sync state back
        self.last_span = dyn_deser.last_span;

        let heap_value: HeapValue<'input, false> = partial.build()?;

        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
    }
}

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    /// Read the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_event(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        let event = self
            .parser
            .next_event()
            .map_err(DeserializeError::parser)?
            .ok_or(DeserializeError::unexpected_eof(expected))?;
        trace!(?event, expected, "expect_event: got event");
        // Capture the span of the consumed event for error reporting
        self.last_span = self.parser.current_span();
        Ok(event)
    }

    /// Peek at the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_peek(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        let event = self
            .parser
            .peek_event()
            .map_err(DeserializeError::parser)?
            .ok_or(DeserializeError::unexpected_eof(expected))?;
        trace!(?event, expected, "expect_peek: peeked event");
        Ok(event)
    }

    /// Push a step onto the current path (for error reporting).
    #[inline]
    fn push_path(&mut self, step: PathStep) {
        self.current_path.push(step);
    }

    /// Pop the last step from the current path.
    #[inline]
    fn pop_path(&mut self) {
        self.current_path.pop();
    }

    /// Get a clone of the current path (for attaching to errors).
    #[inline]
    fn path_clone(&self) -> Path {
        self.current_path.clone()
    }

    /// Main deserialization entry point - deserialize into a Partial.
    pub fn deserialize_into(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let shape = wip.shape();
        trace!(
            shape_name = shape.type_identifier,
            "deserialize_into: starting"
        );

        // Check for raw capture type (e.g., RawJson)
        // Raw capture types are tuple structs with a single Cow<str> field
        // If capture_raw returns None (e.g., streaming mode), fall through
        // and try normal deserialization (which will likely fail with a helpful error)
        if self.parser.raw_capture_shape() == Some(shape)
            && let Some(raw) = self
                .parser
                .capture_raw()
                .map_err(DeserializeError::parser)?
        {
            // The raw type is a tuple struct like RawJson(Cow<str>)
            // Access field 0 (the Cow<str>) and set it
            wip = wip.begin_nth_field(0)?;
            wip = self.set_string_value(wip, Cow::Borrowed(raw))?;
            wip = wip.end()?;
            return Ok(wip);
        }

        // Check for container-level proxy (format-specific proxies take precedence)
        let format_ns = self.parser.format_namespace();
        let (wip_returned, has_proxy) =
            wip.begin_custom_deserialization_from_shape_with_format(format_ns)?;
        wip = wip_returned;
        if has_proxy {
            wip = self.deserialize_into(wip)?;
            return wip.end().map_err(DeserializeError::reflect);
        }

        // Check for field-level proxy (opaque types with proxy attribute)
        // Format-specific proxies take precedence over format-agnostic proxies
        if wip
            .parent_field()
            .and_then(|field| field.effective_proxy(format_ns))
            .is_some()
        {
            wip = wip.begin_custom_deserialization_with_format(format_ns)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
            return Ok(wip);
        }

        // Check Def first for Option
        if matches!(&shape.def, Def::Option(_)) {
            return self.deserialize_option(wip);
        }

        // Check Def for Result - treat it as a 2-variant enum
        if matches!(&shape.def, Def::Result(_)) {
            return self.deserialize_result_as_enum(wip);
        }

        // Priority 1: Check for builder_shape (immutable collections like Bytes -> BytesMut)
        if shape.builder_shape.is_some() {
            wip = wip.begin_inner()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
            return Ok(wip);
        }

        // Priority 2: Check for smart pointers (Box, Arc, Rc)
        if matches!(&shape.def, Def::Pointer(_)) {
            return self.deserialize_pointer(wip);
        }

        // Priority 3: Check for .inner (transparent wrappers like NonZero)
        // Collections (List/Map/Set/Array) have .inner for variance but shouldn't use this path
        // Opaque scalars (like ULID) may have .inner for documentation but should NOT be
        // deserialized as transparent wrappers - they use hint_opaque_scalar instead
        let is_opaque_scalar =
            matches!(shape.def, Def::Scalar) && matches!(shape.ty, Type::User(UserType::Opaque));
        if shape.inner.is_some()
            && !is_opaque_scalar
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

        // Priority 4: Check for metadata containers (like Spanned<T>, Documented<T>)
        // These deserialize transparently - the value field gets the data,
        // metadata fields are populated from parser state (span, doc, tag, etc.)
        if shape.is_metadata_container() {
            trace!("deserialize_into: metadata container detected");
            if let Type::User(UserType::Struct(st)) = &shape.ty {
                for field in st.fields {
                    match field.metadata_kind() {
                        Some("span") => {
                            // Populate span from parser's current position
                            wip = wip.begin_field(field.effective_name())?;
                            if let Some(span) = self.last_span {
                                wip = wip.begin_some()?;
                                // Set the span struct fields
                                wip = wip.begin_field("offset")?;
                                wip = wip.set(span.offset)?;
                                wip = wip.end()?;
                                wip = wip.begin_field("len")?;
                                wip = wip.set(span.len)?;
                                wip = wip.end()?;
                                wip = wip.end()?;
                            } else {
                                wip = wip.set_default()?;
                            }
                            wip = wip.end()?;
                        }
                        Some(_other) => {
                            // Other metadata types (doc, tag) - set to default for now
                            wip = wip.begin_field(field.effective_name())?;
                            wip = wip.set_default()?;
                            wip = wip.end()?;
                        }
                        None => {
                            // This is the value field - recurse into it
                            wip = wip.begin_field(field.effective_name())?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end()?;
                        }
                    }
                }
            }
            return Ok(wip);
        }

        // Priority 5: Check the Type for structs and enums
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                    trace!("deserialize_into: dispatching to deserialize_tuple");
                    return self.deserialize_tuple(wip);
                }
                trace!("deserialize_into: dispatching to deserialize_struct");
                return self.deserialize_struct(wip);
            }
            Type::User(UserType::Enum(_)) => {
                trace!("deserialize_into: dispatching to deserialize_enum");
                return self.deserialize_enum(wip);
            }
            _ => {}
        }

        // Priority 6: Check Def for containers and scalars
        match &shape.def {
            Def::Scalar => {
                trace!("deserialize_into: dispatching to deserialize_scalar");
                self.deserialize_scalar(wip)
            }
            Def::List(_) => {
                trace!("deserialize_into: dispatching to deserialize_list");
                self.deserialize_list(wip)
            }
            Def::Map(_) => {
                trace!("deserialize_into: dispatching to deserialize_map");
                self.deserialize_map(wip)
            }
            Def::Array(_) => {
                trace!("deserialize_into: dispatching to deserialize_array");
                self.deserialize_array(wip)
            }
            Def::Set(_) => {
                trace!("deserialize_into: dispatching to deserialize_set");
                self.deserialize_set(wip)
            }
            Def::DynamicValue(_) => {
                trace!("deserialize_into: dispatching to deserialize_dynamic_value");
                self.deserialize_dynamic_value(wip)
            }
            _ => Err(DeserializeError::unsupported(format!(
                "unsupported shape def: {:?}",
                shape.def
            ))),
        }
    }

    /// Deserialize using an explicit source shape for parser hints.
    ///
    /// This walks `hint_shape` for control flow and parser hints, but builds
    /// into the `wip` Partial (which should be a DynamicValue like `Value`).
    pub fn deserialize_into_with_shape(
        &mut self,
        wip: Partial<'input, BORROW>,
        hint_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        self.deserialize_value_recursive(wip, hint_shape)
    }

    /// Internal recursive deserialization using hint_shape for dispatch.
    fn deserialize_value_recursive(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        hint_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Handle Option
        if let Def::Option(opt_def) = &hint_shape.def {
            self.parser.hint_option();
            let event = self.expect_peek("value for option")?;
            // Treat both Null and Unit as None
            // Unit is used by Styx for tags without payload (e.g., @string vs @string{...})
            if matches!(
                event,
                ParseEvent::Scalar(ScalarValue::Null | ScalarValue::Unit)
            ) {
                let _ = self.expect_event("null or unit")?;
                wip = wip.set_default()?;
            } else {
                wip = self.deserialize_value_recursive(wip, opt_def.t)?;
            }
            return Ok(wip);
        }

        // Handle smart pointers - unwrap to inner type
        if let Def::Pointer(ptr_def) = &hint_shape.def
            && let Some(pointee) = ptr_def.pointee()
        {
            return self.deserialize_value_recursive(wip, pointee);
        }

        // Handle transparent wrappers (but not collections)
        if let Some(inner) = hint_shape.inner
            && !matches!(
                &hint_shape.def,
                Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
            )
        {
            return self.deserialize_value_recursive(wip, inner);
        }

        // Dispatch based on hint shape type
        match &hint_shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                    self.deserialize_tuple_dynamic(wip, struct_def.fields)
                } else {
                    self.deserialize_struct_dynamic(wip, struct_def.fields)
                }
            }
            Type::User(UserType::Enum(enum_def)) => self.deserialize_enum_dynamic(wip, enum_def),
            _ => match &hint_shape.def {
                Def::Scalar => self.deserialize_scalar_dynamic(wip, hint_shape),
                Def::List(list_def) => self.deserialize_list_dynamic(wip, list_def.t),
                Def::Array(array_def) => {
                    self.deserialize_array_dynamic(wip, array_def.t, array_def.n)
                }
                Def::Map(map_def) => self.deserialize_map_dynamic(wip, map_def.k, map_def.v),
                Def::Set(set_def) => self.deserialize_list_dynamic(wip, set_def.t),
                _ => Err(DeserializeError::unsupported(format!(
                    "unsupported hint shape for dynamic deserialization: {:?}",
                    hint_shape.def
                ))),
            },
        }
    }

    fn deserialize_option(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Hint to non-self-describing parsers that an Option is expected
        self.parser.hint_option();

        let event = self.expect_peek("value for option")?;

        // Treat both Null and Unit as None
        // Unit is used by Styx for tags without payload (e.g., @string vs @string{...})
        if matches!(
            event,
            ParseEvent::Scalar(ScalarValue::Null | ScalarValue::Unit)
        ) {
            // Consume the null/unit
            let _ = self.expect_event("null or unit")?;
            // Set to None (default)
            wip = wip.set_default()?;
        } else {
            // Some(value)
            wip = wip.begin_some()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }
        Ok(wip)
    }

    /// Check if a field matches a given name by effective name or alias.
    fn field_matches(field: &facet_core::Field, name: &str) -> bool {
        field.effective_name() == name || field.alias.iter().any(|alias| *alias == name)
    }

    fn deserialize_struct(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Get struct fields for lookup
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DeserializeError::unsupported(format!(
                    "expected struct type but got {:?}",
                    wip.shape().ty
                )));
            }
        };

        // Check if we have any flattened fields
        let has_flatten = struct_def.fields.iter().any(|f| f.is_flattened());

        if has_flatten {
            self.deserialize_struct_with_flatten(wip)
        } else {
            self.deserialize_struct_simple(wip)
        }
    }

    fn deserialize_tuple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Get field count for tuple hints
        let field_count = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def.fields.len(),
            _ => 0, // Unit type or unknown - will be handled below
        };

        // Hint to non-self-describing parsers how many fields to expect
        // Tuples are like positional structs, so we use hint_struct_fields
        self.parser.hint_struct_fields(field_count);

        // Special case: transparent newtypes (marked with #[facet(transparent)] or
        // #[repr(transparent)]) can accept values directly without a sequence wrapper.
        // This enables patterns like:
        //   #[facet(transparent)]
        //   struct Wrapper(i32);
        //   toml: "value = 42"  ->  Wrapper(42)
        // Plain tuple structs without the transparent attribute use array syntax.
        if field_count == 1 && wip.shape().is_transparent() {
            // Unwrap into field "0" and deserialize directly
            wip = wip.begin_field("0")?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
            return Ok(wip);
        }

        // Special case: unit type () can accept Scalar(Unit) or Scalar(Null) directly
        // This enables patterns like styx bare identifiers: { id, name } -> IndexMap<String, ()>
        // and JSON null values for unit types (e.g., ConfigValue::Null(Spanned<()>))
        if field_count == 0
            && matches!(
                self.expect_peek("value")?,
                ParseEvent::Scalar(ScalarValue::Unit | ScalarValue::Null)
            )
        {
            self.expect_event("value")?; // consume the unit/null scalar
            return Ok(wip);
        }

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (for
        // non-self-describing formats like postcard where tuples are positional structs)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            // For non-self-describing formats, StructStart(Object) is valid for tuples
            // because hint_struct_fields was called and tuples are positional structs
            ParseEvent::StructStart(_) if !self.parser.is_self_describing() => true,
            // For self-describing formats like TOML/JSON, objects with numeric keys
            // (e.g., { "0" = true, "1" = 1 }) are valid tuple representations
            ParseEvent::StructStart(ContainerKind::Object) => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "array",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "sequence start for tuple",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        let mut index = 0usize;
        loop {
            let event = self.expect_peek("value")?;

            // Check for end of container
            if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                self.expect_event("value")?;
                break;
            }

            // In struct mode, skip FieldKey events
            if struct_mode && matches!(event, ParseEvent::FieldKey(_)) {
                self.expect_event("value")?;
                continue;
            }

            // Select field by index
            let field_name = alloc::string::ToString::to_string(&index);
            wip = wip.begin_field(&field_name)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
            index += 1;
        }

        Ok(wip)
    }

    /// Helper to collect field evidence using save/restore.
    ///
    /// This saves the parser position, reads through the current struct to
    /// collect field names and their scalar values, then restores the position.
    fn collect_evidence(
        &mut self,
    ) -> Result<alloc::vec::Vec<crate::FieldEvidence<'input>>, DeserializeError> {
        use crate::{FieldEvidence, FieldLocationHint};

        let save_point = self.parser.save();

        let mut evidence = alloc::vec::Vec::new();
        let mut depth = 0i32;
        let mut pending_field_name: Option<alloc::borrow::Cow<'input, str>> = None;

        // Read through the structure
        loop {
            let event = self.parser.next_event().map_err(DeserializeError::parser)?;
            let Some(event) = event else { break };

            match event {
                ParseEvent::StructStart(_) => {
                    depth += 1;
                    // If we were expecting a value, record field with no scalar
                    if depth > 1
                        && let Some(name) = pending_field_name.take()
                    {
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: None,
                        });
                    }
                }
                ParseEvent::StructEnd => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ParseEvent::SequenceStart(_) => {
                    depth += 1;
                    // If we were expecting a value, record field with no scalar
                    if let Some(name) = pending_field_name.take() {
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: None,
                        });
                    }
                }
                ParseEvent::SequenceEnd => {
                    depth -= 1;
                }
                ParseEvent::FieldKey(key) => {
                    // If there's a pending field, record it without a value
                    if let Some(name) = pending_field_name.take() {
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: None,
                        });
                    }
                    if depth == 1 {
                        // Top-level field - save name, wait for value
                        pending_field_name = key.name;
                    }
                }
                ParseEvent::Scalar(scalar) => {
                    if let Some(name) = pending_field_name.take() {
                        // Record field with its scalar value
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: Some(scalar),
                        });
                    }
                }
                ParseEvent::OrderedField | ParseEvent::VariantTag(_) => {}
            }
        }

        // Handle any remaining pending field
        if let Some(name) = pending_field_name.take() {
            evidence.push(FieldEvidence {
                name,
                location: FieldLocationHint::KeyValue,
                value_type: None,
                scalar_value: None,
            });
        }

        self.parser.restore(save_point);
        Ok(evidence)
    }

    fn deserialize_list(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        trace!("deserialize_list: starting");

        // Check if this is a Vec<u8> - if so, try the optimized byte sequence path
        // We specifically check for Vec (not Bytes, BytesMut, or other list-like types)
        // because those types may have different builder patterns
        let is_byte_vec = wip.shape().type_identifier == "Vec"
            && matches!(
                &wip.shape().def,
                Def::List(list_def) if list_def.t.type_identifier == "u8"
            );

        if is_byte_vec && self.parser.hint_byte_sequence() {
            // Parser supports bulk byte reading - expect Scalar(Bytes(...))
            let event = self.expect_event("bytes")?;
            trace!(?event, "deserialize_list: got bytes event");

            return match event {
                ParseEvent::Scalar(ScalarValue::Bytes(bytes)) => self.set_bytes_value(wip, bytes),
                _ => Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "bytes",
                        got: event.kind_name().into(),
                    },
                }),
            };
        }

        // Fallback: element-by-element deserialization
        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;
        trace!(?event, "deserialize_list: got container start event");

        // Expect SequenceStart for lists
        match event {
            ParseEvent::SequenceStart(_) => {
                trace!("deserialize_list: got sequence start");
            }
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "array",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "sequence start",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        // Initialize the list
        wip = wip.init_list()?;
        trace!("deserialize_list: initialized list, starting loop");

        loop {
            let event = self.expect_peek("value")?;
            trace!(?event, "deserialize_list: loop iteration");

            // Check for end of sequence
            if matches!(event, ParseEvent::SequenceEnd) {
                self.expect_event("value")?;
                trace!("deserialize_list: reached end of sequence");
                break;
            }

            trace!("deserialize_list: deserializing list item");
            wip = wip.begin_list_item()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }

        trace!("deserialize_list: completed");
        Ok(wip)
    }

    fn deserialize_array(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Get the fixed array length from the type definition
        let array_len = match &wip.shape().def {
            Def::Array(array_def) => array_def.n,
            _ => {
                return Err(DeserializeError::unsupported(
                    "deserialize_array called on non-array type",
                ));
            }
        };

        // Hint to non-self-describing parsers that a fixed-size array is expected
        // (unlike hint_sequence, this doesn't read a length prefix)
        self.parser.hint_array(array_len);

        let event = self.expect_event("value")?;

        // Expect SequenceStart for arrays
        match event {
            ParseEvent::SequenceStart(_) => {}
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "array",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "sequence start for array",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        // Transition to Array tracker state. This is important for empty arrays
        // like [u8; 0] which have no elements to initialize but still need
        // their tracker state set correctly for require_full_initialization to pass.
        wip = wip.init_array()?;

        let mut index = 0usize;
        loop {
            let event = self.expect_peek("value")?;

            // Check for end of sequence
            if matches!(event, ParseEvent::SequenceEnd) {
                self.expect_event("value")?;
                break;
            }

            wip = wip.begin_nth_field(index)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
            index += 1;
        }

        Ok(wip)
    }

    fn deserialize_set(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;

        // Expect SequenceStart for sets
        match event {
            ParseEvent::SequenceStart(_) => {}
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "set",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "sequence start for set",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        // Initialize the set
        wip = wip.init_set()?;

        loop {
            let event = self.expect_peek("value")?;

            // Check for end of sequence
            if matches!(event, ParseEvent::SequenceEnd) {
                self.expect_event("value")?;
                break;
            }

            wip = wip.begin_set_item()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }

        Ok(wip)
    }

    fn deserialize_map(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // For non-self-describing formats, hint that a map is expected
        self.parser.hint_map();

        let event = self.expect_event("value")?;

        // Initialize the map
        wip = wip.init_map()?;

        // Handle both self-describing (StructStart) and non-self-describing (SequenceStart) formats
        match event {
            ParseEvent::StructStart(_) => {
                // Self-describing format (e.g., JSON): maps are represented as objects
                loop {
                    let event = self.expect_event("value")?;
                    match event {
                        ParseEvent::StructEnd => break,
                        ParseEvent::FieldKey(key) => {
                            // Begin key
                            wip = wip.begin_key()?;
                            wip = self.deserialize_map_key(wip, key.name, key.doc, key.tag)?;
                            wip = wip.end()?;

                            // Begin value
                            wip = wip.begin_value()?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end()?;
                        }
                        other => {
                            return Err(DeserializeError {
                                span: self.last_span,
                                path: None,
                                kind: DeserializeErrorKind::TypeMismatchStr {
                                    expected: "field key or struct end for map",
                                    got: other.kind_name().into(),
                                },
                            });
                        }
                    }
                }
            }
            ParseEvent::SequenceStart(_) => {
                // Non-self-describing format (e.g., postcard): maps are sequences of key-value pairs
                loop {
                    let event = self.expect_peek("value")?;
                    match event {
                        ParseEvent::SequenceEnd => {
                            self.expect_event("value")?;
                            break;
                        }
                        ParseEvent::OrderedField => {
                            self.expect_event("value")?;

                            // Deserialize key
                            wip = wip.begin_key()?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end()?;

                            // Deserialize value
                            wip = wip.begin_value()?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end()?;
                        }
                        other => {
                            return Err(DeserializeError {
                                span: self.last_span,
                                path: None,
                                kind: DeserializeErrorKind::TypeMismatchStr {
                                    expected: "ordered field or sequence end for map",
                                    got: other.kind_name().into(),
                                },
                            });
                        }
                    }
                }
            }
            other => {
                return Err(DeserializeError {
                    span: self.last_span,
                    path: None,
                    kind: DeserializeErrorKind::TypeMismatchStr {
                        expected: "struct start or sequence start for map",
                        got: other.kind_name().into(),
                    },
                });
            }
        }

        Ok(wip)
    }

    fn deserialize_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Hint to non-self-describing parsers what scalar type is expected
        let shape = wip.shape();

        // First, try hint_opaque_scalar for types that may have format-specific
        // binary representations (e.g., UUID as 16 raw bytes in postcard)
        let opaque_handled = match shape.type_identifier {
            // Standard primitives are never opaque
            "bool" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32"
            | "i64" | "i128" | "isize" | "f32" | "f64" | "String" | "&str" | "char" => false,
            // For all other scalar types, ask the parser if it handles them specially
            _ => self.parser.hint_opaque_scalar(shape.type_identifier, shape),
        };

        // If the parser didn't handle the opaque type, fall back to standard hints
        if !opaque_handled {
            let hint = match shape.type_identifier {
                "bool" => Some(ScalarTypeHint::Bool),
                "u8" => Some(ScalarTypeHint::U8),
                "u16" => Some(ScalarTypeHint::U16),
                "u32" => Some(ScalarTypeHint::U32),
                "u64" => Some(ScalarTypeHint::U64),
                "u128" => Some(ScalarTypeHint::U128),
                "usize" => Some(ScalarTypeHint::Usize),
                "i8" => Some(ScalarTypeHint::I8),
                "i16" => Some(ScalarTypeHint::I16),
                "i32" => Some(ScalarTypeHint::I32),
                "i64" => Some(ScalarTypeHint::I64),
                "i128" => Some(ScalarTypeHint::I128),
                "isize" => Some(ScalarTypeHint::Isize),
                "f32" => Some(ScalarTypeHint::F32),
                "f64" => Some(ScalarTypeHint::F64),
                "String" | "&str" => Some(ScalarTypeHint::String),
                "char" => Some(ScalarTypeHint::Char),
                // For unknown scalar types, check if they implement FromStr
                // (e.g., camino::Utf8PathBuf, types not handled by hint_opaque_scalar)
                _ if shape.is_from_str() => Some(ScalarTypeHint::String),
                _ => None,
            };
            if let Some(hint) = hint {
                self.parser.hint_scalar_type(hint);
            }
        }

        let event = self.expect_event("value")?;

        match event {
            ParseEvent::Scalar(scalar) => {
                wip = self.set_scalar(wip, scalar)?;
                Ok(wip)
            }
            ParseEvent::StructStart(_container_kind) => {
                // When deserializing into a scalar, extract the _arg value.
                let mut found_scalar: Option<ScalarValue<'input>> = None;

                loop {
                    let inner_event = self.expect_event("field or struct end")?;
                    match inner_event {
                        ParseEvent::StructEnd => break,
                        ParseEvent::FieldKey(key) => {
                            // Look for _arg field (single argument)
                            if key.name.as_deref() == Some("_arg") {
                                let value_event = self.expect_event("argument value")?;
                                if let ParseEvent::Scalar(scalar) = value_event {
                                    found_scalar = Some(scalar);
                                } else {
                                    // Skip non-scalar argument
                                    self.parser.skip_value().map_err(DeserializeError::parser)?;
                                }
                            } else {
                                // Skip other fields (_node_name, _arguments, properties, etc.)
                                self.parser.skip_value().map_err(DeserializeError::parser)?;
                            }
                        }
                        _ => {
                            // Skip unexpected events
                        }
                    }
                }

                if let Some(scalar) = found_scalar {
                    wip = self.set_scalar(wip, scalar)?;
                    Ok(wip)
                } else {
                    Err(DeserializeError {
                        span: self.last_span,
                        path: None,
                        kind: DeserializeErrorKind::TypeMismatchStr {
                            expected: "scalar value or node with argument",
                            got: "node without argument".into(),
                        },
                    })
                }
            }
            other => Err(DeserializeError {
                span: self.last_span,
                path: None,
                kind: DeserializeErrorKind::TypeMismatchStr {
                    expected: "scalar value",
                    got: other.kind_name().into(),
                },
            }),
        }
    }

    /// Deserialize a map key from a string or tag.
    ///
    /// Format parsers typically emit string keys, but the target map might have non-string key types
    /// (e.g., integers, enums). This function parses the string key into the appropriate type:
    /// - String types: set directly
    /// - Enum unit variants: use select_variant_named
    /// - Integer types: parse the string as a number
    /// - Transparent newtypes: descend into the inner type
    /// - Option types: None key becomes None, Some(key) recurses into inner type
    /// - Metadata containers (like `Documented<T>`): populate doc/tag metadata and recurse into value
    ///
    /// The `tag` parameter is for formats like Styx where keys can be type patterns (e.g., `@string`).
    /// When present, it indicates the key was a tag rather than a bare identifier.
    fn deserialize_map_key(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        key: Option<Cow<'input, str>>,
        doc: Option<Vec<Cow<'input, str>>>,
        tag: Option<Cow<'input, str>>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let shape = wip.shape();

        trace!(shape_name = %shape, shape_def = ?shape.def, ?key, ?doc, ?tag, "deserialize_map_key");

        // Handle metadata containers (like Documented<T> or ObjectKey): populate metadata and recurse into value
        if shape.is_metadata_container() {
            trace!("deserialize_map_key: metadata container detected");

            // Find field info from the shape's struct type
            if let Type::User(UserType::Struct(st)) = &shape.ty {
                for field in st.fields {
                    if field.metadata_kind() == Some("doc") {
                        // This is the doc field - set it from the doc parameter
                        wip = wip.begin_field(field.effective_name())?;
                        if let Some(ref doc_lines) = doc {
                            // Set as Some(Vec<String>)
                            wip = wip.begin_some()?;
                            wip = wip.init_list()?;
                            for line in doc_lines {
                                wip = wip.begin_list_item()?;
                                wip = self.set_string_value(wip, line.clone())?;
                                wip = wip.end()?;
                            }
                            wip = wip.end()?;
                        } else {
                            // Set as None
                            wip = wip.set_default()?;
                        }
                        wip = wip.end()?;
                    } else if field.metadata_kind() == Some("tag") {
                        // This is the tag field - set it from the tag parameter
                        wip = wip.begin_field(field.effective_name())?;
                        if let Some(ref tag_name) = tag {
                            // Set as Some(String)
                            wip = wip.begin_some()?;
                            wip = self.set_string_value(wip, tag_name.clone())?;
                            wip = wip.end()?;
                        } else {
                            // Set as None (not a tagged key)
                            wip = wip.set_default()?;
                        }
                        wip = wip.end()?;
                    } else if field.metadata_kind().is_none() {
                        // This is the value field - recurse with the key and tag.
                        // Doc is already consumed by this container, but tag may be needed
                        // by a nested metadata container (e.g., Documented<ObjectKey>).
                        wip = wip.begin_field(field.effective_name())?;
                        wip = self.deserialize_map_key(wip, key.clone(), None, tag.clone())?;
                        wip = wip.end()?;
                    }
                }
            }

            return Ok(wip);
        }

        // Handle Option<T> key types: None key -> None variant, Some(key) -> Some(inner)
        if let Def::Option(_) = &shape.def {
            match key {
                None => {
                    // Unit key -> None variant (use set_default to mark as initialized)
                    wip = wip.set_default()?;
                    return Ok(wip);
                }
                Some(inner_key) => {
                    // Named key -> Some(inner)
                    wip = wip.begin_some()?;
                    wip = self.deserialize_map_key(wip, Some(inner_key), None, None)?;
                    wip = wip.end()?;
                    return Ok(wip);
                }
            }
        }

        // From here on, we need an actual key name
        let key = key.ok_or_else(|| DeserializeError {
            span: self.last_span,
            path: None,
            kind: DeserializeErrorKind::TypeMismatchStr {
                expected: "named key",
                got: "unit key".into(),
            },
        })?;

        // For transparent types (like UserId(String)), we need to use begin_inner
        // to set the inner value. But NOT for pointer types like &str or Cow<str>
        // which are handled directly.
        let is_pointer = matches!(shape.def, Def::Pointer(_));
        if shape.inner.is_some() && !is_pointer {
            wip = wip.begin_inner()?;
            wip = self.deserialize_map_key(wip, Some(key), None, None)?;
            wip = wip.end()?;
            return Ok(wip);
        }

        // Handle terminal cases (enum, numeric, string) via non-generic inner function
        use crate::deserializer::setters::{
            MapKeyTerminalResult, deserialize_map_key_terminal_inner,
        };
        match deserialize_map_key_terminal_inner(wip, key, self.last_span) {
            Ok(wip) => Ok(wip),
            Err(MapKeyTerminalResult::NeedsSetString { wip, s }) => self.set_string_value(wip, s),
            Err(MapKeyTerminalResult::Error(e)) => Err(e),
        }
    }
}
