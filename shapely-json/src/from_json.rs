use crate::parser::{JsonParseErrorKind, JsonParseErrorWithContext, JsonParser};
use shapely::{Partial, error, trace, warn};

pub fn from_json<'input>(
    partial: &mut Partial,
    json: &'input str,
) -> Result<(), JsonParseErrorWithContext<'input>> {
    use shapely::{Innards, Scalar};

    trace!("Starting JSON deserialization");
    let mut parser = JsonParser::new(json);

    fn deserialize_value<'input>(
        parser: &mut JsonParser<'input>,
        partial: &mut Partial,
    ) -> Result<(), JsonParseErrorWithContext<'input>> {
        let shape_desc = partial.shape();
        let shape = shape_desc.get();
        trace!("Deserializing value with shape:\n{:?}", shape);

        match &shape.innards {
            Innards::Scalar(scalar) => {
                let slot = partial.scalar_slot().expect("Scalar slot");
                trace!("Deserializing \x1b[1;36mscalar\x1b[0m, \x1b[1;35m{scalar:?}\x1b[0m");

                match scalar {
                    Scalar::String => slot.fill(parser.parse_string()?),
                    Scalar::U64 => slot.fill(parser.parse_u64()?),
                    // Add other scalar types as needed
                    _ => {
                        warn!("Unsupported scalar type: {:?}", scalar);
                        return Err(parser.make_error(JsonParseErrorKind::Custom(format!(
                            "Unsupported scalar type: {:?}",
                            scalar
                        ))));
                    }
                }
            }
            Innards::Struct { .. } => {
                trace!("Deserializing \x1b[1;36mstruct\x1b[0m");

                let mut first = true;
                while let Some(key) = if first {
                    first = false;
                    parser.expect_object_start()?
                } else {
                    parser.parse_object_key()?
                } {
                    trace!("Processing struct key: \x1b[1;33m{}\x1b[0m", key);
                    let slot = partial
                        .slot_by_name(&key)
                        .map_err(|_| parser.make_error(JsonParseErrorKind::UnknownField(key)))?;
                    let mut partial_field = Partial::alloc(slot.shape());
                    deserialize_value(parser, &mut partial_field)?;
                    slot.fill_from_partial(partial_field);
                }
                trace!("Finished deserializing \x1b[1;36mstruct\x1b[0m");

                // TODO: this would be a good place to decide what to do about unset fields? Is this
                // where we finally get to use `set_default`?
            }
            // Add support for other shapes (Array, Transparent) as needed
            _ => {
                error!("Unsupported shape: {:?}", shape.innards);
                return Err(parser.make_error(JsonParseErrorKind::Custom(format!(
                    "Unsupported shape: {:?}",
                    shape.innards
                ))));
            }
        }
        Ok(())
    }

    deserialize_value(&mut parser, partial)
}
