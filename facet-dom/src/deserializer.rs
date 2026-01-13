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
pub struct DomDeserializer<'de, P> {
    parser: P,
    _marker: std::marker::PhantomData<&'de ()>,
}

impl<'de, P> DomDeserializer<'de, P>
where
    P: DomParser<'de>,
{
    /// Create a new DOM deserializer.
    pub fn new(parser: P) -> Self {
        Self {
            parser,
            _marker: std::marker::PhantomData,
        }
    }

    /// Deserialize a value of type `T`.
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

    /// Deserialize into an existing Partial.
    pub fn deserialize_into(
        &mut self,
        wip: Partial<'de, true>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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
        mut wip: Partial<'de, true>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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

        // Find the elements field (if any)
        let elements_field = struct_def
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.is_elements());

        // Process children
        if let Some((elements_idx, _elements_field)) = elements_field {
            // Begin the elements list
            wip = wip
                .begin_nth_field(elements_idx)
                .map_err(DomDeserializeError::Reflect)?;
            wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;

            loop {
                let event = self.peek_event("child or ChildrenEnd")?;
                match event {
                    DomEvent::ChildrenEnd => break,
                    DomEvent::Text(_) => {
                        let event = self.expect_event("Text")?;
                        if let DomEvent::Text(text) = event {
                            // Add text as a list item
                            wip = wip
                                .begin_list_item()
                                .map_err(DomDeserializeError::Reflect)?;
                            wip = self.deserialize_text_into_enum(wip, text)?;
                            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                        }
                    }
                    DomEvent::NodeStart { .. } => {
                        // Add child element as a list item
                        wip = wip
                            .begin_list_item()
                            .map_err(DomDeserializeError::Reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                    }
                    DomEvent::Comment(_) => {
                        // Skip comments
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

            // End the list and field
            wip = wip.end().map_err(DomDeserializeError::Reflect)?; // end field

            // Consume ChildrenEnd
            let _ = self.expect_event("ChildrenEnd")?;
        } else {
            // No elements field - check for text field
            let text_field = struct_def
                .fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.is_text());

            if let Some((text_idx, _)) = text_field {
                // Collect all text content
                let mut text_content = String::new();
                loop {
                    let event = self.peek_event("text or ChildrenEnd")?;
                    match event {
                        DomEvent::ChildrenEnd => break,
                        DomEvent::Text(_) => {
                            let event = self.expect_event("Text")?;
                            if let DomEvent::Text(text) = event {
                                text_content.push_str(&text);
                            }
                        }
                        DomEvent::NodeStart { .. } => {
                            // Skip unknown child elements
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                        DomEvent::Comment(_) => {
                            let _ = self.expect_event("Comment")?;
                        }
                        other => {
                            return Err(DomDeserializeError::TypeMismatch {
                                expected: "text or ChildrenEnd",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }

                // Set the text field
                wip = wip
                    .begin_nth_field(text_idx)
                    .map_err(DomDeserializeError::Reflect)?;
                wip = self.set_string_value(wip, Cow::Owned(text_content))?;
                wip = wip.end().map_err(DomDeserializeError::Reflect)?;

                // Consume ChildrenEnd
                let _ = self.expect_event("ChildrenEnd")?;
            } else {
                // No text or elements field - skip all children
                loop {
                    let event = self.peek_event("ChildrenEnd")?;
                    if matches!(event, DomEvent::ChildrenEnd) {
                        break;
                    }
                    match self.expect_event("child")? {
                        DomEvent::NodeStart { .. } => {
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                        _ => {}
                    }
                }
                let _ = self.expect_event("ChildrenEnd")?;
            }
        }

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

    /// Deserialize an enum (for mixed content).
    fn deserialize_enum(
        &mut self,
        mut wip: Partial<'de, true>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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
        mut wip: Partial<'de, true>,
        text: Cow<'de, str>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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
        wip: Partial<'de, true>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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
        mut wip: Partial<'de, true>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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
        mut wip: Partial<'de, true>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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
        mut wip: Partial<'de, true>,
        value: Cow<'de, str>,
    ) -> Result<Partial<'de, true>, DomDeserializeError<P::Error>> {
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
