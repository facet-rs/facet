//! Tree-based deserializer for DOM documents.

use std::borrow::Cow;
use std::fmt;

use facet_core::{Def, Facet, Field, Type, UserType};
use facet_reflect::{HeapValue, Partial};

use crate::{DomEvent, DomParser};

/// Error type for DOM deserialization.
#[derive(Debug)]
pub enum DomDeserializeError<E> {
    /// Parser error.
    Parser(E),
    /// Reflection error.
    Reflect(facet_reflect::ReflectError),
    /// Unexpected end of input.
    UnexpectedEof {
        /// What was expected.
        expected: &'static str,
    },
    /// Type mismatch.
    TypeMismatch {
        /// What was expected.
        expected: &'static str,
        /// What was found.
        got: String,
    },
    /// Unknown element.
    UnknownElement {
        /// The element tag name.
        tag: String,
    },
    /// Missing required attribute.
    MissingAttribute {
        /// The attribute name.
        name: &'static str,
    },
    /// Unsupported type.
    Unsupported(String),
}

impl<E: std::error::Error> fmt::Display for DomDeserializeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parser(e) => write!(f, "parser error: {e}"),
            Self::Reflect(e) => write!(f, "reflection error: {e}"),
            Self::UnexpectedEof { expected } => write!(f, "unexpected EOF, expected {expected}"),
            Self::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            Self::UnknownElement { tag } => write!(f, "unknown element: <{tag}>"),
            Self::MissingAttribute { name } => write!(f, "missing required attribute: {name}"),
            Self::Unsupported(msg) => write!(f, "unsupported: {msg}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for DomDeserializeError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parser(e) => Some(e),
            Self::Reflect(e) => Some(e),
            _ => None,
        }
    }
}

/// DOM deserializer.
///
/// This deserializer understands tree-structured documents and maps them to
/// Rust types using facet's reflection system.
///
/// The `BORROW` parameter controls whether strings can be borrowed from the input:
/// - `BORROW = true`: Allows zero-copy deserialization of `&str` and `Cow<str>`
/// - `BORROW = false`: All strings are owned, input doesn't need to outlive result
pub struct DomDeserializer<'de, const BORROW: bool, P> {
    parser: P,
    _marker: std::marker::PhantomData<&'de ()>,
}

impl<'de, P> DomDeserializer<'de, true, P>
where
    P: DomParser<'de>,
{
    /// Create a new DOM deserializer that can borrow strings from input.
    pub fn new(parser: P) -> Self {
        Self {
            parser,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'de, P> DomDeserializer<'de, false, P>
where
    P: DomParser<'de>,
{
    /// Create a new DOM deserializer that produces owned strings.
    pub fn new_owned(parser: P) -> Self {
        Self {
            parser,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'de, P> DomDeserializer<'de, true, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value of type `T`, allowing borrowed strings from input.
    pub fn deserialize<T>(&mut self) -> Result<T, DomDeserializeError<P::Error>>
    where
        T: Facet<'de>,
    {
        let wip: Partial<'de, true> =
            Partial::alloc::<T>().map_err(DomDeserializeError::Reflect)?;
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'de, true> =
            partial.build().map_err(DomDeserializeError::Reflect)?;
        heap_value
            .materialize::<T>()
            .map_err(DomDeserializeError::Reflect)
    }
}

impl<'de, P> DomDeserializer<'de, false, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value of type `T` into an owned type.
    pub fn deserialize<T>(&mut self) -> Result<T, DomDeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        // SAFETY: When BORROW=false, no references into the input are stored.
        // The Partial only contains owned data (String, Vec, etc.), so the
        // lifetime parameter is purely phantom. We transmute from 'static to 'de
        // to satisfy the type system, but the actual data has no lifetime dependency.
        #[allow(unsafe_code)]
        let wip: Partial<'de, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'de, false>>(
                Partial::alloc_owned::<T>().map_err(DomDeserializeError::Reflect)?,
            )
        };
        let partial = self.deserialize_into(wip)?;
        // SAFETY: Same reasoning - with BORROW=false, HeapValue contains only
        // owned data. The 'de lifetime is phantom and we can safely transmute
        // back to 'static since T: Facet<'static>.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'de, false>, HeapValue<'static, false>>(
                partial.build().map_err(DomDeserializeError::Reflect)?,
            )
        };
        heap_value
            .materialize::<T>()
            .map_err(DomDeserializeError::Reflect)
    }
}

