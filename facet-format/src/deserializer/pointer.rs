use facet_core::{Def, Facet};
use facet_reflect::Partial;

use crate::{
    DeserializeError, DeserializeErrorKind, FormatDeserializer, ParseEventKind, ScalarTypeHint,
    ScalarValue, SpanGuard, deserializer::entry::MetaSource,
};

impl<'parser, 'input, const BORROW: bool> FormatDeserializer<'parser, 'input, BORROW> {
    pub(crate) fn deserialize_pointer(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        meta: MetaSource<'input>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
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
                && *pointee == *str::SHAPE
            {
                // Hint to non-self-describing parsers that a string is expected
                if self.is_non_self_describing() {
                    self.parser.hint_scalar_type(ScalarTypeHint::String);
                }
                let event = self.expect_event("string for Cow<str>")?;
                let _guard = SpanGuard::new(self.last_span);
                match event.kind {
                    ParseEventKind::Scalar(ScalarValue::Str(s)) => {
                        // Pass through the Cow as-is to preserve borrowing
                        return Ok(wip.set(s)?);
                    }
                    // For self-describing formats like YAML, unquoted values may be
                    // parsed as other scalar types. Convert them to owned strings.
                    ParseEventKind::Scalar(ScalarValue::I64(n)) => {
                        return Ok(wip.set(std::borrow::Cow::<'_, str>::Owned(n.to_string()))?);
                    }
                    ParseEventKind::Scalar(ScalarValue::U64(n)) => {
                        return Ok(wip.set(std::borrow::Cow::<'_, str>::Owned(n.to_string()))?);
                    }
                    ParseEventKind::Scalar(ScalarValue::F64(n)) => {
                        return Ok(wip.set(std::borrow::Cow::<'_, str>::Owned(n.to_string()))?);
                    }
                    ParseEventKind::Scalar(ScalarValue::Bool(b)) => {
                        return Ok(wip.set(std::borrow::Cow::<'_, str>::Owned(b.to_string()))?);
                    }
                    _ => {
                        return Err(self.mk_err(
                            &wip,
                            DeserializeErrorKind::UnexpectedToken {
                                expected: "string for Cow<str>",
                                got: event.kind_name().into(),
                            },
                        ));
                    }
                }
            }
            // Cow<[u8]> - handle specially to preserve borrowing
            if let Def::Pointer(ptr_def) = shape.def
                && let Some(pointee) = ptr_def.pointee()
                && let Def::Slice(slice_def) = pointee.def
                && *slice_def.t == *u8::SHAPE
            {
                // Hint to non-self-describing parsers that bytes are expected
                if self.is_non_self_describing() {
                    self.parser.hint_scalar_type(ScalarTypeHint::Bytes);
                }
                let event = self.expect_event("bytes for Cow<[u8]>")?;
                let _guard = SpanGuard::new(self.last_span);
                if let ParseEventKind::Scalar(ScalarValue::Bytes(b)) = event.kind {
                    // Pass through the Cow as-is to preserve borrowing
                    return Ok(wip.set(b)?);
                } else {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "bytes for Cow<[u8]>",
                            got: event.kind_name().into(),
                        },
                    ));
                }
            }
            // Other Cow types - use begin_inner
            let _guard = SpanGuard::new(self.last_span);
            wip = wip
                .begin_inner()?
                .with(|w| self.deserialize_into(w, meta))?
                .end()?;
            return Ok(wip);
        }

        // &str - handle specially for zero-copy borrowing
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def.pointee().is_some_and(|p| *p == *str::SHAPE)
        {
            // Hint to non-self-describing parsers that a string is expected
            if self.is_non_self_describing() {
                self.parser.hint_scalar_type(ScalarTypeHint::String);
            }
            let event = self.expect_event("string for &str")?;
            match event.kind {
                ParseEventKind::Scalar(ScalarValue::Str(s)) => {
                    return self.set_string_value(wip, s);
                }
                _ => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "string for &str",
                            got: event.kind_name().into(),
                        },
                    ));
                }
            }
        }

        // &[u8] - handle specially for zero-copy borrowing
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && let Some(pointee) = ptr_def.pointee()
            && let Def::Slice(slice_def) = pointee.def
            && *slice_def.t == *u8::SHAPE
        {
            // Hint to non-self-describing parsers that bytes are expected
            if self.is_non_self_describing() {
                self.parser.hint_scalar_type(ScalarTypeHint::Bytes);
            }
            let event = self.expect_event("bytes for &[u8]")?;
            if let ParseEventKind::Scalar(ScalarValue::Bytes(b)) = event.kind {
                return self.set_bytes_value(wip, b);
            } else {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "bytes for &[u8]",
                        got: event.kind_name().into(),
                    },
                ));
            }
        }

        // Regular smart pointer (Box, Arc, Rc)
        let _guard = SpanGuard::new(self.last_span);
        wip = wip.begin_smart_ptr()?;

        // Check if begin_smart_ptr set up a slice builder (for Arc<[T]>, Rc<[T]>, Box<[T]>)
        // In this case, we need to deserialize as a list manually
        if wip.is_building_smart_ptr_slice() {
            // Deserialize the list elements into the slice builder
            // We can't use deserialize_list() because it calls begin_list() which interferes
            // Hint to non-self-describing parsers that a sequence is expected
            if self.is_non_self_describing() {
                self.parser.hint_sequence();
            }
            let event = self.expect_event("value")?;
            let _guard = SpanGuard::new(self.last_span);

            match event.kind {
                ParseEventKind::SequenceStart(_) => {}
                ParseEventKind::StructStart(kind) => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "array",
                            got: kind.name().into(),
                        },
                    ));
                }
                _ => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "sequence start for Arc<[T]>/Rc<[T]>/Box<[T]>",
                            got: event.kind_name().into(),
                        },
                    ));
                }
            };

            loop {
                let event = self.expect_peek("value")?;

                // Check for end of sequence
                if matches!(event.kind, ParseEventKind::SequenceEnd) {
                    self.expect_event("value")?;
                    break;
                }

                let _guard = SpanGuard::new(self.last_span);
                // List items get fresh metadata from events
                wip = wip
                    .begin_list_item()?
                    .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                    .end()?;
            }

            // Convert the slice builder to Arc/Rc/Box and mark as initialized
            let _guard = SpanGuard::new(self.last_span);
            wip = wip.end()?;
            // DON'T call end() again - the caller (deserialize_struct) will do that
        } else {
            // Regular smart pointer with sized pointee - pass through the metadata
            wip = wip.with(|w| self.deserialize_into(w, meta))?.end()?;
        }

        Ok(wip)
    }
}
