//! Tree-based deserializer for DOM documents.

use std::borrow::Cow;

use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::Partial;

use crate::error::DomDeserializeError;
use crate::tracing_macros::{trace, trace_span};
use crate::{AttributeRecord, DomEvent, DomParser, DomParserExt};

mod entrypoints;
mod field_map;
mod struct_deser;

use struct_deser::StructDeserializer;

/// Extension trait for chaining deserialization on `Partial`.
pub(crate) trait PartialDeserializeExt<'de, const BORROW: bool, P: DomParser<'de>> {
    /// Deserialize into this partial using the given deserializer.
    fn deserialize_with(
        self,
        deserializer: &mut DomDeserializer<'de, BORROW, P>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>>;
}

impl<'de, const BORROW: bool, P: DomParser<'de>> PartialDeserializeExt<'de, BORROW, P>
    for Partial<'de, BORROW>
{
    fn deserialize_with(
        self,
        deserializer: &mut DomDeserializer<'de, BORROW, P>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        deserializer.deserialize_into(self)
    }
}

/// DOM deserializer.
///
/// The `BORROW` parameter controls whether strings can be borrowed from the input:
/// - `BORROW = true`: Allows zero-copy deserialization of `&str` and `Cow<str>`
/// - `BORROW = false`: All strings are owned, input doesn't need to outlive result
pub struct DomDeserializer<'de, const BORROW: bool, P> {
    parser: P,
    _marker: std::marker::PhantomData<&'de ()>,
}

