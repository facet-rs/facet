//! Create and/or write YAML strings from Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;

use core::borrow::Borrow as _;

use alloc::{
    borrow::Cow,
    string::{String, ToString as _},
    vec::Vec,
};

pub use error::YamlSerError;
use facet_serialize::{Serialize, Serializer};
use yaml_rust2::{
    Yaml, YamlEmitter,
    yaml::{Array, Hash},
};

/// Serializer for YAML values.
pub struct YamlSerializer {
    /// Current stack of where we are in the tree.
    key_stack: Vec<Cow<'static, str>>,
    /// YAML document tree.
    yaml: Yaml,
    /// What type the current item is.
    current: KeyOrValue,
}

impl YamlSerializer {
    /// Create a new serialzer.
    pub fn new() -> Self {
        Self {
            key_stack: Vec::new(),
            yaml: Yaml::BadValue,
            current: KeyOrValue::Value,
        }
    }

    /// Get the output serialized YAML document.
    pub fn into_raw_document(self) -> Yaml {
        self.yaml
    }

    /// Get the output serialized YAML string.
    pub fn into_string(self) -> String {
        let mut output = String::new();
        let mut emitter = YamlEmitter::new(&mut output);
        emitter.dump(&self.yaml).unwrap();

        output
    }

    /// Write a value depending on the context.
    fn write_value(&mut self, value: Yaml) -> Result<(), YamlSerError> {
        match self.current {
            // Write the value
            KeyOrValue::Value => {
                if let Some(array) = self.current_mut().as_mut_vec() {
                    // Push it when it's an array, so we don't have to keep track of its keys
                    array.push(value);
                } else if value == Yaml::Null {
                    // Remove the last item if it's none and a hash value
                    self.remove_current();
                } else {
                    // Convert the pushed value to the yaml type
                    self.set_current(value);
                }
            }
            // Push the value as a new item
            KeyOrValue::Key => {
                let yaml_type = type_name(&value);
                let map_key = value
                    .into_string()
                    .ok_or(YamlSerError::InvalidKeyConversion { yaml_type })?;
                self.push_key(map_key.into(), "map key");
            }
        }

        Ok(())
    }

    /// Create a new empty item at the key.
    fn push_key(&mut self, key: Cow<'static, str>, type_name: &'static str) {
        #[cfg(feature = "log")]
        log::trace!("Push {type_name} {}", self.display_full_key());
        #[cfg(not(feature = "log"))]
        let _ = type_name;

        // Push into the map
        self.current_mut()
            .as_mut_hash()
            .unwrap()
            .insert(Yaml::String(key.clone().into_owned()), Yaml::BadValue);

        // Push the key on the stack
        self.key_stack.push(key);
    }

