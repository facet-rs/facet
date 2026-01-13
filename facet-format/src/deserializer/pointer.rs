extern crate alloc;

use facet_core::Def;
use facet_reflect::Partial;

use crate::{
    DeserializeError, FormatDeserializer, FormatParser, ParseEvent, ScalarTypeHint, ScalarValue,
};

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    pub(crate) fn deserialize_pointer(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::KnownPointer;

        let shape = wip.shape();
        let is_cow = if let Def::Pointer(ptr_def) = shape.def {
            matches!(ptr_def.known, Some(KnownPointer::Cow))
        } else {
            false
        };

        if is_cow {
            // Cow<str> - handle specially to preserve borrowing
            if let Def::Pointer(ptr_def) = shape.def
                && let Some(pointee) = ptr_def.pointee()
                && pointee.type_identifier == "str"
            {
                // Hint to non-self-describing parsers that a string is expected
                self.parser.hint_scalar_type(ScalarTypeHint::String);
                let event = self.expect_event("string for Cow<str>")?;
                match event {
                    ParseEvent::Scalar(ScalarValue::Str(s))
                    | ParseEvent::Scalar(ScalarValue::StringlyTyped(s)) => {
                        // Pass through the Cow as-is to preserve borrowing
                        wip = wip.set(s).map_err(DeserializeError::reflect)?;
                        return Ok(wip);
                    }
                    _ => {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "string for Cow<str>",
                            got: format!("{event:?}"),
                            span: self.last_span,
                            path: None,
                        });
                    }
                }
            }
            // Cow<[u8]> - handle specially to preserve borrowing
            if let Def::Pointer(ptr_def) = shape.def
                && let Some(pointee) = ptr_def.pointee()
                && let Def::Slice(slice_def) = pointee.def
                && slice_def.t.type_identifier == "u8"
            {
                // Hint to non-self-describing parsers that bytes are expected
                self.parser.hint_scalar_type(ScalarTypeHint::Bytes);
                let event = self.expect_event("bytes for Cow<[u8]>")?;
                if let ParseEvent::Scalar(ScalarValue::Bytes(b)) = event {
                    // Pass through the Cow as-is to preserve borrowing
                    wip = wip.set(b).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                } else {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "bytes for Cow<[u8]>",
                        got: format!("{event:?}"),
                        span: self.last_span,
                        path: None,
                    });
                }
            }
            // Other Cow types - use begin_inner
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // &str - handle specially for zero-copy borrowing
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            // Hint to non-self-describing parsers that a string is expected
            self.parser.hint_scalar_type(ScalarTypeHint::String);
            let event = self.expect_event("string for &str")?;
            match event {
                ParseEvent::Scalar(ScalarValue::Str(s))
                | ParseEvent::Scalar(ScalarValue::StringlyTyped(s)) => {
                    return self.set_string_value(wip, s);
                }
                _ => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "string for &str",
                        got: format!("{event:?}"),
                        span: self.last_span,
                        path: None,
                    });
                }
            }
        }

        // &[u8] - handle specially for zero-copy borrowing
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && let Some(pointee) = ptr_def.pointee()
            && let Def::Slice(slice_def) = pointee.def
            && slice_def.t.type_identifier == "u8"
        {
            // Hint to non-self-describing parsers that bytes are expected
            self.parser.hint_scalar_type(ScalarTypeHint::Bytes);
            let event = self.expect_event("bytes for &[u8]")?;
            if let ParseEvent::Scalar(ScalarValue::Bytes(b)) = event {
                return self.set_bytes_value(wip, b);
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "bytes for &[u8]",
                    got: format!("{event:?}"),
                    span: self.last_span,
                    path: None,
                });
            }
        }

        // Regular smart pointer (Box, Arc, Rc)
        wip = wip.begin_smart_ptr().map_err(DeserializeError::reflect)?;

        // Check if begin_smart_ptr set up a slice builder (for Arc<[T]>, Rc<[T]>, Box<[T]>)
        // In this case, we need to deserialize as a list manually
        let is_slice_builder = wip.is_building_smart_ptr_slice();

        if is_slice_builder {
            // Deserialize the list elements into the slice builder
            // We can't use deserialize_list() because it calls begin_list() which interferes
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
                        expected: "sequence start for Arc<[T]>/Rc<[T]>/Box<[T]>",
                        got: format!("{event:?}"),
                        span: self.last_span,
                        path: None,
                    });
                }
            };

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

                wip = wip.begin_list_item().map_err(DeserializeError::reflect)?;
                wip = self.deserialize_into(wip)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            }

            // Convert the slice builder to Arc/Rc/Box and mark as initialized
            wip = wip.end().map_err(DeserializeError::reflect)?;
            // DON'T call end() again - the caller (deserialize_struct) will do that
        } else {
            // Regular smart pointer with sized pointee
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }
}
