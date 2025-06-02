//! Create and/or write TOML strings from Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod array_of_tables;
mod error;

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};
use core::ops::{Deref, DerefMut};
use owo_colors::OwoColorize;

pub use error::TomlSerError;
use facet_reflect::HasFields;
use facet_serialize::{Serialize, Serializer};
use log::trace;
use toml_edit::{DocumentMut, Item, Table, Value};

/// Serializer for TOML values.
pub struct TomlSerializer<'shape> {
    /// The TOML document.
    document: DocumentMut,
    /// Current stack of where we are in the tree.
    key_stack: KeyStack<'shape>,
    /// What type the current item is.
    current: KeyOrValue,
}

impl<'shape> TomlSerializer<'shape> {
    /// Create a new serialzer.
    pub fn new() -> Self {
        Self {
            document: DocumentMut::new(),
            key_stack: KeyStack::new(),
            current: KeyOrValue::Value,
        }
    }

    /// Get the output serialized TOML document.
    pub fn into_raw_document(self) -> DocumentMut {
        self.document
    }

    /// Get the output serialized TOML string.
    pub fn into_string(self) -> String {
        self.document.to_string()
    }

    /// Write a value depending on the context.
    fn write_value(&mut self, value: impl Into<Value>) -> Result<(), TomlSerError> {
        let value = value.into();

        match self.current {
            // Write the value
            KeyOrValue::Value => {
                self.set_current_item(value);
            }
            // Push the value as a new item
            KeyOrValue::Key => {
                let map_key = value
                    .as_str()
                    .ok_or_else(|| TomlSerError::InvalidKeyConversion {
                        toml_type: value.type_name(),
                    })?
                    .to_string();
                self.push_key(map_key);
                trace!("Push map key {}", self.key_stack);
            }
        }

        Ok(())
    }

    /// Convert the item at the current key to another type.
    fn set_current_item(&mut self, item: impl Into<Item>) {
        let item = item.into();
        trace!("Set item {} to {}", self.key_stack, item.type_name());

        *self.item_mut() = item;
    }

    /// Get the mutable item for the current key.
    fn item_mut(&'_ mut self) -> &'_ mut Item {
        self.key_stack
            .iter()
            .fold(self.document.as_item_mut(), |item, key| {
                item.get_mut(key.as_ref()).unwrap()
            })
    }

    /// Create a new empty item at the key.
    fn push_key(&mut self, key: impl Into<Cow<'shape, str>>) {
        let key = key.into();
        // Push empty item
        self.item_mut()
            .as_table_mut()
            .expect("the current item to be a table when pushing a new key")
            .insert(&key, Item::None);

        // Push the key on the stack
        self.key_stack.push(key);
    }

    /// Pop the current key, which means the item is finished.
    fn pop_key(&mut self) {
        self.key_stack.pop();
    }
}

impl<'shape> Default for TomlSerializer<'shape> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'shape> Serializer<'shape> for TomlSerializer<'shape> {
    type Error = TomlSerError;

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        let toml_number = TryInto::<i64>::try_into(value)
            .map_err(|_| TomlSerError::InvalidNumberToI64Conversion { source_type: "u64" })?;
        self.write_value(toml_number)
    }

    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error> {
        let toml_number = TryInto::<i64>::try_into(value).map_err(|_| {
            TomlSerError::InvalidNumberToI64Conversion {
                source_type: "u128",
            }
        })?;
        self.write_value(toml_number)
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error> {
        let toml_number = TryInto::<i64>::try_into(value).map_err(|_| {
            TomlSerError::InvalidNumberToI64Conversion {
                source_type: "i128",
            }
        })?;
        self.write_value(toml_number)
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        self.write_value(value.to_string())
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_bytes(&mut self, _value: &[u8]) -> Result<(), Self::Error> {
        Err(TomlSerError::UnsupportedByteArray)
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        _variant_name: &'shape str,
    ) -> Result<(), Self::Error> {
        todo!()
    }

    fn start_object(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        let mut table = Table::new();
        // Also show the table when it's empty
        table.set_implicit(false);

        self.set_current_item(table);

        Ok(())
    }

    fn start_array(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.set_current_item(toml_edit::array());

        Ok(())
    }

    fn start_map(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        let mut table = Table::new();
        // Also show the table when it's empty
        table.set_implicit(false);

        self.set_current_item(table);

        Ok(())
    }

    fn serialize_field_name(&mut self, name: &'shape str) -> Result<(), Self::Error> {
        self.push_key(name);
        trace!("Push field {}", self.key_stack);

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
        self.pop_key();
        trace!("Pop map item {}", self.key_stack);

        Ok(())
    }

    fn end_field(&mut self) -> Result<(), Self::Error> {
        self.pop_key();
        trace!("Pop field {}", self.key_stack);

        Ok(())
    }
}

/// What type the current item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyOrValue {
    /// First part of a map item.
    Key,
    /// A regular value, can be a field, array item, etc.
    Value,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct KeyStack<'shape>(Vec<Cow<'shape, str>>);

impl KeyStack<'_> {
    fn new() -> Self {
        Self::default()
    }
}

impl<'shape> Deref for KeyStack<'shape> {
    type Target = Vec<Cow<'shape, str>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'shape> DerefMut for KeyStack<'shape> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::fmt::Display for KeyStack<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut iter = self.iter();
        if let Some(first) = iter.next() {
            write!(f, "[{}", first.cyan())?;
            for key in iter {
                write!(f, ".{}", key.cyan())?;
            }
            write!(f, "]")?;
        } else {
            write!(f, "{}", "root".red())?;
        }
        Ok(())
    }
}

/// Serialize any `Facet` type to a TOML string.
#[cfg(feature = "alloc")]
pub fn to_string<'a, T: facet_core::Facet<'a>>(value: &'a T) -> Result<String, TomlSerError> {
    // First peek at the value to understand its structure
    let peek = facet_reflect::Peek::new(value);

    // Check if the root is a struct with fields that are arrays of tables
    if let Ok(struct_peek) = peek.into_struct() {
        let mut serializer = TomlSerializer::new();

        // Process each field
        for (field, field_value) in struct_peek.fields_for_serialize() {
            // Check if this field is an array of tables
            if array_of_tables::is_array_of_tables(&field_value) {
                // Handle array of tables specially
                let list = field_value.into_list_like().unwrap();
                let aot = array_of_tables::serialize_array_of_tables(list)?;
                serializer
                    .document
                    .insert(field.name, Item::ArrayOfTables(aot));
            } else {
                // Normal field serialization
                serializer.push_key(field.name);
                trace!("Push field {}", field.name);
                facet_serialize::serialize_iterative(field_value, &mut serializer)?;
                serializer.pop_key();
                trace!("Pop field {}", field.name);
            }
        }

        Ok(serializer.into_string())
    } else {
        // Not a struct at root, use normal serialization
        let mut serializer = TomlSerializer::new();
        value.serialize(&mut serializer)?;
        Ok(serializer.into_string())
    }
}
