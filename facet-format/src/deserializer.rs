extern crate alloc;

use alloc::borrow::Cow;
use alloc::format;

use facet_core::{Def, Facet, NumericType, PrimitiveType, Shape, StructKind, Type, UserType};
pub use facet_path::{Path, PathStep};
use facet_reflect::{HeapValue, Partial, is_spanned_shape};

use crate::{
    ContainerKind, FieldLocationHint, FormatParser, ParseEvent, ScalarTypeHint, ScalarValue,
};

mod error;
pub use error::*;

mod dynamic;
mod eenum;
mod pointer;
mod scalar_matches;
mod setters;
mod struct_simple;
mod struct_with_flatten;
mod validate;

/// Result of variant lookup for HTML/XML elements.
enum VariantMatch {
    /// Direct match: a variant with matching rename attribute.
    Direct(usize),
    /// Custom element fallback: a variant with `html::custom_element` or `xml::custom_element`.
    CustomElement(usize),
}

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
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'input>,
    {
        let wip: Partial<'input, true> =
            Partial::alloc::<T>().map_err(DeserializeError::reflect)?;
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, true> =
            partial.build().map_err(DeserializeError::reflect)?;
        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
    }

    /// Deserialize the next value in the stream into `T` (for backward compatibility).
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
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
    pub fn deserialize_deferred<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'input>,
    {
        let wip: Partial<'input, true> =
            Partial::alloc::<T>().map_err(DeserializeError::reflect)?;
        let wip = wip.begin_deferred().map_err(DeserializeError::reflect)?;
        let partial = self.deserialize_into(wip)?;
        let partial = partial
            .finish_deferred()
            .map_err(DeserializeError::reflect)?;
        let heap_value: HeapValue<'input, true> =
            partial.build().map_err(DeserializeError::reflect)?;
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
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        // SAFETY: alloc_owned produces Partial<'static, false>, but our deserializer
        // expects 'input. Since BORROW=false means we never borrow from input anyway,
        // this is safe. We also transmute the HeapValue back to 'static before materializing.
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>().map_err(DeserializeError::reflect)?,
            )
        };
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, false> =
            partial.build().map_err(DeserializeError::reflect)?;

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
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
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
    pub fn deserialize_deferred<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        // SAFETY: alloc_owned produces Partial<'static, false>, but our deserializer
        // expects 'input. Since BORROW=false means we never borrow from input anyway,
        // this is safe. We also transmute the HeapValue back to 'static before materializing.
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>().map_err(DeserializeError::reflect)?,
            )
        };
        let wip = wip.begin_deferred().map_err(DeserializeError::reflect)?;
        let partial = self.deserialize_into(wip)?;
        let partial = partial
            .finish_deferred()
            .map_err(DeserializeError::reflect)?;
        let heap_value: HeapValue<'input, false> =
            partial.build().map_err(DeserializeError::reflect)?;

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
    ) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>().map_err(DeserializeError::reflect)?,
            )
        };
        let partial = self.deserialize_into_with_shape(wip, source_shape)?;
        let heap_value: HeapValue<'input, false> =
            partial.build().map_err(DeserializeError::reflect)?;

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
    ) -> Result<ParseEvent<'input>, DeserializeError<P::Error>> {
        let event = self
            .parser
            .next_event()
            .map_err(DeserializeError::Parser)?
            .ok_or(DeserializeError::UnexpectedEof { expected })?;
        // Capture the span of the consumed event for error reporting
        self.last_span = self.parser.current_span();
        Ok(event)
    }

    /// Peek at the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_peek(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError<P::Error>> {
        self.parser
            .peek_event()
            .map_err(DeserializeError::Parser)?
            .ok_or(DeserializeError::UnexpectedEof { expected })
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
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Check for raw capture type (e.g., RawJson)
        // Raw capture types are tuple structs with a single Cow<str> field
        // If capture_raw returns None (e.g., streaming mode), fall through
        // and try normal deserialization (which will likely fail with a helpful error)
        if self.parser.raw_capture_shape() == Some(shape)
            && let Some(raw) = self
                .parser
                .capture_raw()
                .map_err(DeserializeError::Parser)?
        {
            // The raw type is a tuple struct like RawJson(Cow<str>)
            // Access field 0 (the Cow<str>) and set it
            wip = wip.begin_nth_field(0).map_err(DeserializeError::reflect)?;
            wip = self.set_string_value(wip, Cow::Borrowed(raw))?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check for container-level proxy
        let (wip_returned, has_proxy) = wip
            .begin_custom_deserialization_from_shape()
            .map_err(DeserializeError::reflect)?;
        wip = wip_returned;
        if has_proxy {
            wip = self.deserialize_into(wip)?;
            return wip.end().map_err(DeserializeError::reflect);
        }

        // Check for field-level proxy (opaque types with proxy attribute)
        if wip
            .parent_field()
            .and_then(|field| field.proxy_convert_in_fn())
            .is_some()
        {
            wip = wip
                .begin_custom_deserialization()
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
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
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
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
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Priority 4: Check for metadata-annotated types (like Spanned<T>)
        if is_spanned_shape(shape) {
            return self.deserialize_spanned(wip);
        }

        // Priority 5: Check the Type for structs and enums
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                    return self.deserialize_tuple(wip);
                }
                return self.deserialize_struct(wip);
            }
            Type::User(UserType::Enum(_)) => return self.deserialize_enum(wip),
            _ => {}
        }

        // Priority 6: Check Def for containers and scalars
        match &shape.def {
            Def::Scalar => self.deserialize_scalar(wip),
            Def::List(_) => self.deserialize_list(wip),
            Def::Map(_) => self.deserialize_map(wip),
            Def::Array(_) => self.deserialize_array(wip),
            Def::Set(_) => self.deserialize_set(wip),
            Def::DynamicValue(_) => self.deserialize_dynamic_value(wip),
            _ => Err(DeserializeError::Unsupported(format!(
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
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        self.deserialize_value_recursive(wip, hint_shape)
    }

    /// Internal recursive deserialization using hint_shape for dispatch.
    fn deserialize_value_recursive(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        hint_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Handle Option
        if let Def::Option(opt_def) = &hint_shape.def {
            self.parser.hint_option();
            let event = self.expect_peek("value for option")?;
            if matches!(event, ParseEvent::Scalar(ScalarValue::Null)) {
                let _ = self.expect_event("null")?;
                wip = wip.set_default().map_err(DeserializeError::reflect)?;
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
                _ => Err(DeserializeError::Unsupported(format!(
                    "unsupported hint shape for dynamic deserialization: {:?}",
                    hint_shape.def
                ))),
            },
        }
    }

    fn deserialize_option(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Hint to non-self-describing parsers that an Option is expected
        self.parser.hint_option();

        let event = self.expect_peek("value for option")?;

        if matches!(event, ParseEvent::Scalar(ScalarValue::Null)) {
            // Consume the null
            let _ = self.expect_event("null")?;
            // Set to None (default)
            wip = wip.set_default().map_err(DeserializeError::reflect)?;
        } else {
            // Some(value)
            wip = wip.begin_some().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }
        Ok(wip)
    }

    /// **XML matching:**
    /// - Text: Match fields with xml::text attribute (name is ignored - text content goes to the field)
    /// - Attributes: Only match if explicit xml::ns matches (no ns_all inheritance per XML spec)
    /// - Elements: Match if explicit xml::ns OR ns_all matches
    ///
    /// **Default (KeyValue):** Match by name/alias only (backwards compatible)
    ///
    /// TODO: This function hardcodes knowledge of XML attributes.
    /// See <https://github.com/facet-rs/facet/issues/1506> for discussion on
    /// making this more extensible.
    fn field_matches_with_namespace(
        field: &facet_core::Field,
        name: &str,
        namespace: Option<&str>,
        location: FieldLocationHint,
        ns_all: Option<&str>,
    ) -> bool {
        // === XML/HTML: Fields with xml::attribute match only attributes
        if field.is_attribute() && !matches!(location, FieldLocationHint::Attribute) {
            return false;
        }

        // === XML/HTML: Fields with xml::element/elements match only child elements
        if (field.is_element() || field.is_elements())
            && !matches!(location, FieldLocationHint::Child)
        {
            return false;
        }

        // === XML/HTML: Text location matches fields with text attribute ===
        // The name "_text" from the parser is ignored - we match by attribute presence
        if matches!(location, FieldLocationHint::Text) {
            return field.is_text();
        }

        // === XML/HTML: Tag location matches fields with html::tag or xml::tag attribute ===
        // The name "_tag" from the parser is ignored - we match by attribute presence
        // This allows custom elements to capture the element's tag name
        if matches!(location, FieldLocationHint::Tag) {
            return field.is_tag();
        }

        // === Check name/alias ===
        let name_matches = field.name == name || field.alias.iter().any(|alias| *alias == name);

        if !name_matches {
            return false;
        }

        // === XML: Namespace matching ===
        // Get the expected namespace for this field
        let field_xml_ns = field
            .get_attr(Some("xml"), "ns")
            .and_then(|attr| attr.get_as::<&str>().copied());

        // CRITICAL: Attributes don't inherit ns_all (per XML spec)
        let expected_ns = if matches!(location, FieldLocationHint::Attribute) {
            field_xml_ns // Attributes: only explicit xml::ns
        } else {
            field_xml_ns.or(ns_all) // Elements: xml::ns OR ns_all
        };

        // Check if namespaces match
        match (namespace, expected_ns) {
            (Some(input_ns), Some(expected)) => input_ns == expected,
            (Some(_input_ns), None) => true, // Input has namespace, field doesn't require one - match
            (None, Some(_expected)) => false, // Input has no namespace, field requires one - NO match
            (None, None) => true,             // Neither has namespace - match
        }
    }

    fn deserialize_struct(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Get struct fields for lookup
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DeserializeError::Unsupported(format!(
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

    /// Find an elements field that can accept a child element with the given name.
    fn find_elements_field_for_element<'a>(
        &self,
        fields: &'a [facet_core::Field],
        element_name: &str,
        element_ns: Option<&str>,
        ns_all: Option<&str>,
    ) -> Option<(usize, &'a facet_core::Field)> {
        for (idx, field) in fields.iter().enumerate() {
            if !field.is_elements() {
                continue;
            }

            // First check if element name matches the field's rename attribute
            // This handles cases like: `#[facet(xml::elements, rename = "author")] authors: Vec<Person>`
            // where XML element <author> should match the `authors` field
            if let Some(renamed) = field.rename
                && renamed.eq_ignore_ascii_case(element_name)
            {
                return Some((idx, field));
            }

            // Also check field aliases
            if field
                .alias
                .iter()
                .any(|a| a.eq_ignore_ascii_case(element_name))
            {
                return Some((idx, field));
            }

            // Get the list item shape
            let item_shape = Self::get_list_item_shape(field.shape())?;

            // Check if the item type can accept this element
            if Self::shape_accepts_element(item_shape, element_name, element_ns, ns_all) {
                return Some((idx, field));
            }
        }
        None
    }

    /// Get the item shape from a list-like field shape.
    const fn get_list_item_shape(shape: &facet_core::Shape) -> Option<&'static facet_core::Shape> {
        match &shape.def {
            Def::List(list_def) => Some(list_def.t()),
            _ => None,
        }
    }

    /// Check if a shape can accept an element with the given name.
    fn shape_accepts_element(
        shape: &facet_core::Shape,
        element_name: &str,
        _element_ns: Option<&str>,
        _ns_all: Option<&str>,
    ) -> bool {
        match &shape.ty {
            Type::User(UserType::Enum(enum_def)) => {
                // For enums, check if element name matches any variant
                let matches_variant = enum_def.variants.iter().any(|v| {
                    let display_name = Self::get_variant_display_name(v);
                    display_name.eq_ignore_ascii_case(element_name)
                });
                if matches_variant {
                    return true;
                }
                // Also check if enum has a custom_element fallback variant that can accept any element
                enum_def.variants.iter().any(|v| v.is_custom_element())
            }
            Type::User(UserType::Struct(struct_def)) => {
                // Similarly, if the struct has a tag field (for HTML/XML custom elements),
                // it can accept any element name
                if struct_def.fields.iter().any(|f| f.is_tag()) {
                    return true;
                }
                // Otherwise, check if element name matches struct's name
                // Use case-insensitive comparison since serializers may normalize case
                let display_name = Self::get_shape_display_name(shape);
                display_name.eq_ignore_ascii_case(element_name)
            }
            _ => {
                // For other types, use type identifier with case-insensitive comparison
                shape.type_identifier.eq_ignore_ascii_case(element_name)
            }
        }
    }

    /// Get the display name for a variant (respecting rename attribute).
    fn get_variant_display_name(variant: &facet_core::Variant) -> &'static str {
        if let Some(attr) = variant.get_builtin_attr("rename")
            && let Some(&renamed) = attr.get_as::<&str>()
        {
            return renamed;
        }
        variant.name
    }

    /// Get the display name for a shape (respecting rename attribute).
    fn get_shape_display_name(shape: &facet_core::Shape) -> &'static str {
        if let Some(renamed) = shape.get_builtin_attr_value::<&str>("rename") {
            return renamed;
        }
        shape.type_identifier
    }

    /// Find a variant by its display name (checking rename attributes).
    /// Returns the actual variant name to use with `select_variant_named`.
    fn find_variant_by_display_name<'a>(
        enum_def: &'a facet_core::EnumType,
        display_name: &str,
    ) -> Option<&'a str> {
        enum_def.variants.iter().find_map(|v| {
            let v_display_name = Self::get_variant_display_name(v);
            if v_display_name == display_name {
                Some(v.name)
            } else {
                None
            }
        })
    }

    /// Find the variant index for an enum that matches the given element name.
    ///
    /// First tries to find an exact match by name/rename. If no match is found,
    /// falls back to the `#[facet(html::custom_element)]` or `#[facet(xml::custom_element)]`
    /// variant if present.
    fn find_variant_for_element(
        enum_def: &facet_core::EnumType,
        element_name: &str,
    ) -> Option<VariantMatch> {
        // First try direct name match
        if let Some(idx) = enum_def.variants.iter().position(|v| {
            let display_name = Self::get_variant_display_name(v);
            display_name == element_name
        }) {
            return Some(VariantMatch::Direct(idx));
        }

        // Fall back to custom_element variant if present
        if let Some(idx) = enum_def.variants.iter().position(|v| v.is_custom_element()) {
            return Some(VariantMatch::CustomElement(idx));
        }

        None
    }

    /// Deserialize into a type with span metadata (like `Spanned<T>`).
    ///
    /// This handles structs that have:
    /// - One or more non-metadata fields (the actual values to deserialize)
    /// - A field with `#[facet(metadata = span)]` to store source location
    ///
    /// The metadata field is populated with a default span since most format parsers
    /// don't track source locations.
    fn deserialize_spanned(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Find the span metadata field and non-metadata fields
        let Type::User(UserType::Struct(struct_def)) = &shape.ty else {
            return Err(DeserializeError::Unsupported(format!(
                "expected struct with span metadata, found {}",
                shape.type_identifier
            )));
        };

        let span_field = struct_def
            .fields
            .iter()
            .find(|f| f.metadata_kind() == Some("span"))
            .ok_or_else(|| {
                DeserializeError::Unsupported(format!(
                    "expected struct with span metadata field, found {}",
                    shape.type_identifier
                ))
            })?;

        let value_fields: alloc::vec::Vec<_> = struct_def
            .fields
            .iter()
            .filter(|f| !f.is_metadata())
            .collect();

        // Deserialize all non-metadata fields transparently
        // For the common case (Spanned<T> with a single "value" field), this is just one field
        for field in value_fields {
            wip = wip
                .begin_field(field.name)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        // Set the span metadata field to default
        // Most format parsers don't track source spans, so we use a default (unknown) span
        wip = wip
            .begin_field(span_field.name)
            .map_err(DeserializeError::reflect)?;
        wip = wip.set_default().map_err(DeserializeError::reflect)?;
        wip = wip.end().map_err(DeserializeError::reflect)?;

        Ok(wip)
    }

    fn deserialize_tuple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Get field count for tuple hints (needed for non-self-describing formats like postcard)
        let field_count = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def.fields.len(),
            _ => 0, // Unit type or unknown - will be handled below
        };

        // Hint to non-self-describing parsers how many fields to expect
        // Tuples are like positional structs, so we use hint_struct_fields
        self.parser.hint_struct_fields(field_count);

        let event = self.expect_peek("value")?;

        // Special case: newtype structs (single-field tuple structs) can accept scalar values
        // directly without requiring a sequence wrapper. This enables patterns like:
        //   struct Wrapper(i32);
        //   toml: "value = 42"  ->  Wrapper(42)
        if field_count == 1 && matches!(event, ParseEvent::Scalar(_)) {
            // Unwrap into field "0" and deserialize the scalar
            wip = wip.begin_field("0").map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (for XML elements or
        // non-self-describing formats like postcard where tuples are positional structs)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            // Ambiguous containers (XML elements) always use struct mode
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            // For non-self-describing formats, StructStart(Object) is valid for tuples
            // because hint_struct_fields was called and tuples are positional structs
            ParseEvent::StructStart(_) if !self.parser.is_self_describing() => true,
            // For self-describing formats like TOML/JSON, objects with numeric keys
            // (e.g., { "0" = true, "1" = 1 }) are valid tuple representations
            ParseEvent::StructStart(ContainerKind::Object) => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                    span: self.last_span,
                    path: None,
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start for tuple",
                    got: format!("{event:?}"),
                    span: self.last_span,
                    path: None,
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
            wip = wip
                .begin_field(&field_name)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            index += 1;
        }

        Ok(wip)
    }

    /// Helper to find a tag value from field evidence.
    fn find_tag_value<'a>(
        evidence: &'a [crate::FieldEvidence<'input>],
        tag_key: &str,
    ) -> Option<&'a str> {
        evidence
            .iter()
            .find(|e| e.name == tag_key)
            .and_then(|e| match &e.scalar_value {
                Some(ScalarValue::Str(s) | ScalarValue::StringlyTyped(s)) => Some(s.as_ref()),
                _ => None,
            })
    }

    /// Helper to collect all evidence from a probe stream.
    fn collect_evidence<S: crate::ProbeStream<'input, Error = P::Error>>(
        mut probe: S,
    ) -> Result<alloc::vec::Vec<crate::FieldEvidence<'input>>, P::Error> {
        let mut evidence = alloc::vec::Vec::new();
        while let Some(ev) = probe.next()? {
            evidence.push(ev);
        }
        Ok(evidence)
    }

    fn deserialize_list(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (XML elements)
        // In struct mode, we skip FieldKey events and treat values as sequence items
        // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                    span: self.last_span,
                    path: None,
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start",
                    got: format!("{event:?}"),
                    span: self.last_span,
                    path: None,
                });
            }
        };

        // Initialize the list
        wip = wip.begin_list().map_err(DeserializeError::reflect)?;

        loop {
            let event = self.expect_peek("value")?;

            // Check for end of container
            if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                self.expect_event("value")?;
                break;
            }

            // In struct mode, skip FieldKey events (they're just labels for items)
            if struct_mode && matches!(event, ParseEvent::FieldKey(_)) {
                self.expect_event("value")?;
                continue;
            }

            wip = wip.begin_list_item().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }

    fn deserialize_array(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Get the fixed array length from the type definition
        let array_len = match &wip.shape().def {
            Def::Array(array_def) => array_def.n,
            _ => {
                return Err(DeserializeError::Unsupported(
                    "deserialize_array called on non-array type".into(),
                ));
            }
        };

        // Hint to non-self-describing parsers that a fixed-size array is expected
        // (unlike hint_sequence, this doesn't read a length prefix)
        self.parser.hint_array(array_len);

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (XML elements)
        // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                    span: self.last_span,
                    path: None,
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start for array",
                    got: format!("{event:?}"),
                    span: self.last_span,
                    path: None,
                });
            }
        };

        // Transition to Array tracker state. This is important for empty arrays
        // like [u8; 0] which have no elements to initialize but still need
        // their tracker state set correctly for require_full_initialization to pass.
        wip = wip.begin_array().map_err(DeserializeError::reflect)?;

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

            wip = wip
                .begin_nth_field(index)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            index += 1;
        }

        Ok(wip)
    }

    fn deserialize_set(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (XML elements)
        // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                    span: self.last_span,
                    path: None,
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start for set",
                    got: format!("{event:?}"),
                    span: self.last_span,
                    path: None,
                });
            }
        };

        // Initialize the set
        wip = wip.begin_set().map_err(DeserializeError::reflect)?;

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

            wip = wip.begin_set_item().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }

    fn deserialize_map(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // For non-self-describing formats, hint that a map is expected
        self.parser.hint_map();

        let event = self.expect_event("value")?;

        // Initialize the map
        wip = wip.begin_map().map_err(DeserializeError::reflect)?;

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
                            wip = wip.begin_key().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_map_key(wip, key.name)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;

                            // Begin value
                            wip = wip.begin_value().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        other => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "field key or struct end for map",
                                got: format!("{other:?}"),
                                span: self.last_span,
                                path: None,
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
                            wip = wip.begin_key().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;

                            // Deserialize value
                            wip = wip.begin_value().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        other => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "ordered field or sequence end for map",
                                got: format!("{other:?}"),
                                span: self.last_span,
                                path: None,
                            });
                        }
                    }
                }
            }
            other => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "struct start or sequence start for map",
                    got: format!("{other:?}"),
                    span: self.last_span,
                    path: None,
                });
            }
        }

        Ok(wip)
    }

    fn deserialize_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
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
                            if key.name == "_arg" {
                                let value_event = self.expect_event("argument value")?;
                                if let ParseEvent::Scalar(scalar) = value_event {
                                    found_scalar = Some(scalar);
                                } else {
                                    // Skip non-scalar argument
                                    self.parser.skip_value().map_err(DeserializeError::Parser)?;
                                }
                            } else {
                                // Skip other fields (_node_name, _arguments, properties, etc.)
                                self.parser.skip_value().map_err(DeserializeError::Parser)?;
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
                    Err(DeserializeError::TypeMismatch {
                        expected: "scalar value or node with argument",
                        got: "node without argument".to_string(),
                        span: self.last_span,
                        path: None,
                    })
                }
            }
            other => Err(DeserializeError::TypeMismatch {
                expected: "scalar value",
                got: format!("{other:?}"),
                span: self.last_span,
                path: None,
            }),
        }
    }

    /// Deserialize a map key from a string.
    ///
    /// Format parsers typically emit string keys, but the target map might have non-string key types
    /// (e.g., integers, enums). This function parses the string key into the appropriate type:
    /// - String types: set directly
    /// - Enum unit variants: use select_variant_named
    /// - Integer types: parse the string as a number
    /// - Transparent newtypes: descend into the inner type
    fn deserialize_map_key(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        key: Cow<'input, str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // For transparent types (like UserId(String)), we need to use begin_inner
        // to set the inner value. But NOT for pointer types like &str or Cow<str>
        // which are handled directly.
        let is_pointer = matches!(shape.def, Def::Pointer(_));
        if shape.inner.is_some() && !is_pointer {
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_map_key(wip, key)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check if target is an enum - use select_variant_named for unit variants
        if let Type::User(UserType::Enum(_)) = &shape.ty {
            wip = wip
                .select_variant_named(&key)
                .map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check if target is a numeric type - parse the string key as a number
        if let Type::Primitive(PrimitiveType::Numeric(num_ty)) = &shape.ty {
            match num_ty {
                NumericType::Integer { signed } => {
                    if *signed {
                        let n: i64 = key.parse().map_err(|_| DeserializeError::TypeMismatch {
                            expected: "valid integer for map key",
                            got: format!("string '{}'", key),
                            span: self.last_span,
                            path: None,
                        })?;
                        // Use set for each size - the Partial handles type conversion
                        wip = wip.set(n).map_err(DeserializeError::reflect)?;
                    } else {
                        let n: u64 = key.parse().map_err(|_| DeserializeError::TypeMismatch {
                            expected: "valid unsigned integer for map key",
                            got: format!("string '{}'", key),
                            span: self.last_span,
                            path: None,
                        })?;
                        wip = wip.set(n).map_err(DeserializeError::reflect)?;
                    }
                    return Ok(wip);
                }
                NumericType::Float => {
                    let n: f64 = key.parse().map_err(|_| DeserializeError::TypeMismatch {
                        expected: "valid float for map key",
                        got: format!("string '{}'", key),
                        span: self.last_span,
                        path: None,
                    })?;
                    wip = wip.set(n).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                }
            }
        }

        // Default: treat as string
        wip = self.set_string_value(wip, key)?;
        Ok(wip)
    }
}
