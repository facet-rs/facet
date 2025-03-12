use crate::parser::{JsonParseErrorKind, JsonParseErrorWithContext, JsonParser};
use shapely::{error, trace, warn, Partial};

pub fn from_json<'input>(
    partial: &mut Partial,
    json: &'input str,
) -> Result<(), JsonParseErrorWithContext<'input>> {
    use shapely::{Innards, Scalar};

    trace!("Starting JSON deserialization");
    let mut parser = JsonParser::new(json);

    // Define our state machine states
    enum DeserializeState<'a> {
        Value { partial: Partial<'a> },
        Struct { partial: Partial<'a>, first: bool },
    }

    // Create our stack and initialize with the root value
    let mut stack = Vec::new();
    stack.push(DeserializeState::Value {
        partial: std::mem::replace(partial, Partial::alloc(partial.shape())),
    });

    while let Some(state) = stack.last_mut() {
        match state {
            DeserializeState::Value { .. } => {
                let mut partial = match stack.pop().unwrap() {
                    DeserializeState::Value { partial } => partial,
                    _ => unreachable!(),
                };
                let shape_desc = partial.shape();
                let shape = shape_desc.get();
                trace!("Deserializing value with shape:\n{shape:?}");

                match &shape.innards {
                    Innards::Scalar(scalar) => {
                        let slot = partial.scalar_slot().expect("Scalar slot");
                        trace!(
                            "Deserializing \x1b[1;36mscalar\x1b[0m, \x1b[1;35m{scalar:?}\x1b[0m"
                        );

                        match scalar {
                            Scalar::String => slot.fill(parser.parse_string()?),
                            Scalar::U64 => slot.fill(parser.parse_u64()?),
                            // Add other scalar types as needed
                            _ => {
                                warn!("Unsupported scalar type: {scalar:?}");
                                return Err(parser.make_error(JsonParseErrorKind::Custom(
                                    format!("Unsupported scalar type: {scalar:?}"),
                                )));
                            }
                        }
                    }
                    Innards::Struct { .. } => {
                        trace!("Deserializing \x1b[1;36mstruct\x1b[0m");
                        // Push the struct state on the stack to continue after we process the fields
                        stack.push(DeserializeState::Struct {
                            partial,
                            first: true,
                        });
                    }
                    // Add support for other shapes (Array, Transparent) as needed
                    _ => {
                        error!("Unsupported shape: {shape}");
                        return Err(parser.make_error(JsonParseErrorKind::Custom(format!(
                            "Unsupported shape: {:?}",
                            shape.innards
                        ))));
                    }
                }
            }
            DeserializeState::Struct { partial, first } => {
                let key = if *first {
                    parser.expect_object_start()?
                } else {
                    parser.parse_object_key()?
                };

                if let Some(key) = key {
                    trace!("Processing struct key: \x1b[1;33m{key}\x1b[0m");
                    let slot = match partial.slot_by_name(&key) {
                        Ok(slot) => slot,
                        Err(_) => {
                            return Err(parser.make_error(JsonParseErrorKind::UnknownField(key)));
                        }
                    };
                    let partial_field = slot.into_partial();
                    stack.push(DeserializeState::Value {
                        partial: partial_field,
                    });
                } else {
                    // No more fields, we're done with this struct
                    trace!("Finished deserializing \x1b[1;36mstruct\x1b[0m");

                    // TODO: this would be a good place to decide what to do about unset fields? Is this
                    // where we finally get to use `set_default`?
                }
            }
        }
    }

    Ok(())
}