    /// Pop the current key, which means the item is finished.
    fn pop_key(&mut self, type_name: &'static str) -> Option<Cow<'static, str>> {
        #[cfg(feature = "log")]
        log::trace!("Pop {type_name} {}", self.display_full_key());
        #[cfg(not(feature = "log"))]
        let _ = type_name;

        self.key_stack.pop()
    }

    /// Convert the item at the current key to another type.
    fn set_current(&mut self, yaml: Yaml) {
        #[cfg(feature = "log")]
        log::trace!("Set {} to {}", self.display_full_key(), type_name(&yaml));

        *self.current_mut() = yaml;
    }

    /// Remove the last item.
    ///
    /// Item can't be in an array.
    fn remove_current(&mut self) {
        let key = self.key_stack.last().unwrap().to_string();

        // Get the second last item
        let mut item = &mut self.yaml;
        for key in &self.key_stack[0..self.key_stack.len() - 1] {
            item = &mut item[key.borrow()];
        }

        // Remove the current key from it
        item.as_mut_hash().unwrap().remove(&Yaml::String(key));
    }

    /// Get the mutable item for the current key.
    fn current_mut(&'_ mut self) -> &'_ mut Yaml {
        self.key_stack
            .iter()
            .fold(&mut self.yaml, |item, key| &mut item[key.borrow()])
    }

    /// Print the keys.
    #[cfg(feature = "log")]
    fn display_full_key(&self) -> String {
        if self.key_stack.is_empty() {
            return "root".to_string();
        }

        let mut output = "[".to_string();
        let mut first = true;
        for key in &self.key_stack {
            // Only loop over valid keys
            output = format!("{output}{}{}", if first { "" } else { "." }, key);
            first = false;
        }
        format!("{output}]")
    }
}

impl Default for YamlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl Serializer for YamlSerializer {
    type Error = YamlSerError;

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        let yaml_number = TryInto::<i64>::try_into(value)
            .map_err(|_| YamlSerError::InvalidNumberToI64Conversion { source_type: "u64" })?;
        self.write_value(Yaml::Integer(yaml_number))
    }

    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error> {
        let yaml_number = TryInto::<i64>::try_into(value).map_err(|_| {
            YamlSerError::InvalidNumberToI64Conversion {
                source_type: "u128",
            }
        })?;
        self.write_value(Yaml::Integer(yaml_number))
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.write_value(Yaml::Integer(value))
    }

    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error> {
        let yaml_number = TryInto::<i64>::try_into(value).map_err(|_| {
            YamlSerError::InvalidNumberToI64Conversion {
                source_type: "i128",
            }
        })?;
        self.write_value(Yaml::Integer(yaml_number))
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        self.write_value(Yaml::Real(value.to_string()))
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        self.write_value(Yaml::Boolean(value))
    }

    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        self.write_value(Yaml::String(value.to_string()))
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        self.write_value(Yaml::String(value.to_string()))
    }

    fn serialize_bytes(&mut self, _value: &[u8]) -> Result<(), Self::Error> {
        Err(YamlSerError::UnsupportedByteArray)
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.write_value(Yaml::Null)
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        self.write_value(Yaml::Null)
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        _variant_name: &'static str,
    ) -> Result<(), Self::Error> {
        todo!()
    }

    fn start_object(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.set_current(Yaml::Hash(Hash::new()));

        Ok(())
    }

    fn start_array(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.set_current(Yaml::Array(Array::new()));

        Ok(())
    }

    fn start_map(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.set_current(Yaml::Hash(Hash::new()));

        Ok(())
    }

    fn serialize_field_name(&mut self, name: &'static str) -> Result<(), Self::Error> {
        self.push_key(Cow::Borrowed(name), "field");

        Ok(())
    }

    fn begin_map_key(&mut self) -> Result<(), Self::Error> {
        self.current = KeyOrValue::Key;

        Ok(())
    }

    fn end_map_key(&mut self) -> Result<(), Self::Error> {
        self.current = KeyOrValue::Value;

        Ok(())
    }

    fn end_map_value(&mut self) -> Result<(), Self::Error> {
        self.pop_key("map item");

        Ok(())
    }

    fn end_field(&mut self) -> Result<(), Self::Error> {
        self.pop_key("field");

        Ok(())
    }
}

/// What type the current item is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyOrValue {
    /// First part of a map item.
    Key,
    /// A regular value, can be a field, array item, etc.
    Value,
}

/// Serialize any `Facet` type to a YAML string.
#[cfg(feature = "alloc")]
pub fn to_string<'a, T: facet_core::Facet<'a>>(value: &'a T) -> Result<String, YamlSerError> {
    let mut serializer = YamlSerializer::new();
    value.serialize(&mut serializer)?;

    Ok(serializer.into_string())
}

/// Static type name for a YAML type.
fn type_name(yaml: &Yaml) -> &'static str {
    match yaml {
        Yaml::Real(_) => "real",
        Yaml::Integer(_) => "integer",
        Yaml::String(_) => "string",
        Yaml::Boolean(_) => "boolean",
        Yaml::Array(_) => "array",
        Yaml::Hash(_) => "hash",
        Yaml::Alias(_) => "alias",
        Yaml::Null => "null",
        Yaml::BadValue => "bad value",
    }
}