impl<'de, const BORROW: bool, P> DomDeserializer<'de, BORROW, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value into an existing Partial.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** The parser should be positioned such that the next event represents
    /// the value to deserialize. For structs/enums, this means a `NodeStart` is next
    /// (peeked but not consumed). For scalars within an element, the parser should be
    /// inside the element (after `ChildrenStart`).
    ///
    /// **Exit:** The parser will have consumed all events related to this value,
    /// including the closing `NodeEnd` for struct types.
    pub fn deserialize_into(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let shape = wip.shape();
        trace!(type_id = %shape.type_identifier, def = ?shape.def, "deserialize_into");

        match &shape.ty {
            Type::User(UserType::Struct(_)) => self.deserialize_struct(wip),
            Type::User(UserType::Enum(_)) => self.deserialize_enum(wip),
            _ => match &shape.def {
                Def::Scalar => self.deserialize_scalar(wip),
                Def::Pointer(_) => self.deserialize_pointer(wip),
                Def::List(_) => self.deserialize_list(wip),
                Def::Set(_) => self.deserialize_set(wip),
                Def::Map(_) => self.deserialize_map(wip),
                Def::Option(_) => self.deserialize_option(wip),
                _ => Err(DomDeserializeError::Unsupported(format!(
                    "unsupported type: {:?}",
                    shape.ty
                ))),
            },
        }
    }

    /// Deserialize a struct type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned before the struct's `NodeStart` (peeked, not consumed).
    ///
    /// **Exit:** Parser has consumed through the struct's closing `NodeEnd`.
    fn deserialize_struct(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DomDeserializeError::Unsupported(
                    "expected struct type".into(),
                ));
            }
        };

        self.deserialize_struct_innards(wip, struct_def)
    }

    /// Deserialize the innards of a struct-like thing (struct, tuple, or enum variant data).
    ///
    /// Delegates to `StructDeserializer` for the actual implementation.
    fn deserialize_struct_innards(
        &mut self,
        wip: Partial<'de, BORROW>,
        struct_def: &'static facet_core::StructType,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace_span!("deserialize_struct_innards");

        // Extract xml::ns_all attribute from the shape
        let ns_all = wip
            .shape()
            .attributes
            .iter()
            .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
            .and_then(|attr| attr.get_as::<&str>().copied());

        StructDeserializer::new(self, struct_def, ns_all).deserialize(wip)
    }

    /// Deserialize an enum type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned at either:
    /// - A `NodeStart` event (element-based variant), or
    /// - A `Text` event (text-based variant, e.g., for enums with a `#[xml::text]` variant)
    ///
    /// **Exit:** All events for this enum have been consumed:
    /// - If entry was `NodeStart`: through the closing `NodeEnd`
    /// - If entry was `Text`: just that text event
    ///
    /// # Variant Selection
    ///
    /// For `NodeStart`: The element tag name is matched against variant names (considering
    /// `#[rename]` attributes). If no match, looks for a variant with `#[xml::custom_element]`.
    ///
    /// For `Text`: Looks for a variant with `#[xml::text]` attribute.
    fn deserialize_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.parser.peek_event_or_eof("NodeStart or Text")?;

        match event {
            DomEvent::NodeStart { tag, .. } => {
                let tag = tag.clone();
                let enum_def = match &wip.shape().ty {
                    Type::User(UserType::Enum(def)) => def,
                    _ => {
                        return Err(DomDeserializeError::Unsupported(
                            "expected enum type".into(),
                        ));
                    }
                };

                // For untagged enums, the element tag is the enum's name (not a variant name)
                // We need to select the first variant and deserialize the content into it
                let is_untagged = wip.shape().is_untagged();

                let variant_idx = if is_untagged {
                    // For untagged enums, select the first (and typically only) variant
                    // The element tag should match the enum's rename, not a variant name
                    trace!(tag = %tag, "untagged enum - selecting first variant");
                    0
                } else {
                    // For tagged enums, match the element tag against variant names
                    enum_def
                        .variants
                        .iter()
                        .position(|v| {
                            let variant_name = v
                                .get_builtin_attr("rename")
                                .and_then(|a| a.get_as::<&str>().copied())
                                .unwrap_or(v.name);
                            variant_name.eq_ignore_ascii_case(&tag)
                        })
                        .or_else(|| enum_def.variants.iter().position(|v| v.is_custom_element()))
                        .ok_or_else(|| DomDeserializeError::UnknownElement {
                            tag: tag.to_string(),
                        })?
                };

                let variant = &enum_def.variants[variant_idx];
                wip = wip.select_nth_variant(variant_idx)?;
                trace!(variant_name = variant.name, variant_kind = ?variant.data.kind, is_untagged, "selected variant");

                // Handle variant based on its kind
                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant: just consume the element
                        self.parser.expect_node_start()?;
                        // Skip to end of element
                        let event = self.parser.peek_event_or_eof("ChildrenStart or NodeEnd")?;
                        if matches!(event, DomEvent::ChildrenStart) {
                            self.parser.expect_children_start()?;
                            self.parser.expect_children_end()?;
                        }
                        self.parser.expect_node_end()?;
                    }
                    StructKind::TupleStruct => {
                        // Newtype variant: deserialize the inner type
                        // The variant data has one field (index 0)
                        wip = wip.begin_nth_field(0)?.deserialize_with(self)?.end()?;
                    }
                    StructKind::Struct | StructKind::Tuple => {
                        // Struct/tuple variant: deserialize using the variant's data as a StructType
                        wip = self.deserialize_struct_innards(wip, &variant.data)?;
                    }
                }
            }
            DomEvent::Text(_) => {
                let text = self.parser.expect_text()?;
                wip = self.deserialize_text_into_enum(wip, text)?;
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

    /// Deserialize text content into an enum by selecting the `#[xml::text]` variant.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** The text has already been consumed from the parser (passed as argument).
    ///
    /// **Exit:** No parser state change (text was already consumed).
    ///
    /// # Fallback
    ///
    /// If `wip` is not actually an enum, falls back to `set_string_value`.
    fn deserialize_text_into_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        text: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let enum_def = match &wip.shape().ty {
            Type::User(UserType::Enum(def)) => def,
            _ => {
                return self.set_string_value(wip, text);
            }
        };

        let text_variant_idx = enum_def
            .variants
            .iter()
            .position(|v| v.is_text())
            .ok_or_else(|| {
                DomDeserializeError::Unsupported("enum has no Text variant for text content".into())
            })?;

        wip = wip.select_nth_variant(text_variant_idx)?;
        wip = self.set_string_value(wip, text)?;

        Ok(wip)
    }

    /// Deserialize a scalar value (string, number, bool, etc.).
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned at either:
    /// - A `Text` event (inline text content), or
    /// - A `NodeStart` event (element wrapping the text content)
    ///
    /// **Exit:** All events for this scalar have been consumed:
    /// - If entry was `Text`: just that text event
    /// - If entry was `NodeStart`: through the closing `NodeEnd`
    ///
    /// # XML Data Model
    ///
    /// In XML, scalars can appear as:
    /// - Attribute values (handled elsewhere)
    /// - Text content: `<parent>text here</parent>`
    /// - Element with text: `<field>value</field>` (element is consumed)
    fn deserialize_scalar(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!("deserialize_scalar called");
        let event = self.parser.peek_event_or_eof("Text or NodeStart")?;
        trace!(event = ?event, "peeked event in deserialize_scalar");
        match event {
            DomEvent::Text(_) => {
                trace!("deserialize_scalar: matched Text arm");
                let text = self.parser.expect_text()?;
                self.set_string_value(wip, text)
            }
            DomEvent::NodeStart { .. } => {
                trace!("deserialize_scalar: matched NodeStart arm");
                let _tag = self.parser.expect_node_start()?;
                trace!(tag = %_tag, "deserialize_scalar: consumed NodeStart");

                loop {
                    let event = self
                        .parser
                        .peek_event_or_eof("Attribute or ChildrenStart or NodeEnd")?;
                    trace!(event = ?event, "deserialize_scalar: in attr loop");
                    match event {
                        DomEvent::Attribute { .. } => {
                            let AttributeRecord {
                                name: _name,
                                value: _value,
                                namespace: _namespace,
                            } = self.parser.expect_attribute()?;
                            trace!(name = %_name, "deserialize_scalar: consumed Attribute");
                        }
                        DomEvent::ChildrenStart => {
                            self.parser.expect_children_start()?;
                            trace!("deserialize_scalar: consumed ChildrenStart");
                            break;
                        }
                        DomEvent::NodeEnd => {
                            self.parser.expect_node_end()?;
                            trace!("deserialize_scalar: void element, returning empty string");
                            return self.set_string_value(wip, Cow::Borrowed(""));
                        }
                        other => {
                            trace!(other = ?other, "deserialize_scalar: unexpected event in attr loop");
                            return Err(DomDeserializeError::TypeMismatch {
                                expected: "Attribute or ChildrenStart or NodeEnd",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }

                trace!("deserialize_scalar: starting text content loop");
                let mut text_content = String::new();
                loop {
                    let event = self.parser.peek_event_or_eof("Text or ChildrenEnd")?;
                    trace!(event = ?event, "deserialize_scalar: in text content loop");
                    match event {
                        DomEvent::Text(_) => {
                            let text = self.parser.expect_text()?;
                            trace!(text = %text, "deserialize_scalar: got text");
                            text_content.push_str(&text);
                        }
                        DomEvent::ChildrenEnd => {
                            trace!("deserialize_scalar: got ChildrenEnd, breaking text loop");
                            break;
                        }
                        DomEvent::NodeStart { .. } => {
                            trace!("deserialize_scalar: skipping nested NodeStart");
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                        DomEvent::Comment(_) => {
                            let _comment = self.parser.expect_comment()?;
                        }
                        other => {
                            return Err(DomDeserializeError::TypeMismatch {
                                expected: "Text or ChildrenEnd",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }

                trace!("deserialize_scalar: consuming ChildrenEnd");
                self.parser.expect_children_end()?;
                trace!("deserialize_scalar: consuming NodeEnd");
                self.parser.expect_node_end()?;
                trace!(text_content = %text_content, "deserialize_scalar: setting string value");

                self.set_string_value(wip, Cow::Owned(text_content))
            }
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Text or NodeStart",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Deserialize a list (Vec, slice, etc.) from repeated child elements.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned inside an element, after `ChildrenStart`.
    /// Child elements will be deserialized as list items.
    ///
    /// **Exit:** Parser is positioned at `ChildrenEnd` (peeked, not consumed).
    /// The caller is responsible for consuming `ChildrenEnd` and `NodeEnd`.
    ///
    /// # Note
    ///
    /// This is used for "wrapped" list semantics where a parent element contains
    /// the list items. For "flat" list semantics (items directly as siblings),
    /// see the flat sequence handling in `deserialize_struct_innards`.
    fn deserialize_list(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = wip.init_list()?;

        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
            if matches!(event, DomEvent::ChildrenEnd) {
                break;
            }

            wip = wip.begin_list_item()?.deserialize_with(self)?.end()?;
        }

        Ok(wip)
    }

    /// Deserialize a set type (HashSet, BTreeSet, etc.).
    ///
    /// Works the same as lists: each child element becomes a set item.
    fn deserialize_set(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = wip.init_set()?;

        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
            if matches!(event, DomEvent::ChildrenEnd) {
                break;
            }

            wip = wip.begin_set_item()?.deserialize_with(self)?.end()?;
        }

        Ok(wip)
    }

    /// Deserialize a map type (HashMap, BTreeMap, etc.).
    ///
    /// In XML, maps use a **wrapped** model:
    /// - The field name becomes a wrapper element
    /// - Each child element becomes a map entry (tag = key, content = value)
    ///
    /// Example: `<data><alpha>1</alpha><beta>2</beta></data>` -> {"alpha": 1, "beta": 2}
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned at the wrapper element's `NodeStart`.
    ///
    /// **Exit:** Parser has consumed through the wrapper element's `NodeEnd`.
    fn deserialize_map(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        // Consume the wrapper element's NodeStart
        let event = self.parser.peek_event_or_eof("NodeStart for map wrapper")?;
        match event {
            DomEvent::NodeStart { tag, .. } => {
                trace!(wrapper_tag = %tag, "map wrapper element");
                let _ = self.parser.expect_node_start()?;
            }
            other => {
                return Err(DomDeserializeError::TypeMismatch {
                    expected: "NodeStart for map wrapper",
                    got: format!("{other:?}"),
                });
            }
        }

        // Skip attributes on the wrapper element
        loop {
            let event = self
                .parser
                .peek_event_or_eof("Attribute or ChildrenStart or NodeEnd")?;
            match event {
                DomEvent::Attribute { .. } => {
                    self.parser.expect_attribute()?;
                }
                DomEvent::ChildrenStart => {
                    self.parser.expect_children_start()?;
                    break;
                }
                DomEvent::NodeEnd => {
                    // Empty map (void element)
                    self.parser.expect_node_end()?;
                    return Ok(wip.init_map()?);
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "Attribute or ChildrenStart or NodeEnd",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        wip = wip.init_map()?;

        // Now parse map entries from children
        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
            match event {
                DomEvent::ChildrenEnd => break,
                DomEvent::NodeStart { tag, .. } => {
                    let key = tag.clone();
                    trace!(key = %key, "map entry");

                    // Set the key (element name)
                    wip = wip.begin_key()?;
                    wip = self.set_string_value(wip, key)?;
                    wip = wip.end()?;

                    // Deserialize the value (element content)
                    wip = wip.begin_value()?.deserialize_with(self)?.end()?;
                }
                DomEvent::Text(_) | DomEvent::Comment(_) => {
                    // Skip whitespace text and comments between map entries
                    if matches!(event, DomEvent::Text(_)) {
                        self.parser.expect_text()?;
                    } else {
                        self.parser.expect_comment()?;
                    }
                }
                _ => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "map entry element",
                        got: format!("{event:?}"),
                    });
                }
            }
        }

        // Consume wrapper's ChildrenEnd and NodeEnd
        self.parser.expect_children_end()?;
        self.parser.expect_node_end()?;

        Ok(wip)
    }

    /// Deserialize an Option type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned where the optional value would be.
    ///
    /// **Exit:** If value was present, all events for the value have been consumed.
    /// If value was absent, no events consumed.
    ///
    /// # None Detection
    ///
    /// The option is `None` if the next event is `ChildrenEnd` or `NodeEnd`
    /// (indicating no content). Otherwise, the inner value is deserialized.
    fn deserialize_option(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.parser.peek_event_or_eof("value")?;
        if matches!(event, DomEvent::ChildrenEnd | DomEvent::NodeEnd) {
            wip = wip.set_default()?;
        } else {
            wip = wip.begin_some()?.deserialize_with(self)?.end()?;
        }
        Ok(wip)
    }

    /// Deserialize a pointer type (Box, Arc, Rc, etc.).
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned at the value that the pointer will wrap.
    ///
    /// **Exit:** All events for the inner value have been consumed.
    ///
    /// # Pointer Actions
    ///
    /// Uses `facet_dessert::begin_pointer` to determine how to handle the pointer:
    /// - `HandleAsScalar`: Treat as scalar (e.g., `Box<str>`)
    /// - `SliceBuilder`: Build a slice (e.g., `Arc<[T]>`)
    /// - `SizedPointee`: Regular pointer to sized type
    fn deserialize_pointer(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        use facet_dessert::{PointerAction, begin_pointer};

        let (wip, action) = begin_pointer(wip)?;

        match action {
            PointerAction::HandleAsScalar => self.deserialize_scalar(wip),
            PointerAction::SliceBuilder => Ok(self.deserialize_list(wip)?.end()?),
            PointerAction::SizedPointee => Ok(wip.deserialize_with(self)?.end()?),
        }
    }

    /// Set a string value on the current partial, parsing it to the appropriate type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry/Exit:** No parser state change. The string value is passed as an argument.
    ///
    /// # Type Handling
    ///
    /// Delegates to `facet_dessert::set_string_value` which handles parsing the string
    /// into the appropriate scalar type (String, &str, integers, floats, bools, etc.).
    pub(crate) fn set_string_value(
        &mut self,
        wip: Partial<'de, BORROW>,
        value: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        Ok(facet_dessert::set_string_value(
            wip,
            value,
            self.parser.current_span(),
        )?)
    }

    /// Set a string value, handling field-level proxy conversion if present.
    ///
    /// If the field has a proxy attribute (e.g., `#[facet(proxy = PointsProxy)]`),
    /// this will:
    /// 1. Begin custom deserialization (push a frame for the proxy type)
    /// 2. Set the string value into the proxy type
    /// 3. End the frame (which converts proxy -> target via TryFrom)
    ///
    /// If no proxy is present, it just calls `set_string_value` directly.
    pub(crate) fn set_string_value_with_proxy(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        value: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        // Check if the field has a proxy
        let has_proxy = wip.parent_field().and_then(|f| f.proxy()).is_some();

        if has_proxy {
            // Use custom deserialization through the proxy
            wip = wip.begin_custom_deserialization()?;
            wip = self.set_string_value(wip, value)?;
            wip = wip.end()?;
            Ok(wip)
        } else {
            self.set_string_value(wip, value)
        }
    }
}
