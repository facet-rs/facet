//! XML deserialization using quick-xml streaming events.
//!
//! This deserializer uses quick-xml's event-based API, processing events
//! on-demand and supporting rewind via event indices for flatten deserialization.

use facet_core::{
    Def, Facet, Field, NumericType, PrimitiveType, ShapeLayout, StructKind, Type, UserType,
};
use facet_reflect::{Partial, is_spanned_shape};
use miette::SourceSpan;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::error::{XmlError, XmlErrorKind};

pub(crate) type Result<T> = std::result::Result<T, XmlError>;

// ============================================================================
// Public API
// ============================================================================

/// Deserialize an XML string into a value of type `T`.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml as xml;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     id: u32,
///     #[facet(xml::element)]
///     name: String,
/// }
///
/// let xml_str = r#"<Person id="42"><name>Alice</name></Person>"#;
/// let person: Person = facet_xml::from_str(xml_str).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.id, 42);
/// ```
pub fn from_str<'input, 'facet, T>(xml: &'input str) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!(
        "from_str: parsing XML for type {}",
        core::any::type_name::<T>()
    );

    let mut deserializer = XmlDeserializer::new(xml)?;
    let partial = Partial::alloc::<T>()?;

    let partial = deserializer.deserialize_document(partial)?;

    let result = partial
        .build()
        .map_err(|e| XmlError::new(XmlErrorKind::Reflect(e)).with_source(xml))?
        .materialize()
        .map_err(|e| XmlError::new(XmlErrorKind::Reflect(e)).with_source(xml))?;

    Ok(result)
}

/// Deserialize an XML byte slice into a value of type `T`.
///
/// This is a convenience wrapper around [`from_str`] that first validates
/// that the input is valid UTF-8.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml as xml;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     id: u32,
///     #[facet(xml::element)]
///     name: String,
/// }
///
/// let xml_bytes = b"<Person id=\"42\"><name>Alice</name></Person>";
/// let person: Person = facet_xml::from_slice(xml_bytes).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.id, 42);
/// ```
pub fn from_slice<'input, 'facet, T>(xml: &'input [u8]) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    let xml_str = std::str::from_utf8(xml)
        .map_err(|e| XmlError::new(XmlErrorKind::InvalidUtf8(e.to_string())))?;
    from_str(xml_str)
}

// ============================================================================
// Extension trait for XML-specific field attributes
// ============================================================================

/// Extension trait for Field to check XML-specific attributes.
pub(crate) trait XmlFieldExt {
    /// Returns true if this field is an element field.
    fn is_xml_element(&self) -> bool;
    /// Returns true if this field is an elements (list) field.
    fn is_xml_elements(&self) -> bool;
    /// Returns true if this field is an attribute field.
    fn is_xml_attribute(&self) -> bool;
    /// Returns true if this field is a text field.
    fn is_xml_text(&self) -> bool;
    /// Returns true if this field stores the element name.
    #[allow(dead_code)]
    fn is_xml_element_name(&self) -> bool;
}

impl XmlFieldExt for Field {
    fn is_xml_element(&self) -> bool {
        self.is_child() || self.has_attr(Some("xml"), "element")
    }

    fn is_xml_elements(&self) -> bool {
        self.has_attr(Some("xml"), "elements")
    }

    fn is_xml_attribute(&self) -> bool {
        self.has_attr(Some("xml"), "attribute")
    }

    fn is_xml_text(&self) -> bool {
        self.has_attr(Some("xml"), "text")
    }

    fn is_xml_element_name(&self) -> bool {
        self.has_attr(Some("xml"), "element_name")
    }
}

// ============================================================================
// Event wrapper with owned strings
// ============================================================================

/// An XML event with owned string data and span information.
#[derive(Debug, Clone)]
enum OwnedEvent {
    /// Start of an element with tag name and attributes
    Start {
        name: String,
        attributes: Vec<(String, String)>,
    },
    /// End of an element
    End { name: String },
    /// Empty element (self-closing)
    Empty {
        name: String,
        attributes: Vec<(String, String)>,
    },
    /// Text content
    Text { content: String },
    /// CDATA content
    CData { content: String },
    /// End of file
    Eof,
}

