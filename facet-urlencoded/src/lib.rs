#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

use facet_core::{Def, Facet, Type, UserType};
use facet_reflect::{HeapValue, Wip};
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
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(
    urlencoded: &'input str,
) -> Result<T, UrlEncodedError<'facet>> {
    let val = from_str_value(Wip::alloc::<T>()?, urlencoded)?;
    Ok(val.materialize::<T>()?)
}

/// Deserializes a URL encoded form data string into an heap-allocated value.
///
/// This is the lower-level function that works with `Wip` directly.
fn from_str_value<'mem, 'shape>(
    wip: Wip<'mem, 'shape>,
    urlencoded: &str,
) -> Result<HeapValue<'mem, 'shape>, UrlEncodedError<'shape>> {
    trace!("Starting URL encoded form data deserialization");

    // Parse the URL encoded string into key-value pairs
    let pairs = form_urlencoded::parse(urlencoded.as_bytes());

    // Process the input into a nested structure
    let mut nested_values = NestedValues::new();
    for (key, value) in pairs {
        nested_values.insert(&key, value.to_string());
    }

    // Create pre-initialized structure so that we have all the required fields
    // for better error reporting when fields are missing
    initialize_nested_structures(&mut nested_values);

    // Process the deserialization
    deserialize_value(wip, &nested_values)
}

/// Ensures that all nested structures have entries in the NestedValues
/// This helps ensure we get better error reporting when fields are missing
fn initialize_nested_structures(nested: &mut NestedValues) {
    // Go through each nested value and recursively initialize it
    for nested_value in nested.nested.values_mut() {
        initialize_nested_structures(nested_value);
    }
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

    #[expect(dead_code)]
    fn get_nested(&self, key: &str) -> Option<&NestedValues> {
        self.nested.get(key)
    }

    fn keys(&self) -> impl Iterator<Item = &String> {
        self.flat.keys()
    }

    #[expect(dead_code)]
    fn nested_keys(&self) -> impl Iterator<Item = &String> {
        self.nested.keys()
    }
}

/// Deserialize a value recursively using the nested values
fn deserialize_value<'mem, 'shape>(
    wip: Wip<'mem, 'shape>,
    values: &NestedValues,
) -> Result<HeapValue<'mem, 'shape>, UrlEncodedError<'shape>> {
    let shape = wip.shape();
    match shape.ty {
        Type::User(UserType::Struct(_)) => {
            trace!("Deserializing struct");

            let mut wip = wip;

            // Process flat fields
            for key in values.keys() {
                if let Some(index) = wip.field_index(key) {
                    let value = values.get(key).unwrap(); // Safe because we're iterating over keys
                    let field = wip.field(index)?;
                    wip = deserialize_scalar_field(key, value, field)?;
                } else {
                    trace!("Unknown field: {}", key);
                }
            }

            // Process nested fields
            for key in values.nested.keys() {
                if let Some(index) = wip.field_index(key) {
                    let nested_values = values.nested.get(key).unwrap(); // Safe because we're iterating over keys
                    let field = wip.field(index)?;
                    wip = deserialize_nested_field(key, nested_values, field)?;
                } else {
                    trace!("Unknown nested field: {}", key);
                }
            }

            trace!("Finished deserializing struct");
            Ok(wip.build()?)
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
fn deserialize_scalar_field<'mem, 'shape>(
    key: &str,
    value: &str,
    wip: Wip<'mem, 'shape>,
) -> Result<Wip<'mem, 'shape>, UrlEncodedError<'shape>> {
    match wip.shape().def {
        Def::Scalar(_sd) => {
            let wip = if wip.shape().is_type::<String>() {
                let s = value.to_string();
                wip.put(s)?
            } else if wip.shape().is_type::<u64>() {
                match value.parse::<u64>() {
                    Ok(num) => wip.put(num)?,
                    Err(_) => {
                        return Err(UrlEncodedError::InvalidNumber(
                            key.to_string(),
                            value.to_string(),
                        ));
                    }
                }
            } else {
                warn!("facet-yaml: unsupported scalar type: {}", wip.shape());
                return Err(UrlEncodedError::UnsupportedType(format!("{}", wip.shape())));
            };
            Ok(wip.pop()?)
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

/// Helper function to deserialize a nested field
fn deserialize_nested_field<'mem, 'shape>(
    key: &str,
    nested_values: &NestedValues,
    wip: Wip<'mem, 'shape>,
) -> Result<Wip<'mem, 'shape>, UrlEncodedError<'shape>> {
    let shape = wip.shape();
    match shape.ty {
        Type::User(UserType::Struct(_)) => {
            trace!("Deserializing nested struct field: {}", key);

            let mut current_wip = wip;

            // Process flat fields in the nested structure
            for nested_key in nested_values.keys() {
                if let Some(index) = current_wip.field_index(nested_key) {
                    let value = nested_values.get(nested_key).unwrap(); // Safe because we're iterating over keys
                    let field_wip = current_wip.field(index)?;
                    current_wip = deserialize_scalar_field(nested_key, value, field_wip)?
                }
            }

            // Process deeper nested fields
            for nested_key in nested_values.nested.keys() {
                if let Some(index) = current_wip.field_index(nested_key) {
                    let deeper_nested = nested_values.nested.get(nested_key).unwrap(); // Safe because we're iterating over keys
                    let field_wip = current_wip.field(index)?;
                    current_wip = deserialize_nested_field(nested_key, deeper_nested, field_wip)?;
                }
            }

            // Return to parent level
            Ok(current_wip.pop()?)
        }
        _ => {
            error!("Expected struct field for nested value");
            Err(UrlEncodedError::UnsupportedShape(format!(
                "Expected struct for nested field '{}'",
                key
            )))
        }
    }
}

/// Errors that can occur during URL encoded form data deserialization.
#[derive(Debug)]
#[non_exhaustive]
pub enum UrlEncodedError<'shape> {
    /// The field value couldn't be parsed as a number.
    InvalidNumber(String, String),
    /// The shape is not supported for deserialization.
    UnsupportedShape(String),
    /// The type is not supported for deserialization.
    UnsupportedType(String),
    /// Reflection error
    ReflectError(facet_reflect::ReflectError<'shape>),
}

impl<'shape> From<facet_reflect::ReflectError<'shape>> for UrlEncodedError<'shape> {
    fn from(err: facet_reflect::ReflectError<'shape>) -> Self {
        UrlEncodedError::ReflectError(err)
    }
}

impl<'shape> core::fmt::Display for UrlEncodedError<'shape> {
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
            UrlEncodedError::ReflectError(err) => {
                write!(f, "Reflection error: {}", err)
            }
        }
    }
}

impl<'shape> std::error::Error for UrlEncodedError<'shape> {}
