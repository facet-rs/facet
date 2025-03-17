#[doc = include_str!("../README.md")]
use shapely::{Partial, error, trace, warn};

#[cfg(test)]
mod tests;

/// Deserializes URL encoded form data into a Shapely Partial.
///
/// # Example
///
/// ```
/// use shapely::Shapely;
/// use shapely_urlencoded::from_urlencoded;
///
/// #[derive(Debug, Shapely, PartialEq)]
/// struct SearchParams {
///     query: String,
///     page: u64,
/// }
///
/// let query_string = "query=rust+programming&page=2";
///
/// let mut partial = SearchParams::partial();
/// from_urlencoded(&mut partial, query_string).expect("Failed to parse URL encoded data");
///
/// let params = partial.build::<SearchParams>();
/// assert_eq!(params, SearchParams { query: "rust programming".to_string(), page: 2 });
/// ```
pub fn from_urlencoded(partial: &mut Partial, input: &str) -> Result<(), UrlEncodedError> {
    use shapely::{Innards, Scalar};

    trace!("Starting URL encoded form data deserialization");

    // Parse the URL encoded string into key-value pairs
    let pairs = form_urlencoded::parse(input.as_bytes());

    // Create a map to store the parsed key-value pairs
    let mut pairs_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (key, value) in pairs {
        pairs_map.insert(key.to_string(), value.to_string());
    }

    // Process the struct fields
    let shape_desc = partial.shape();
    let shape = shape_desc.get();
    trace!("Deserializing value with shape:\n{:?}", shape);

    match &shape.innards {
        Innards::Struct { .. } => {
            trace!("Deserializing \x1b[1;36mstruct\x1b[0m");

            for (key, value) in pairs_map {
                trace!("Processing field key: \x1b[1;33m{}\x1b[0m", key);

                let slot = match partial.slot_by_name(&key) {
                    Ok(slot) => slot,
                    Err(_) => {
                        warn!("Unknown field: {}", key);
                        continue; // Skip unknown fields
                    }
                };

                let slot_shape = slot.shape();
                let slot_shape_ref = slot_shape.get();

                match &slot_shape_ref.innards {
                    Innards::Scalar(scalar) => {
                        let mut partial_field = Partial::alloc(slot.shape());
                        let field_slot = partial_field.scalar_slot().expect("Scalar slot");

                        match scalar {
                            Scalar::String => {
                                field_slot.fill(value);
                            }
                            Scalar::U64 => match value.parse::<u64>() {
                                Ok(num) => field_slot.fill(num),
                                Err(_) => {
                                    return Err(UrlEncodedError::InvalidNumber(key.clone(), value));
                                }
                            },
                            // Add other scalar types as needed
                            _ => {
                                warn!("Unsupported scalar type: {:?}", scalar);
                                return Err(UrlEncodedError::UnsupportedType(format!(
                                    "{:?}",
                                    scalar
                                )));
                            }
                        }

                        slot.fill_from_partial(partial_field);
                    }
                    // Add support for other shapes (Array, Transparent) as needed
                    _ => {
                        error!("Unsupported shape: {:?}", slot_shape_ref.innards);
                        return Err(UrlEncodedError::UnsupportedShape(format!(
                            "{:?}",
                            slot_shape_ref.innards
                        )));
                    }
                }
            }

            trace!("Finished deserializing \x1b[1;36mstruct\x1b[0m");
        }
        _ => {
            error!("Unsupported shape: {:?}", shape.innards);
            return Err(UrlEncodedError::UnsupportedShape(format!(
                "{:?}",
                shape.innards
            )));
        }
    }

    trace!(
        "Successfully deserialized URL encoded form data for shape: \x1b[1;32m{}\x1b[0m at address \x1b[1;34m{:?}\x1b[0m\n",
        shape,
        partial.addr()
    );
    Ok(())
}

/// Errors that can occur during URL encoded form data deserialization.
#[derive(Debug)]
pub enum UrlEncodedError {
    /// The field value couldn't be parsed as a number.
    InvalidNumber(String, String),
    /// The shape is not supported for deserialization.
    UnsupportedShape(String),
    /// The type is not supported for deserialization.
    UnsupportedType(String),
}

impl std::fmt::Display for UrlEncodedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UrlEncodedError::InvalidNumber(field, value) => {
                write!(f, "Invalid number for field '{}': '{}'", field, value)
            }
            UrlEncodedError::UnsupportedShape(shape) => {
                write!(f, "Unsupported shape: {}", shape)
            }
            UrlEncodedError::UnsupportedType(ty) => {
                write!(f, "Unsupported type: {}", ty)
            }
        }
    }
}

impl std::error::Error for UrlEncodedError {}