#[derive(Debug, Clone)]
struct SpannedEvent {
    event: OwnedEvent,
    /// Byte offset in the original input where this event starts.
    offset: usize,
    /// Length of the event in bytes.
    len: usize,
}

impl SpannedEvent {
    fn span(&self) -> SourceSpan {
        SourceSpan::from((self.offset, self.len))
    }
}

// ============================================================================
// Event Collector
// ============================================================================

/// Collects all events from the parser upfront.
struct EventCollector<'input> {
    reader: Reader<&'input [u8]>,
    input: &'input str,
}

impl<'input> EventCollector<'input> {
    fn new(input: &'input str) -> Self {
        let mut reader = Reader::from_str(input);
        reader.config_mut().trim_text(true);
        Self { reader, input }
    }

    fn collect_all(mut self) -> Result<Vec<SpannedEvent>> {
        let mut events = Vec::new();
        let mut buf = Vec::new();

        loop {
            let offset = self.reader.buffer_position() as usize;
            let event = self.reader.read_event_into(&mut buf).map_err(|e| {
                XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
            })?;

            let (owned, len) = match event {
                Event::Start(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let attributes = self.collect_attributes(&e)?;
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::Start { name, attributes }, len)
                }
                Event::End(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::End { name }, len)
                }
                Event::Empty(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let attributes = self.collect_attributes(&e)?;
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::Empty { name, attributes }, len)
                }
                Event::Text(e) => {
                    let content = e.unescape().map_err(|e| {
                        XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
                    })?;
                    if content.trim().is_empty() {
                        buf.clear();
                        continue; // Skip whitespace-only text
                    }
                    let len = self.reader.buffer_position() as usize - offset;
                    (
                        OwnedEvent::Text {
                            content: content.into_owned(),
                        },
                        len,
                    )
                }
                Event::CData(e) => {
                    let content = String::from_utf8_lossy(&e).into_owned();
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::CData { content }, len)
                }
                Event::Eof => {
                    events.push(SpannedEvent {
                        event: OwnedEvent::Eof,
                        offset,
                        len: 0,
                    });
                    break;
                }
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {
                    // Skip comments, declarations, processing instructions, doctypes
                    buf.clear();
                    continue;
                }
            };

            log::trace!("XML event: {owned:?} at offset {offset}");
            events.push(SpannedEvent {
                event: owned,
                offset,
                len,
            });
            buf.clear();
        }

        Ok(events)
    }

    fn collect_attributes(&self, e: &BytesStart<'_>) -> Result<Vec<(String, String)>> {
        let mut attrs = Vec::new();
        for attr in e.attributes() {
            let attr = attr.map_err(|e| {
                XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
            })?;
            let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
            let value = attr
                .unescape_value()
                .map_err(|e| {
                    XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
                })?
                .into_owned();
            attrs.push((key, value));
        }
        Ok(attrs)
    }
}

// ============================================================================
// Deserializer
// ============================================================================

/// XML deserializer that processes events from a collected event stream.
struct XmlDeserializer<'input> {
    input: &'input str,
    events: Vec<SpannedEvent>,
    pos: usize,
}

impl<'input> XmlDeserializer<'input> {
    /// Create a new deserializer by parsing the input and collecting all events.
    fn new(input: &'input str) -> Result<Self> {
        let collector = EventCollector::new(input);
        let events = collector.collect_all()?;

        Ok(Self {
            input,
            events,
            pos: 0,
        })
    }

    /// Create an error with source code attached for diagnostics.
    fn err(&self, kind: impl Into<XmlErrorKind>) -> XmlError {
        XmlError::new(kind).with_source(self.input.to_string())
    }

    /// Create an error with source code and span attached for diagnostics.
    fn err_at(&self, kind: impl Into<XmlErrorKind>, span: impl Into<SourceSpan>) -> XmlError {
        XmlError::new(kind)
            .with_source(self.input.to_string())
            .with_span(span)
    }

    /// Consume and return the current event (cloned to avoid borrow issues).
    fn next(&mut self) -> Option<SpannedEvent> {
        if self.pos < self.events.len() {
            let event = self.events[self.pos].clone();
            self.pos += 1;
            Some(event)
        } else {
            None
        }
    }

    /// Save current position for potential rewind.
    #[allow(dead_code)]
    fn save_position(&self) -> usize {
        self.pos
    }

