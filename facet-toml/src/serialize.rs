//! Create and/or write TOML strings from Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

use core::convert::Infallible;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use log::trace;
use toml_edit::{DocumentMut, Item, Table, Value};

use facet_serialize::{Serialize, Serializer};
use yansi::{Paint as _, Painted};

/// Serializer for TOML values.
pub struct TomlSerializer {
    /// The TOML document.
    document: DocumentMut,
    /// Current stack of where we are in the tree.
    key_stack: Vec<Key>,
    /// What type the current item is.
    current: CurrentType,
}

impl TomlSerializer {
    /// Create a new serialzer.
    pub fn new() -> Self {
        Self {
            document: DocumentMut::new(),
            key_stack: Vec::new(),
            current: CurrentType::Regular,
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
    fn write_value(&mut self, value: impl Into<Value>) -> Result<(), Infallible> {
        let value = value.into();

        match self.current {
            CurrentType::Regular | CurrentType::MapValue => {
                // Write the value
                self.set_current_item(value);
            }
            // Push the value as a new item
            CurrentType::MapKey => {
                let map_key = value
                    .clone()
                    .as_str()
                    .expect("Map key cannot be converted to string")
                    .to_string();
                self.push_key(Key::MapValue(map_key), "map key");
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
        let mut item = self.document.as_item_mut();
        for key in &self.key_stack {
            item = &mut item[key.key()];
        }

        item
    }

    /// Create a new empty item at the key.
    #[track_caller]
    fn push_key(&mut self, key: Key, type_name: &'static str) {
        // Push empty item
        self.item_mut()
            .as_table_mut()
            .unwrap()
            .insert(key.key(), Item::None);

        // Push the key on the stack
        self.key_stack.push(key);

        trace!("Push {type_name} {}", self.display_full_key());
    }

    /// Pop the current key, which means the item is finished.
    #[track_caller]
    fn pop_key(&mut self, type_name: &'static str) {
        trace!("Pop {type_name} {}", self.display_full_key());

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
                stack_item.colored_key()
            );
            first = false;
        }
        format!("{output}]")
    }
}

impl Default for TomlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl Serializer for TomlSerializer {
    type Error = Infallible;

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        // TODO: handle casting
        self.write_value(value as i64)
    }

    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error> {
        // TODO: handle casting
        self.write_value(value as i64)
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error> {
        // TODO: handle casting
        self.write_value(value as i64)
    }

    fn serialize_isize(&mut self, value: isize) -> Result<(), Self::Error> {
        self.write_value(value as i64)
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
        todo!()
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        _variant_name: &'static str,
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

    fn serialize_field_name(&mut self, name: &'static str) -> Result<(), Self::Error> {
        self.push_key(Key::Key(name), "field");

        Ok(())
    }

    fn begin_map_key(&mut self) -> Result<(), Self::Error> {
        self.current = CurrentType::MapKey;

        Ok(())
    }

    fn end_map_key(&mut self) -> Result<(), Self::Error> {
        self.current = CurrentType::Regular;

        Ok(())
    }

    fn begin_map_value(&mut self) -> Result<(), Self::Error> {
        self.current = CurrentType::MapValue;

        Ok(())
    }

    fn end_map_value(&mut self) -> Result<(), Self::Error> {
        self.pop_key("map item");

        self.current = CurrentType::Regular;

        Ok(())
    }

    fn end_field(&mut self) -> Result<(), Self::Error> {
        self.pop_key("field");

        Ok(())
    }
}

/// What type the current item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CurrentType {
    /// A regular value, can be a field, array item, etc.
    Regular,
    /// First part of a map item.
    MapKey,
    /// Second part of a map item.
    MapValue,
}

/// Current key part in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Key {
    /// Regular table, used for most operations.
    Key(&'static str),
    /// Map item.
    MapValue(String),
}

impl Key {
    /// Get the key.
    pub fn key(&'_ self) -> &'_ str {
        match self {
            Self::Key(key) => key,
            Self::MapValue(key) => key.as_str(),
        }
    }

    /// Get the with colors.
    pub fn colored_key(&'_ self) -> Painted<&'_ str> {
        match self {
            Self::Key(key) => (*key).blue(),
            Self::MapValue(key) => key.as_str().green(),
        }
    }
}

/// Serialize any `Facet` type to a TOML string.
#[cfg(feature = "alloc")]
pub fn to_string<'a, T: facet_core::Facet<'a>>(value: &'a T) -> String {
    let mut serializer = TomlSerializer::new();
    value.serialize(&mut serializer).unwrap();

    serializer.into_string()
}
