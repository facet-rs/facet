extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::fmt;

use facet_core::Facet;
use facet_value::{VArray, VObject, Value, ValueError, from_value};

use crate::{FormatParser, ParseEvent, ScalarValue};

/// Generic facade around a format-specific parser.
pub struct FormatDeserializer<P> {
    parser: P,
}

impl<P> FormatDeserializer<P> {
    /// Create a new facade around the parser.
    pub const fn new(parser: P) -> Self {
        Self { parser }
    }

    /// Consume the facade and return the underlying parser.
    pub fn into_inner(self) -> P {
        self.parser
    }

    /// Borrow the inner parser mutably.
    pub fn parser_mut(&mut self) -> &mut P {
        &mut self.parser
    }
}

impl<'de, P> FormatDeserializer<P>
where
    P: FormatParser<'de>,
{
    /// Deserialize the next value in the stream into `T`.
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        let value = self.build_value()?;
        from_value(value).map_err(DeserializeError::Value)
    }

    fn build_value(&mut self) -> Result<Value, DeserializeError<P::Error>> {
        enum Frame {
            Struct {
                fields: Vec<(String, Value)>,
                pending_key: Option<String>,
            },
            Sequence {
                items: Vec<Value>,
            },
        }

        fn push_value(value: Value, stack: &mut [Frame]) -> Result<Option<Value>, &'static str> {
            match stack.last_mut() {
                Some(Frame::Struct {
                    fields,
                    pending_key,
                }) => {
                    let key = pending_key.take().ok_or("missing field key before value")?;
                    fields.push((key, value));
                    Ok(None)
                }
                Some(Frame::Sequence { items }) => {
                    items.push(value);
                    Ok(None)
                }
                None => Ok(Some(value)),
            }
        }

        fn finish_frame(frame: Frame) -> Result<Value, &'static str> {
            match frame {
                Frame::Struct {
                    fields,
                    pending_key,
                } => {
                    if pending_key.is_some() {
                        return Err("struct finished while waiting for a value");
                    }
                    let mut object = VObject::new();
                    for (key, value) in fields {
                        object.insert(key, value);
                    }
                    Ok(Value::from(object))
                }
                Frame::Sequence { items } => {
                    let mut array = VArray::new();
                    for item in items {
                        array.push(item);
                    }
                    Ok(Value::from(array))
                }
            }
        }

        fn scalar_to_value(value: ScalarValue<'_>) -> Value {
            match value {
                ScalarValue::Null => Value::NULL,
                ScalarValue::Bool(bool) => Value::from(bool),
                ScalarValue::I64(int) => Value::from(int),
                ScalarValue::U64(int) => Value::from(int),
                ScalarValue::F64(float) => Value::from(float),
                ScalarValue::Str(text) => Value::from(text.into_owned()),
                ScalarValue::Bytes(bytes) => Value::from(bytes.into_owned()),
            }
        }

        let mut stack: Vec<Frame> = Vec::new();

        let root = loop {
            let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
            match event {
                ParseEvent::StructStart => stack.push(Frame::Struct {
                    fields: Vec::new(),
                    pending_key: None,
                }),
                ParseEvent::StructEnd => {
                    let frame = stack.pop().ok_or(DeserializeError::Structure(
                        "received StructEnd without matching StructStart",
                    ))?;
                    let value = finish_frame(frame).map_err(DeserializeError::Structure)?;
                    if let Some(root) =
                        push_value(value, &mut stack).map_err(DeserializeError::Structure)?
                    {
                        break root;
                    }
                }
                ParseEvent::FieldKey(name, _) => match stack.last_mut() {
                    Some(Frame::Struct { pending_key, .. }) => {
                        *pending_key = Some(name.into_owned());
                    }
                    _ => {
                        return Err(DeserializeError::Structure(
                            "encountered field key outside of a struct",
                        ));
                    }
                },
                ParseEvent::SequenceStart => stack.push(Frame::Sequence { items: Vec::new() }),
                ParseEvent::SequenceEnd => {
                    let frame = stack.pop().ok_or(DeserializeError::Structure(
                        "received SequenceEnd without matching SequenceStart",
                    ))?;
                    let value = finish_frame(frame).map_err(DeserializeError::Structure)?;
                    if let Some(root) =
                        push_value(value, &mut stack).map_err(DeserializeError::Structure)?
                    {
                        break root;
                    }
                }
                ParseEvent::Scalar(scalar) => {
                    let value = scalar_to_value(scalar);
                    if let Some(root) =
                        push_value(value, &mut stack).map_err(DeserializeError::Structure)?
                    {
                        break root;
                    }
                }
                ParseEvent::VariantTag(_) => {
                    return Err(DeserializeError::Structure(
                        "variant tags are not supported yet",
                    ));
                }
            }
        };

        Ok(root)
    }
}

/// Error produced by [`FormatDeserializer`].
#[derive(Debug)]
pub enum DeserializeError<E> {
    /// Error emitted by the format-specific parser.
    Parser(E),
    /// Parse events were not well-formed (mismatched delimiters, etc.).
    Structure(&'static str),
    /// Converting the intermediate [`Value`] into the requested type failed.
    Value(ValueError),
}

impl<E: fmt::Display> fmt::Display for DeserializeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::Parser(err) => write!(f, "{err}"),
            DeserializeError::Structure(msg) => write!(f, "{msg}"),
            DeserializeError::Value(err) => write!(f, "{err}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for DeserializeError<E> {}
