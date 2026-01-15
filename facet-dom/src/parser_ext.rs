//! Extension trait for DomParser with expect_* convenience methods.

use std::borrow::Cow;

use crate::trace;
use crate::{DomDeserializeError, DomEvent, DomParser};

/// Extension trait adding convenience methods to any `DomParser`.
pub trait DomParserExt<'de>: DomParser<'de> {
    /// Get the next event, returning an error if EOF.
    fn next_event_or_eof(
        &mut self,
        expected: &'static str,
    ) -> Result<DomEvent<'de>, DomDeserializeError<Self::Error>> {
        let event = self
            .next_event()
            .map_err(DomDeserializeError::Parser)?
            .ok_or(DomDeserializeError::UnexpectedEof { expected })?;
        trace!(event = %event.trace(), kind = %"next");
        Ok(event)
    }

    /// Peek at the next event, returning an error if EOF.
    fn peek_event_or_eof(
        &mut self,
        expected: &'static str,
    ) -> Result<&DomEvent<'de>, DomDeserializeError<Self::Error>> {
        let event = self
            .peek_event()
            .map_err(DomDeserializeError::Parser)?
            .ok_or(DomDeserializeError::UnexpectedEof { expected })?;
        trace!(event = %event.trace(), kind = %"peek");
        Ok(event)
    }

    /// Expect and consume a NodeStart event, returning the tag name.
    fn expect_node_start(&mut self) -> Result<Cow<'de, str>, DomDeserializeError<Self::Error>> {
        match self.next_event_or_eof("NodeStart")? {
            DomEvent::NodeStart { tag, .. } => Ok(tag),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "NodeStart",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a ChildrenStart event.
    fn expect_children_start(&mut self) -> Result<(), DomDeserializeError<Self::Error>> {
        match self.next_event_or_eof("ChildrenStart")? {
            DomEvent::ChildrenStart => Ok(()),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "ChildrenStart",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a ChildrenEnd event.
    fn expect_children_end(&mut self) -> Result<(), DomDeserializeError<Self::Error>> {
        match self.next_event_or_eof("ChildrenEnd")? {
            DomEvent::ChildrenEnd => Ok(()),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "ChildrenEnd",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a NodeEnd event.
    fn expect_node_end(&mut self) -> Result<(), DomDeserializeError<Self::Error>> {
        match self.next_event_or_eof("NodeEnd")? {
            DomEvent::NodeEnd => Ok(()),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "NodeEnd",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a Text event, returning the text content.
    fn expect_text(&mut self) -> Result<Cow<'de, str>, DomDeserializeError<Self::Error>> {
        match self.next_event_or_eof("Text")? {
            DomEvent::Text(text) => Ok(text),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Text",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume an Attribute event, returning (name, value, namespace).
    fn expect_attribute(
        &mut self,
    ) -> Result<AttributeRecord<'de>, DomDeserializeError<Self::Error>> {
        match self.next_event_or_eof("Attribute")? {
            DomEvent::Attribute {
                name,
                value,
                namespace,
            } => Ok(AttributeRecord {
                name,
                value,
                namespace,
            }),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Attribute",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a Comment event, returning the comment text.
    fn expect_comment(&mut self) -> Result<Cow<'de, str>, DomDeserializeError<Self::Error>> {
        match self.next_event_or_eof("Comment")? {
            DomEvent::Comment(text) => Ok(text),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Comment",
                got: format!("{other:?}"),
            }),
        }
    }
}

/// An attribute name-value-namespace triple from a DOM event.
pub struct AttributeRecord<'de> {
    /// The attribute name.
    pub name: Cow<'de, str>,
    /// The attribute value.
    pub value: Cow<'de, str>,
    /// The attribute namespace URI, if any.
    pub namespace: Option<Cow<'de, str>>,
}

impl<'de, P: DomParser<'de>> DomParserExt<'de> for P {}