    /// Restore to a previously saved position.
    #[allow(dead_code)]
    fn restore_position(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Deserialize the document starting from the root element.
    fn deserialize_document<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>> {
        // Expect a start or empty element
        let Some(event) = self.next() else {
            return Err(self.err(XmlErrorKind::UnexpectedEof));
        };

        let span = event.span();

        match event.event {
            OwnedEvent::Start { name, attributes } => {
                self.deserialize_element(partial, &name, &attributes, span, false)
            }
            OwnedEvent::Empty { name, attributes } => {
                self.deserialize_element(partial, &name, &attributes, span, true)
            }
            other => Err(self.err(XmlErrorKind::UnexpectedEvent(format!(
                "expected start element, got {other:?}"
            )))),
        }
    }

    /// Deserialize an element into a partial value.
    fn deserialize_element<'facet>(
        &mut self,
        partial: Partial<'facet>,
        element_name: &str,
        attributes: &[(String, String)],
        span: SourceSpan,
        is_empty: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let shape = partial.shape();

        log::trace!(
            "deserialize_element: {} into shape {:?}",
            element_name,
            shape.ty
        );

        // Check Def first for scalars (String, etc.)
        if matches!(&shape.def, Def::Scalar) {
            // For scalar types, we expect text content
            if is_empty {
                // Empty element for a string means empty string
                if shape.is_type::<String>() {
                    partial = partial.set(String::new())?;
                    return Ok(partial);
                }
                return Err(self.err_at(
                    XmlErrorKind::InvalidValueForShape(
                        "expected text content for scalar type".into(),
                    ),
                    span,
                ));
            }

            // Get text content
            let text = self.read_text_until_end(element_name)?;
            partial = self.set_scalar_value(partial, &text)?;
            return Ok(partial);
        }

        // Handle transparent types (newtype wrappers)
        if shape.inner.is_some() {
            partial = partial.begin_inner()?;
            partial =
                self.deserialize_element(partial, element_name, attributes, span, is_empty)?;
            partial = partial.end()?;
            return Ok(partial);
        }

        // Handle fixed arrays
        if let Def::Array(arr_def) = &shape.def {
            if is_empty {
                return Err(self.err_at(
                    XmlErrorKind::InvalidValueForShape("empty element for array".into()),
                    span,
                ));
            }
            let array_len = arr_def.n;
            return self.deserialize_array_content(partial, array_len, element_name);
        }

        // Handle sets
        if matches!(&shape.def, Def::Set(_)) {
            if is_empty {
                partial = partial.begin_set()?;
                // Empty set - nothing to do
                return Ok(partial);
            }
            return self.deserialize_set_content(partial, element_name);
        }

        // Handle maps
        if matches!(&shape.def, Def::Map(_)) {
            if is_empty {
                partial = partial.begin_map()?;
                // Empty map - nothing to do
                return Ok(partial);
            }
            return self.deserialize_map_content(partial, element_name);
        }

        // Handle different shapes
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                // Get fields
                let fields = struct_def.fields;
                let deny_unknown = shape.has_deny_unknown_fields_attr();

                match struct_def.kind {
                    StructKind::Unit => {
                        // Unit struct - nothing to deserialize, just skip content
                        if !is_empty {
                            self.skip_element(element_name)?;
                        }
                        return Ok(partial);
                    }
                    StructKind::Tuple | StructKind::TupleStruct => {
                        // Tuple struct - deserialize fields by position
                        if is_empty {
                            // Set defaults for all fields
                            partial = self.set_defaults_for_unset_fields(partial, fields)?;
                            return Ok(partial);
                        }

                        // Deserialize tuple fields from child elements
                        partial = self.deserialize_tuple_content(partial, fields, element_name)?;

                        // Set defaults for any unset fields
                        partial = self.set_defaults_for_unset_fields(partial, fields)?;
                        return Ok(partial);
                    }
                    StructKind::Struct => {
                        // Normal named struct - fall through to standard handling
                    }
                }

                // First, deserialize attributes
                partial =
                    self.deserialize_attributes(partial, fields, attributes, deny_unknown, span)?;

                // If empty element, we're done with content
                if is_empty {
                    // Set defaults for missing fields
                    partial = self.set_defaults_for_unset_fields(partial, fields)?;
                    return Ok(partial);
                }

                // Deserialize child elements and text content
                partial =
                    self.deserialize_element_content(partial, fields, element_name, deny_unknown)?;

                // Set defaults for any unset fields
                partial = self.set_defaults_for_unset_fields(partial, fields)?;

                Ok(partial)
            }
            Type::User(UserType::Enum(enum_def)) => {
                // Enum deserialization - expect wrapper element with variant as child
                // Format: <MyEnum><VariantName>...</VariantName></MyEnum>
                if is_empty {
                    return Err(self.err_at(
                        XmlErrorKind::InvalidValueForShape("empty element for enum".into()),
                        span,
                    ));
                }

                // Read the variant element
                let variant_event = loop {
                    let Some(event) = self.next() else {
                        return Err(self.err(XmlErrorKind::UnexpectedEof));
                    };

                    match &event.event {
                        OwnedEvent::Text { content } if content.trim().is_empty() => {
                            // Skip whitespace
                            continue;
                        }
                        OwnedEvent::Start { .. } | OwnedEvent::Empty { .. } => {
                            break event;
                        }
                        _ => {
                            return Err(self.err_at(
                                XmlErrorKind::UnexpectedEvent(format!(
                                    "expected variant element, got {:?}",
                                    event.event
                                )),
                                event.span(),
                            ));
                        }
                    }
                };

                let variant_span = variant_event.span();
                let (variant_name, variant_attrs, variant_is_empty) = match &variant_event.event {
                    OwnedEvent::Start { name, attributes } => {
                        (name.clone(), attributes.clone(), false)
                    }
                    OwnedEvent::Empty { name, attributes } => {
                        (name.clone(), attributes.clone(), true)
                    }
                    _ => unreachable!(),
                };

                // Find the variant by name
                let variant = enum_def
                    .variants
                    .iter()
                    .find(|v| v.name == variant_name)
                    .ok_or_else(|| {
                        self.err_at(
                            XmlErrorKind::NoMatchingElement(variant_name.clone()),
                            variant_span,
                        )
                    })?;

                // Select the variant
                partial = partial.select_variant_named(&variant_name)?;

                let variant_fields = variant.data.fields;

                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant - nothing to deserialize
                        if !variant_is_empty {
                            self.skip_element(&variant_name)?;
                        }
                    }
                    StructKind::Tuple | StructKind::TupleStruct => {
                        // Tuple variant - deserialize fields by position
                        if !variant_is_empty {
                            partial = self.deserialize_tuple_content(
                                partial,
                                variant_fields,
                                &variant_name,
                            )?;
                        }
                        partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
                    }
                    StructKind::Struct => {
                        // Struct variant - deserialize as struct
                        partial = self.deserialize_attributes(
                            partial,
                            variant_fields,
                            &variant_attrs,
                            false,
                            variant_span,
                        )?;
                        if !variant_is_empty {
                            partial = self.deserialize_element_content(
                                partial,
                                variant_fields,
                                &variant_name,
                                false,
                            )?;
                        }
                        partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
                    }
                }

                // Skip to the end of the wrapper element
                loop {
                    let Some(event) = self.next() else {
                        return Err(self.err(XmlErrorKind::UnexpectedEof));
                    };

                    match &event.event {
                        OwnedEvent::End { name } if name == element_name => {
                            break;
                        }
                        OwnedEvent::Text { content } if content.trim().is_empty() => {
                            // Skip whitespace
                            continue;
                        }
                        _ => {
                            return Err(self.err_at(
                                XmlErrorKind::UnexpectedEvent(format!(
                                    "expected end of enum wrapper, got {:?}",
                                    event.event
                                )),
                                event.span(),
                            ));
                        }
                    }
                }

                Ok(partial)
            }
            _ => Err(self.err_at(
                XmlErrorKind::UnsupportedShape(format!("cannot deserialize into {:?}", shape.ty)),
                span,
            )),
        }
    }

    /// Deserialize XML attributes into struct fields.
    fn deserialize_attributes<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        attributes: &[(String, String)],
        deny_unknown: bool,
        element_span: SourceSpan,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;

        for (attr_name, attr_value) in attributes {
            // Find the field that matches this attribute
            let field_match = fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.is_xml_attribute() && f.name == attr_name);

            if let Some((idx, field)) = field_match {
                log::trace!(
                    "deserialize attribute {} into field {}",
                    attr_name,
                    field.name
                );

                partial = partial.begin_nth_field(idx)?;

                // Handle Option<T>
                if matches!(&partial.shape().def, Def::Option(_)) {
                    partial = partial.begin_some()?;
                }

                // Handle Spanned<T>
                if is_spanned_shape(partial.shape()) {
                    partial = partial.begin_field("value")?;
                }

                // Deserialize the value
                partial = self.set_scalar_value(partial, attr_value)?;

                // End Spanned<T> if needed
                if is_spanned_shape((field.shape)()) {
                    partial = partial.end()?; // end value field
                }

                partial = partial.end()?; // end field
            } else if deny_unknown {
                // Unknown attribute when deny_unknown_fields is set
                let expected: Vec<&'static str> = fields
                    .iter()
                    .filter(|f| f.is_xml_attribute())
                    .map(|f| f.name)
                    .collect();
                return Err(self.err_at(
                    XmlErrorKind::UnknownAttribute {
                        attribute: attr_name.clone(),
                        expected,
                    },
                    element_span,
                ));
            }
            // Otherwise ignore unknown attributes
        }

        Ok(partial)
    }

    /// Deserialize child elements and text content.
    fn deserialize_element_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        parent_element_name: &str,
        deny_unknown: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let mut text_content = String::new();

        // Track which element fields are lists (for xml::elements)
        let mut elements_field_started: Option<usize> = None;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    // End any open elements list
                    if elements_field_started.is_some() {
                        partial = partial.end()?; // end list
                        partial = partial.end()?; // end field
                    }

                    // Handle accumulated text content
                    if !text_content.is_empty() {
                        partial = self.set_text_field(partial, fields, &text_content)?;
                    }

                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    partial = self.deserialize_child_element(
                        partial,
                        fields,
                        &name,
                        &attributes,
                        span,
                        false,
                        &mut elements_field_started,
                        deny_unknown,
                    )?;
                }
                OwnedEvent::Empty { name, attributes } => {
                    partial = self.deserialize_child_element(
                        partial,
                        fields,
                        &name,
                        &attributes,
                        span,
                        true,
                        &mut elements_field_started,
                        deny_unknown,
                    )?;
                }
                OwnedEvent::Text { content } | OwnedEvent::CData { content } => {
                    text_content.push_str(&content);
                }
                OwnedEvent::End { name } => {
                    // End tag for a different element - this shouldn't happen
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize tuple struct content - fields are numbered elements like `<_0>`, `<_1>`, etc.
    fn deserialize_tuple_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        parent_element_name: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let mut field_idx = 0;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    if field_idx >= fields.len() {
                        return Err(self.err_at(
                            XmlErrorKind::UnexpectedEvent(format!(
                                "too many elements for tuple struct (expected {})",
                                fields.len()
                            )),
                            span,
                        ));
                    }

                    partial = partial.begin_nth_field(field_idx)?;

                    // Handle Option<T>
                    if matches!(&partial.shape().def, Def::Option(_)) {
                        partial = partial.begin_some()?;
                    }

                    partial = self.deserialize_element(partial, &name, &attributes, span, false)?;
                    partial = partial.end()?; // end field
                    field_idx += 1;
                }
                OwnedEvent::Empty { name, attributes } => {
                    if field_idx >= fields.len() {
                        return Err(self.err_at(
                            XmlErrorKind::UnexpectedEvent(format!(
                                "too many elements for tuple struct (expected {})",
                                fields.len()
                            )),
                            span,
                        ));
                    }

                    partial = partial.begin_nth_field(field_idx)?;

                    // Handle Option<T>
                    if matches!(&partial.shape().def, Def::Option(_)) {
                        partial = partial.begin_some()?;
                    }

                    partial = self.deserialize_element(partial, &name, &attributes, span, true)?;
                    partial = partial.end()?; // end field
                    field_idx += 1;
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore text content in tuple structs
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize fixed array content - expects sequential child elements
    fn deserialize_array_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        array_len: usize,
        parent_element_name: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let mut idx = 0;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    if idx < array_len {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(format!(
                                "not enough elements for array (got {idx}, expected {array_len})"
                            )),
                            span,
                        ));
                    }
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    if idx >= array_len {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(format!(
                                "too many elements for array (expected {array_len})"
                            )),
                            span,
                        ));
                    }
                    partial = partial.begin_nth_field(idx)?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, false)?;
                    partial = partial.end()?;
                    idx += 1;
                }
                OwnedEvent::Empty { name, attributes } => {
                    if idx >= array_len {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(format!(
                                "too many elements for array (expected {array_len})"
                            )),
                            span,
                        ));
                    }
                    partial = partial.begin_nth_field(idx)?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, true)?;
                    partial = partial.end()?;
                    idx += 1;
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore whitespace between elements
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize set content - each child element is a set item
    fn deserialize_set_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        parent_element_name: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        partial = partial.begin_set()?;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    partial = partial.begin_set_item()?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, false)?;
                    partial = partial.end()?; // end set item
                }
                OwnedEvent::Empty { name, attributes } => {
                    partial = partial.begin_set_item()?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, true)?;
                    partial = partial.end()?; // end set item
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore whitespace between elements
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize map content - expects <entry key="...">value</entry> or similar structure
    fn deserialize_map_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        parent_element_name: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        partial = partial.begin_map()?;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    // Map entry: element name is the key, content is the value
                    partial = partial.begin_key()?;
                    partial = partial.set(name.clone())?;
                    partial = partial.end()?; // end key

                    partial = partial.begin_value()?;
                    // If there's a key attribute, use that as context; otherwise read content
                    partial = self.deserialize_map_entry_value(partial, &name, &attributes)?;
                    partial = partial.end()?; // end value
                }
                OwnedEvent::Empty { name, .. } => {
                    // Empty element as map entry - key is element name, value is default/empty
                    partial = partial.begin_key()?;
                    partial = partial.set(name.clone())?;
                    partial = partial.end()?; // end key

                    partial = partial.begin_value()?;
                    // Set default value for the map value type
                    let value_shape = partial.shape();
                    if value_shape.is_type::<String>() {
                        partial = partial.set(String::new())?;
                    } else if value_shape.is_type::<bool>() {
                        partial = partial.set(true)?; // presence implies true
                    } else {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(
                                "empty element for non-string/bool map value".into(),
                            ),
                            span,
                        ));
                    }
                    partial = partial.end()?; // end value
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore whitespace between elements
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize the value portion of a map entry
    fn deserialize_map_entry_value<'facet>(
        &mut self,
        partial: Partial<'facet>,
        element_name: &str,
        _attributes: &[(String, String)],
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let shape = partial.shape();

        // For scalar values, read text content
        if matches!(&shape.def, Def::Scalar) {
            let text = self.read_text_until_end(element_name)?;
            partial = self.set_scalar_value(partial, &text)?;
            return Ok(partial);
        }

        // For complex values, read the element content
        // This is a simplified version - complex map values would need more work
        let text = self.read_text_until_end(element_name)?;
        if shape.is_type::<String>() {
            partial = partial.set(text)?;
        } else {
            partial = self.set_scalar_value(partial, &text)?;
        }

        Ok(partial)
    }

    /// Deserialize a child element into the appropriate field.
    fn deserialize_child_element<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        element_name: &str,
        attributes: &[(String, String)],
        span: SourceSpan,
        is_empty: bool,
        elements_field_started: &mut Option<usize>,
        deny_unknown: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;

        // First try to find a direct element field match
        if let Some((idx, field)) = fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.is_xml_element() && f.name == element_name)
        {
            log::trace!("matched element {} to field {}", element_name, field.name);

            // End any open elements list from a different field
            if let Some(prev_idx) = *elements_field_started {
                if prev_idx != idx {
                    partial = partial.end()?; // end list
                    partial = partial.end()?; // end field
                    *elements_field_started = None;
                }
            }

            partial = partial.begin_nth_field(idx)?;

            // Handle Option<T>
            if matches!(&partial.shape().def, Def::Option(_)) {
                partial = partial.begin_some()?;
            }

            // Deserialize the element content
            partial =
                self.deserialize_element(partial, element_name, attributes, span, is_empty)?;

            partial = partial.end()?; // end field
            return Ok(partial);
        }

        // Try to find an elements (list) field that accepts this element
        if let Some((idx, _field)) = fields.iter().enumerate().find(|(_, f)| f.is_xml_elements()) {
            // If we haven't started this list yet, begin it
            if elements_field_started.is_none() || *elements_field_started != Some(idx) {
                // End previous list if any
                if elements_field_started.is_some() {
                    partial = partial.end()?; // end list
                    partial = partial.end()?; // end field
                }

                partial = partial.begin_nth_field(idx)?;
                partial = partial.begin_list()?;
                *elements_field_started = Some(idx);
            }

            // Add item to list
            partial = partial.begin_list_item()?;
            partial =
                self.deserialize_element(partial, element_name, attributes, span, is_empty)?;
            partial = partial.end()?; // end list item

            return Ok(partial);
        }

        // No matching field found
        if deny_unknown {
            // Unknown element when deny_unknown_fields is set
            let expected: Vec<&'static str> = fields
                .iter()
                .filter(|f| f.is_xml_element() || f.is_xml_elements())
                .map(|f| f.name)
                .collect();
            return Err(self.err_at(
                XmlErrorKind::UnknownField {
                    field: element_name.to_string(),
                    expected,
                },
                span,
            ));
        }

        // Skip this element
        log::trace!("skipping unknown element: {element_name}");
        if !is_empty {
            self.skip_element(element_name)?;
        }
        Ok(partial)
    }

    /// Set the text content field.
    fn set_text_field<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        text: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;

        // Find the text field
        if let Some((idx, _field)) = fields.iter().enumerate().find(|(_, f)| f.is_xml_text()) {
            partial = partial.begin_nth_field(idx)?;

            // Handle Option<T>
            if matches!(&partial.shape().def, Def::Option(_)) {
                partial = partial.begin_some()?;
            }

            partial = partial.set(text.to_string())?;
            partial = partial.end()?;
        }
        // If no text field, ignore the text content

        Ok(partial)
    }

    /// Read text content until the end tag.
    fn read_text_until_end(&mut self, element_name: &str) -> Result<String> {
        let mut text = String::new();

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            match event.event {
                OwnedEvent::End { ref name } if name == element_name => {
                    break;
                }
                OwnedEvent::Text { content } | OwnedEvent::CData { content } => {
                    text.push_str(&content);
                }
                other => {
                    return Err(self.err(XmlErrorKind::UnexpectedEvent(format!(
                        "expected text or end tag, got {other:?}"
                    ))));
                }
            }
        }

        Ok(text)
    }

    /// Skip an element and all its content.
    fn skip_element(&mut self, element_name: &str) -> Result<()> {
        let mut depth = 1;

        while depth > 0 {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            match &event.event {
                OwnedEvent::Start { .. } => depth += 1,
                OwnedEvent::End { name } if name == element_name && depth == 1 => {
                    depth -= 1;
                }
                OwnedEvent::End { .. } => depth -= 1,
                OwnedEvent::Empty { .. } => {}
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {}
                OwnedEvent::Eof => return Err(self.err(XmlErrorKind::UnexpectedEof)),
            }
        }

        Ok(())
    }

    /// Set defaults for any unset fields.
    fn set_defaults_for_unset_fields<'facet>(
        &self,
        partial: Partial<'facet>,
        fields: &[Field],
    ) -> Result<Partial<'facet>> {
        use facet_core::Characteristic;
        let mut partial = partial;

        for (idx, field) in fields.iter().enumerate() {
            if partial.is_field_set(idx)? {
                continue;
            }

            let field_has_default_flag = field.has_default();
            let field_has_default_fn = field.default_fn().is_some();
            let field_type_has_default = (field.shape)().is(Characteristic::Default);
            let should_skip = field.should_skip_deserializing();

            if field_has_default_fn
                || field_has_default_flag
                || field_type_has_default
                || should_skip
            {
                log::trace!("setting default for unset field: {}", field.name);
                partial = partial.set_nth_field_to_default(idx)?;
            }
        }

        Ok(partial)
    }

    /// Set a scalar value on the partial based on its type.
    fn set_scalar_value<'facet>(
        &self,
        partial: Partial<'facet>,
        value: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let shape = partial.shape();

        // Handle transparent wrappers
        if shape.inner.is_some() {
            partial = partial.begin_inner()?;
            partial = self.set_scalar_value(partial, value)?;
            partial = partial.end()?;
            return Ok(partial);
        }

        // Handle usize and isize explicitly before other numeric types
        if shape.is_type::<usize>() {
            let n: usize = value.parse().map_err(|_| {
                self.err(XmlErrorKind::InvalidValueForShape(format!(
                    "cannot parse `{value}` as usize"
                )))
            })?;
            partial = partial.set(n)?;
            return Ok(partial);
        }

        if shape.is_type::<isize>() {
            let n: isize = value.parse().map_err(|_| {
                self.err(XmlErrorKind::InvalidValueForShape(format!(
                    "cannot parse `{value}` as isize"
                )))
            })?;
            partial = partial.set(n)?;
            return Ok(partial);
        }

        // Try numeric types
        if let Type::Primitive(PrimitiveType::Numeric(numeric_type)) = shape.ty {
            let size = match shape.layout {
                ShapeLayout::Sized(layout) => layout.size(),
                ShapeLayout::Unsized => {
                    return Err(self.err(XmlErrorKind::InvalidValueForShape(
                        "cannot assign to unsized type".into(),
                    )));
                }
            };

            return self.set_numeric_value(partial, value, numeric_type, size);
        }

        // Boolean
        if shape.is_type::<bool>() {
            let b = match value.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "cannot parse `{value}` as boolean"
                    ))));
                }
            };
            partial = partial.set(b)?;
            return Ok(partial);
        }

        // Char
        if shape.is_type::<char>() {
            let mut chars = value.chars();
            let c = chars.next().ok_or_else(|| {
                self.err(XmlErrorKind::InvalidValueForShape(
                    "empty string cannot be converted to char".into(),
                ))
            })?;
            if chars.next().is_some() {
                return Err(self.err(XmlErrorKind::InvalidValueForShape(
                    "string has more than one character".into(),
                )));
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
        if partial.shape().vtable.parse.is_some() {
            partial = partial
                .parse_from_str(value)
                .map_err(|e| self.err(XmlErrorKind::Reflect(e)))?;
            return Ok(partial);
        }

        // Last resort: try setting as string
        partial = partial
            .set(value.to_string())
            .map_err(|e| self.err(XmlErrorKind::Reflect(e)))?;

        Ok(partial)
    }

    /// Set a numeric value with proper type conversion.
    fn set_numeric_value<'facet>(
        &self,
        partial: Partial<'facet>,
        value: &str,
        numeric_type: NumericType,
        size: usize,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        match numeric_type {
            NumericType::Integer { signed: false } => {
                let n: u64 = value.parse().map_err(|_| {
                    self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "cannot parse `{value}` as unsigned integer"
                    )))
                })?;

                match size {
                    1 => {
                        let v = u8::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for u8"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    2 => {
                        let v = u16::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for u16"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    4 => {
                        let v = u32::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for u32"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(n)?;
                    }
                    16 => {
                        let n: u128 = value.parse().map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "cannot parse `{value}` as u128"
                            )))
                        })?;
                        partial = partial.set(n)?;
                    }
                    _ => {
                        return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "unsupported unsigned integer size: {size}"
                        ))));
                    }
                }
            }
            NumericType::Integer { signed: true } => {
                let n: i64 = value.parse().map_err(|_| {
                    self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "cannot parse `{value}` as signed integer"
                    )))
                })?;

                match size {
                    1 => {
                        let v = i8::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for i8"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    2 => {
                        let v = i16::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for i16"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    4 => {
                        let v = i32::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for i32"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(n)?;
                    }
                    16 => {
                        let n: i128 = value.parse().map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "cannot parse `{value}` as i128"
                            )))
                        })?;
                        partial = partial.set(n)?;
                    }
                    _ => {
                        return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "unsupported signed integer size: {size}"
                        ))));
                    }
                }
            }
            NumericType::Float => match size {
                4 => {
                    let v: f32 = value.parse().map_err(|_| {
                        self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "cannot parse `{value}` as f32"
                        )))
                    })?;
                    partial = partial.set(v)?;
                }
                8 => {
                    let v: f64 = value.parse().map_err(|_| {
                        self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "cannot parse `{value}` as f64"
                        )))
                    })?;
                    partial = partial.set(v)?;
                }
                _ => {
                    return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "unsupported float size: {size}"
                    ))));
                }
            },
        }

        Ok(partial)
    }
}