impl<'de, const BORROW: bool, P> DomDeserializer<'de, BORROW, P>
where
    P: DomParser<'de>,
{
    /// Deserialize into an existing Partial.
    pub fn deserialize_into(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let shape = wip.shape();

        // Dispatch based on type
        match &shape.ty {
            Type::User(UserType::Struct(_)) => self.deserialize_struct(wip),
            Type::User(UserType::Enum(_)) => self.deserialize_enum(wip),
            _ => match &shape.def {
                Def::Scalar => self.deserialize_scalar(wip),
                Def::List(_) => self.deserialize_list(wip),
                Def::Option(_) => self.deserialize_option(wip),
                _ => Err(DomDeserializeError::Unsupported(format!(
                    "unsupported type: {:?}",
                    shape.ty
                ))),
            },
        }
    }

    /// Expect the next event to be of a specific kind.
    fn expect_event(
        &mut self,
        expected: &'static str,
    ) -> Result<DomEvent<'de>, DomDeserializeError<P::Error>> {
        self.parser
            .next_event()
            .map_err(DomDeserializeError::Parser)?
            .ok_or(DomDeserializeError::UnexpectedEof { expected })
    }

    /// Peek at the next event.
    fn peek_event(
        &mut self,
        expected: &'static str,
    ) -> Result<&DomEvent<'de>, DomDeserializeError<P::Error>> {
        self.parser
            .peek_event()
            .map_err(DomDeserializeError::Parser)?
            .ok_or(DomDeserializeError::UnexpectedEof { expected })
    }

    /// Deserialize a struct from an element.
    fn deserialize_struct(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DomDeserializeError::Unsupported(
                    "expected struct type".into(),
                ));
            }
        };

        // Expect NodeStart
        let event = self.expect_event("NodeStart")?;
        let _tag = match event {
            DomEvent::NodeStart { tag, .. } => tag,
            other => {
                return Err(DomDeserializeError::TypeMismatch {
                    expected: "NodeStart",
                    got: format!("{other:?}"),
                });
            }
        };

        // Process attributes
        loop {
            let event = self.peek_event("Attribute or ChildrenStart")?;
            match event {
                DomEvent::Attribute { .. } => {
                    let event = self.expect_event("Attribute")?;
                    if let DomEvent::Attribute { name, value, .. } = event {
                        // Find matching field
                        if let Some((idx, _field)) =
                            self.find_attribute_field(struct_def.fields, &name)
                        {
                            wip = wip
                                .begin_nth_field(idx)
                                .map_err(DomDeserializeError::Reflect)?;
                            wip = self.set_string_value(wip, value)?;
                            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                        }
                        // Unknown attributes are ignored (could add deny_unknown_fields later)
                    }
                }
                DomEvent::ChildrenStart => break,
                DomEvent::NodeEnd => {
                    // Void element (no children)
                    let _ = self.expect_event("NodeEnd")?;
                    return Ok(wip);
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "Attribute or ChildrenStart",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Consume ChildrenStart
        let _ = self.expect_event("ChildrenStart")?;

        // Find special fields
        let elements_field = struct_def
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.is_elements());
        let text_field = struct_def
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.is_text());

        // Track text content for text field
        let mut text_content = String::new();

        // If there's an elements field, we need to begin the list before processing children
        let mut elements_list_started = false;
        if let Some((elements_idx, _)) = elements_field {
            wip = wip
                .begin_nth_field(elements_idx)
                .map_err(DomDeserializeError::Reflect)?;
            wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;
            elements_list_started = true;
        }

        // Process children - unified loop handling element fields, elements collection, and text
        loop {
            let event = self.peek_event("child or ChildrenEnd")?;
            match event {
                DomEvent::ChildrenEnd => break,
                DomEvent::Text(_) => {
                    let event = self.expect_event("Text")?;
                    if let DomEvent::Text(text) = event {
                        if elements_list_started {
                            // Add text as a list item (for mixed content)
                            wip = wip
                                .begin_list_item()
                                .map_err(DomDeserializeError::Reflect)?;
                            wip = self.deserialize_text_into_enum(wip, text)?;
                            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                        } else if text_field.is_some() {
                            // Accumulate text for text field
                            text_content.push_str(&text);
                        }
                        // Otherwise ignore text
                    }
                }
                DomEvent::NodeStart { tag, .. } => {
                    // Clone tag to avoid borrow conflict with self
                    let tag = tag.clone();
                    // First, check if this matches an individual element field
                    // (only if we don't have an elements collection active)
                    if !elements_list_started {
                        if let Some((idx, _field)) =
                            self.find_element_field(struct_def.fields, &tag)
                        {
                            // Deserialize into the specific field
                            wip = wip
                                .begin_nth_field(idx)
                                .map_err(DomDeserializeError::Reflect)?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                        } else {
                            // No matching field and no elements collection - skip
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                    } else {
                        // Add to elements collection
                        wip = wip
                            .begin_list_item()
                            .map_err(DomDeserializeError::Reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                    }
                }
                DomEvent::Comment(_) => {
                    let _ = self.expect_event("Comment")?;
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "child content",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // End the elements list if it was started
        if elements_list_started {
            wip = wip.end().map_err(DomDeserializeError::Reflect)?; // end list
            wip = wip.end().map_err(DomDeserializeError::Reflect)?; // end field
        }

        // Set the text field if we accumulated text
        if let Some((text_idx, _)) = text_field {
            if !text_content.is_empty() || !elements_list_started {
                wip = wip
                    .begin_nth_field(text_idx)
                    .map_err(DomDeserializeError::Reflect)?;
                wip = self.set_string_value(wip, Cow::Owned(text_content))?;
                wip = wip.end().map_err(DomDeserializeError::Reflect)?;
            }
        }

        // Consume ChildrenEnd
        let _ = self.expect_event("ChildrenEnd")?;

        // Consume NodeEnd
        let _ = self.expect_event("NodeEnd")?;

        Ok(wip)
    }

    /// Find an attribute field by name.
    fn find_attribute_field(
        &self,
        fields: &'static [Field],
        name: &str,
    ) -> Option<(usize, &'static Field)> {
        fields.iter().enumerate().find(|(_, f)| {
            f.is_attribute()
                && (f.name.eq_ignore_ascii_case(name)
                    || f.rename.map_or(false, |r| r.eq_ignore_ascii_case(name)))
        })
    }

    /// Find an element field by tag name.
    fn find_element_field(
        &self,
        fields: &'static [Field],
        tag: &str,
    ) -> Option<(usize, &'static Field)> {
        fields.iter().enumerate().find(|(_, f)| {
            f.is_element()
                && (f.name.eq_ignore_ascii_case(tag)
                    || f.rename.map_or(false, |r| r.eq_ignore_ascii_case(tag)))
        })
    }

    /// Deserialize an enum (for mixed content).
    fn deserialize_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.peek_event("NodeStart or Text")?;

        match event {
            DomEvent::NodeStart { tag, .. } => {
                let tag = tag.clone();
                // Find variant matching the tag
                let enum_def = match &wip.shape().ty {
                    Type::User(UserType::Enum(def)) => def,
                    _ => {
                        return Err(DomDeserializeError::Unsupported(
                            "expected enum type".into(),
                        ));
                    }
                };

                // Find matching variant
                let variant_idx = enum_def
                    .variants
                    .iter()
                    .position(|v| {
                        let variant_name = v
                            .get_builtin_attr("rename")
                            .and_then(|a| a.get_as::<&str>().copied())
                            .unwrap_or(v.name);
                        variant_name.eq_ignore_ascii_case(&tag)
                    })
                    .or_else(|| {
                        // Check for custom_element fallback
                        enum_def.variants.iter().position(|v| v.is_custom_element())
                    })
                    .ok_or_else(|| DomDeserializeError::UnknownElement {
                        tag: tag.to_string(),
                    })?;

                wip = wip
                    .select_nth_variant(variant_idx)
                    .map_err(DomDeserializeError::Reflect)?;
                wip = self.deserialize_into(wip)?;
            }
            DomEvent::Text(_) => {
                // Text variant
                let event = self.expect_event("Text")?;
                if let DomEvent::Text(text) = event {
                    wip = self.deserialize_text_into_enum(wip, text)?;
                }
            }
            other => {
                return Err(DomDeserializeError::TypeMismatch {
                    expected: "NodeStart or Text",
                    got: format!("{other:?}"),
                });
            }
        }

        Ok(wip)
    }

    /// Deserialize text content into an enum's Text variant.
    fn deserialize_text_into_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        text: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let enum_def = match &wip.shape().ty {
            Type::User(UserType::Enum(def)) => def,
            _ => {
                // Not an enum - try setting as string directly
                return self.set_string_value(wip, text);
            }
        };

        // Find the Text variant
        let text_variant_idx = enum_def
            .variants
            .iter()
            .position(|v| v.is_text())
            .ok_or_else(|| {
                DomDeserializeError::Unsupported("enum has no Text variant for text content".into())
            })?;

        wip = wip
            .select_nth_variant(text_variant_idx)
            .map_err(DomDeserializeError::Reflect)?;
        wip = self.set_string_value(wip, text)?;

        Ok(wip)
    }

    /// Deserialize a scalar value.
    fn deserialize_scalar(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        // For scalars in DOM context, we expect text content
        let event = self.expect_event("Text")?;
        match event {
            DomEvent::Text(text) => self.set_string_value(wip, text),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Text",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Deserialize a list.
    fn deserialize_list(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;

        // Process children until ChildrenEnd
        loop {
            let event = self.peek_event("child or ChildrenEnd")?;
            if matches!(event, DomEvent::ChildrenEnd) {
                break;
            }

            wip = wip
                .begin_list_item()
                .map_err(DomDeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
        }

        Ok(wip)
    }

    /// Deserialize an Option.
    fn deserialize_option(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        // In DOM context, presence of content means Some
        let event = self.peek_event("value")?;
        if matches!(event, DomEvent::ChildrenEnd | DomEvent::NodeEnd) {
            // No content - None
            wip = wip.set_default().map_err(DomDeserializeError::Reflect)?;
        } else {
            // Has content - Some
            wip = wip.begin_some().map_err(DomDeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
        }
        Ok(wip)
    }

    /// Set a string value, handling type conversion.
    fn set_string_value(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        value: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        // Handle Option<T>
        if matches!(&wip.shape().def, Def::Option(_)) {
            wip = wip.begin_some().map_err(DomDeserializeError::Reflect)?;
            wip = self.set_string_value(wip, value)?;
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
            return Ok(wip);
        }

        // Use facet-dessert for string setting
        let span = self.parser.current_span();
        facet_dessert::set_string_value(wip, value, span).map_err(|e| match e {
            facet_dessert::DessertError::Reflect { error, .. } => {
                DomDeserializeError::Reflect(error)
            }
            facet_dessert::DessertError::CannotBorrow { message } => {
                DomDeserializeError::Unsupported(message.into_owned())
            }
        })
    }
}
