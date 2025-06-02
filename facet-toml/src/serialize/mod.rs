//! Create and/or write TOML strings from Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod array_of_tables;
mod error;

use alloc::{
    borrow::Cow,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::borrow::Borrow as _;
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
    key_stack: Vec<Cow<'shape, str>>,
    /// What type the current item is.
    current: KeyOrValue,
}

impl<'shape> TomlSerializer<'shape> {
    /// Create a new serialzer.
    pub fn new() -> Self {
        Self {
            document: DocumentMut::new(),
            key_stack: Vec::new(),
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
                self.push_key(Cow::Owned(map_key));
                trace!("Push map key {}", self.display_full_key());
            }
        }

        Ok(())
    }

    /// Convert the item at the current key to another type.
    fn set_current_item(&mut self, item: impl Into<Item>) {
        let item = item.into();
        trace!(
            "Set item {} to {}",
            self.display_full_key(),
            item.type_name()
        );

        *self.item_mut() = item;
    }

    /// Get the mutable item for the current key.
    fn item_mut(&'_ mut self) -> &'_ mut Item {
        self.key_stack
            .iter()
            .fold(self.document.as_item_mut(), |item, key| {
                let key: &str = key.borrow();
                item.get_mut(key).unwrap()
            })
    }

    /// Create a new empty item at the key.
    fn push_key(&mut self, key: Cow<'shape, str>) {
        // Push empty item
        self.item_mut()
            .as_table_mut()
            .unwrap()
            .insert(key.borrow(), Item::None);

        // Push the key on the stack
        self.key_stack.push(key);
    }

    /// Pop the current key, which means the item is finished.
    fn pop_key(&mut self) {
        self.key_stack.pop();
    }

    /// Print the keys.
    fn display_full_key(&self) -> String {
        if self.key_stack.is_empty() {
            return "root".red().to_string();
        }

        let mut output = "[".to_string();
        let mut first = true;
        for stack_item in &self.key_stack {
            // Only loop over valid keys
            output = format!(
                "{output}{}{}",
                if first { "" } else { "." },
                stack_item.cyan()
            );
            first = false;
        }
        format!("{output}]")
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
        self.push_key(Cow::Borrowed(name));
        trace!("Push field {}", self.display_full_key());

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
        trace!("Pop map item {}", self.display_full_key());

        Ok(())
    }

    fn end_field(&mut self) -> Result<(), Self::Error> {
        self.pop_key();
        trace!("Pop field {}", self.display_full_key());

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
                serializer.push_key(Cow::Borrowed(field.name));
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
