#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

use facet::{Facet, Opaque, PokeStruct, PokeUninit};
use log::*;

#[cfg(test)]
mod tests;

/// Deserializes a URL encoded form data string into a value of type `T` that implements `Facet`.
///
/// This function supports parsing both flat structures and nested structures using the common
/// bracket notation. For example, a form field like `user[name]` will be deserialized into
/// a struct with a field named `user` that contains a field named `name`.
///
/// # Nested Structure Format
///
/// For nested structures, the library supports the standard bracket notation used in most web frameworks:
/// - Simple nested objects: `object[field]=value`
/// - Deeply nested objects: `object[field1][field2]=value`
///
/// # Basic Example
///
/// ```
/// use facet::Facet;
/// use facet_urlencoded::from_str;
///
/// #[derive(Debug, Facet, PartialEq)]
/// struct SearchParams {
///     query: String,
///     page: u64,
/// }
///
/// let query_string = "query=rust+programming&page=2";
///
/// let params: SearchParams = from_str(query_string).expect("Failed to parse URL encoded data");
/// assert_eq!(params, SearchParams { query: "rust programming".to_string(), page: 2 });
/// ```
///
/// # Nested Structure Example
///
/// ```
/// use facet::Facet;
/// use facet_urlencoded::from_str;
///
/// #[derive(Debug, Facet, PartialEq)]
/// struct Address {
///     street: String,
///     city: String,
/// }
///
/// #[derive(Debug, Facet, PartialEq)]
/// struct User {
///     name: String,
///     address: Address,
/// }
///
/// let query_string = "name=John+Doe&address[street]=123+Main+St&address[city]=Anytown";
///
/// let user: User = from_str(query_string).expect("Failed to parse URL encoded data");
/// assert_eq!(user, User {
///     name: "John Doe".to_string(),
///     address: Address {
///         street: "123 Main St".to_string(),
///         city: "Anytown".to_string(),
///     },
/// });
/// ```
pub fn from_str<T: Facet>(urlencoded: &str) -> Result<T, UrlEncodedError> {
    let (poke, _guard) = PokeUninit::alloc::<T>();
    let opaque = from_str_opaque(poke, urlencoded)?;
    Ok(unsafe { opaque.read::<T>() })
}

/// Deserializes a URL encoded form data string into an `Opaque` value.
///
/// This is the lower-level function that works with `Poke` directly.
fn from_str_opaque<'mem>(
    poke: PokeUninit<'mem>,
    urlencoded: &str,
) -> Result<Opaque<'mem>, UrlEncodedError> {
    trace!("Starting URL encoded form data deserialization");

    // Parse the URL encoded string into key-value pairs
    let pairs = form_urlencoded::parse(urlencoded.as_bytes());

    // Process the input into a nested structure
    let mut nested_values = NestedValues::new();
    for (key, value) in pairs {
        nested_values.insert(&key, value.to_string());
    }

    // Process the deserialization
    deserialize_value(poke, &nested_values)
}

/// Internal helper struct to represent nested values from URL-encoded data
struct NestedValues {
    // Root level key-value pairs
    flat: std::collections::HashMap<String, String>,
    // Nested structures: key -> nested map
    nested: std::collections::HashMap<String, NestedValues>,
}

impl NestedValues {
    fn new() -> Self {
        Self {
            flat: std::collections::HashMap::new(),
            nested: std::collections::HashMap::new(),
        }
    }

    fn insert(&mut self, key: &str, value: String) {
        // For bracket notation like user[name] or user[address][city]
        if let Some(open_bracket) = key.find('[') {
            if let Some(close_bracket) = key.find(']') {
                if open_bracket < close_bracket {
                    let parent_key = &key[0..open_bracket];
                    let nested_key = &key[(open_bracket + 1)..close_bracket];
                    let remainder = &key[(close_bracket + 1)..];

                    let nested = self
                        .nested
                        .entry(parent_key.to_string())
                        .or_insert_with(NestedValues::new);

                    if remainder.is_empty() {
                        // Simple case: user[name]=value
                        nested.flat.insert(nested_key.to_string(), value);
                    } else {
                        // Handle deeply nested case like user[address][city]=value
                        let new_key = format!("{}{}", nested_key, remainder);
                        nested.insert(&new_key, value);
                    }
                    return;
                }
            }
        }

        // If we get here, it's a flat key-value pair
        self.flat.insert(key.to_string(), value);
    }

    fn get(&self, key: &str) -> Option<&String> {
        self.flat.get(key)
    }

    fn get_nested(&self, key: &str) -> Option<&NestedValues> {
        self.nested.get(key)
    }

    fn keys(&self) -> impl Iterator<Item = &String> {
        self.flat.keys()
    }

    fn nested_keys(&self) -> impl Iterator<Item = &String> {
        self.nested.keys()
    }
}

/// Deserialize a value recursively using the nested values
fn deserialize_value<'mem>(
    poke: PokeUninit<'mem>,
    values: &NestedValues,
) -> Result<Opaque<'mem>, UrlEncodedError> {
    match poke {
        PokeUninit::Struct(mut ps) => {
            trace!("Deserializing struct");

            // Process flat fields
            for key in values.keys() {
                if let Ok((index, field_poke)) = ps.field_by_name(key) {
                    let value = values.get(key).unwrap(); // Safe because we're iterating over keys
                    deserialize_scalar_field(key, value, field_poke, index, &mut ps)?;
                } else {
                    warn!("Unknown field: {}", key);
                    // Skip unknown fields
                }
            }

            // Process nested fields
            for key in values.nested_keys() {
                if let Ok((index, field_poke)) = ps.field_by_name(key) {
                    if let Some(nested_values) = values.get_nested(key) {
                        match field_poke {
                            PokeUninit::Struct(_) => {
                                let _nested_opaque = deserialize_value(field_poke, nested_values)?;
                                unsafe {
                                    ps.mark_initialized(index);
                                }
                            }
                            _ => {
                                return Err(UrlEncodedError::UnsupportedShape(format!(
                                    "Expected struct for nested field '{}'",
                                    key
                                )));
                            }
                        }
                    }
                } else {
                    warn!("Unknown nested field: {}", key);
                    // Skip unknown fields
                }
            }

            trace!("Finished deserializing struct");
            Ok(ps.build_in_place())
        }
        _ => {
            error!("Unsupported root type");
            Err(UrlEncodedError::UnsupportedShape(
                "Unsupported root type".to_string(),
            ))
        }
    }
}

/// Helper function to deserialize a scalar field
fn deserialize_scalar_field<'mem>(
    key: &str,
    value: &str,
    field_poke: PokeUninit<'mem>,
    index: usize,
    ps: &mut PokeStruct<'mem>,
) -> Result<(), UrlEncodedError> {
    match field_poke {
        PokeUninit::Scalar(ps_scalar) => {
            if ps_scalar.shape().is_type::<String>() {
                let s = value.to_string();
                ps_scalar.put(s);
            } else if ps_scalar.shape().is_type::<u64>() {
                match value.parse::<u64>() {
                    Ok(num) => {
                        ps_scalar.put(num);
                    }
                    Err(_) => {
                        return Err(UrlEncodedError::InvalidNumber(
                            key.to_string(),
                            value.to_string(),
                        ));
                    }
                }
            } else {
                warn!("Unsupported scalar type: {}", ps_scalar.shape());
                return Err(UrlEncodedError::UnsupportedType(format!(
                    "{}",
                    ps_scalar.shape()
                )));
            }
            unsafe { ps.mark_initialized(index) };
            Ok(())
        }
        _ => {
            error!("Expected scalar field");
            Err(UrlEncodedError::UnsupportedShape(format!(
                "Expected scalar for field '{}'",
                key
            )))
        }
    }
}

/// Errors that can occur during URL encoded form data deserialization.
#[derive(Debug)]
#[non_exhaustive]
pub enum UrlEncodedError {
    /// The field value couldn't be parsed as a number.
    InvalidNumber(String, String),
    /// The shape is not supported for deserialization.
    UnsupportedShape(String),
    /// The type is not supported for deserialization.
    UnsupportedType(String),
}

impl core::fmt::Display for UrlEncodedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
