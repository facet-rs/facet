//! Serde serializer for Styx.

use serde::ser::{self, Serialize};
use styx_format::{FormatOptions, StyxWriter};

use crate::error::{Error, Result};

/// Styx serializer implementing serde::Serializer.
pub struct Serializer {
    writer: StyxWriter,
    at_root: bool,
}

impl Serializer {
    /// Create a new serializer with default options.
    pub fn new() -> Self {
        Self::with_options(FormatOptions::default())
    }

    /// Create a new serializer with the given options.
    pub fn with_options(options: FormatOptions) -> Self {
        Self {
            writer: StyxWriter::with_options(options),
            at_root: true,
        }
    }

    /// Consume the serializer and return the output as a string.
    pub fn finish(self) -> String {
        self.writer.finish_string()
    }
}

impl Default for Serializer {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = SeqSerializer<'a>;
    type SerializeTuple = SeqSerializer<'a>;
    type SerializeTupleStruct = SeqSerializer<'a>;
    type SerializeTupleVariant = SeqSerializer<'a>;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = StructSerializer<'a>;
    type SerializeStructVariant = StructSerializer<'a>;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.at_root = false;
        self.writer.write_bool(v);
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.at_root = false;
        self.writer.write_i64(v);
        Ok(())
    }

    fn serialize_i128(self, v: i128) -> Result<()> {
        self.at_root = false;
        self.writer.write_i128(v);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.at_root = false;
        self.writer.write_u64(v);
        Ok(())
    }

    fn serialize_u128(self, v: u128) -> Result<()> {
        self.at_root = false;
        self.writer.write_u128(v);
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(v as f64)
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.at_root = false;
        self.writer.write_f64(v);
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<()> {
        self.at_root = false;
        self.writer.write_char(v);
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.at_root = false;
        self.writer.write_string(v);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.at_root = false;
        self.writer.write_bytes(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<()> {
        self.at_root = false;
        self.writer.write_null();
        Ok(())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        self.at_root = false;
        self.writer.write_null();
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.at_root = false;
        self.writer.write_variant_tag(variant);
        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        self.at_root = false;
        self.writer.write_variant_tag(variant);
        self.writer.write_byte(b' ');
        value.serialize(&mut *self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.at_root = false;
        self.writer.begin_seq();
        Ok(SeqSerializer { ser: self })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.at_root = false;
        self.writer.write_variant_tag(variant);
        self.writer.begin_seq();
        Ok(SeqSerializer { ser: self })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        let is_root = self.at_root;
        self.at_root = false;
        self.writer.begin_struct(is_root);
        Ok(MapSerializer { ser: self })
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        let is_root = self.at_root;
        self.at_root = false;
        self.writer.begin_struct(is_root);
        Ok(StructSerializer { ser: self })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.at_root = false;
        self.writer.write_variant_tag(variant);
        self.writer.begin_struct(false);
        Ok(StructSerializer { ser: self })
    }
}

/// Serializer for sequences.
pub struct SeqSerializer<'a> {
    ser: &'a mut Serializer,
}

impl<'a> ser::SerializeSeq for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

impl<'a> ser::SerializeTuple for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

impl<'a> ser::SerializeTupleStruct for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

impl<'a> ser::SerializeTupleVariant for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

/// Serializer for maps.
pub struct MapSerializer<'a> {
    ser: &'a mut Serializer,
}

impl<'a> ser::SerializeMap for MapSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        // Keys need special handling - we need to get the string value
        let key_str = KeySerializer::serialize(key)?;
        self.ser.writer.field_key(&key_str).map_err(Error::new)
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_struct().map_err(Error::new)
    }
}

/// Serializer for structs.
pub struct StructSerializer<'a> {
    ser: &'a mut Serializer,
}

impl<'a> ser::SerializeStruct for StructSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.ser.writer.field_key(key).map_err(Error::new)?;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_struct().map_err(Error::new)
    }
}

impl<'a> ser::SerializeStructVariant for StructSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.ser.writer.field_key(key).map_err(Error::new)?;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_struct().map_err(Error::new)
    }
}

/// Helper serializer to extract string keys from map keys.
struct KeySerializer {
    key: String,
}

impl KeySerializer {
    fn serialize<T: ?Sized + Serialize>(value: &T) -> Result<String> {
        let mut serializer = KeySerializer { key: String::new() };
        value.serialize(&mut serializer)?;
        Ok(serializer.key)
    }
}

impl<'a> ser::Serializer for &'a mut KeySerializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = ser::Impossible<(), Error>;
    type SerializeTuple = ser::Impossible<(), Error>;
    type SerializeTupleStruct = ser::Impossible<(), Error>;
    type SerializeTupleVariant = ser::Impossible<(), Error>;
    type SerializeMap = ser::Impossible<(), Error>;
    type SerializeStruct = ser::Impossible<(), Error>;
    type SerializeStructVariant = ser::Impossible<(), Error>;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.key = v.to_string();
        Ok(())
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<()> {
        Err(Error::new("bytes cannot be used as map keys"))
    }

    fn serialize_none(self) -> Result<()> {
        Err(Error::new("None cannot be used as map key"))
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Err(Error::new("unit cannot be used as map key"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Err(Error::new("unit struct cannot be used as map key"))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.key = variant.to_string();
        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        Err(Error::new("newtype variant cannot be used as map key"))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(Error::new("sequence cannot be used as map key"))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Err(Error::new("tuple cannot be used as map key"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(Error::new("tuple struct cannot be used as map key"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::new("tuple variant cannot be used as map key"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::new("map cannot be used as map key"))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Err(Error::new("struct cannot be used as map key"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::new("struct variant cannot be used as map key"))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compact serializer (always uses braces, never unwraps root)
// ─────────────────────────────────────────────────────────────────────────────

/// Compact serializer that always wraps structs in braces.
pub struct CompactSerializer {
    writer: StyxWriter,
}

impl CompactSerializer {
    /// Create a new compact serializer with the given options.
    pub fn with_options(options: FormatOptions) -> Self {
        Self {
            writer: StyxWriter::with_options(options),
        }
    }

    /// Consume the serializer and return the output as a string.
    pub fn finish(self) -> String {
        self.writer.finish_string()
    }
}

impl<'a> ser::Serializer for &'a mut CompactSerializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = CompactSeqSerializer<'a>;
    type SerializeTuple = CompactSeqSerializer<'a>;
    type SerializeTupleStruct = CompactSeqSerializer<'a>;
    type SerializeTupleVariant = CompactSeqSerializer<'a>;
    type SerializeMap = CompactMapSerializer<'a>;
    type SerializeStruct = CompactStructSerializer<'a>;
    type SerializeStructVariant = CompactStructSerializer<'a>;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.writer.write_bool(v);
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.writer.write_i64(v);
        Ok(())
    }

    fn serialize_i128(self, v: i128) -> Result<()> {
        self.writer.write_i128(v);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.writer.write_u64(v);
        Ok(())
    }

    fn serialize_u128(self, v: u128) -> Result<()> {
        self.writer.write_u128(v);
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(v as f64)
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.writer.write_f64(v);
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<()> {
        self.writer.write_char(v);
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.writer.write_string(v);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.writer.write_bytes(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<()> {
        self.writer.write_null();
        Ok(())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        self.writer.write_null();
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.writer.write_variant_tag(variant);
        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        self.writer.write_variant_tag(variant);
        self.writer.write_byte(b' ');
        value.serialize(&mut *self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.writer.begin_seq();
        Ok(CompactSeqSerializer { ser: self })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.writer.write_variant_tag(variant);
        self.writer.begin_seq();
        Ok(CompactSeqSerializer { ser: self })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        self.writer.begin_struct(false); // Never root in compact mode
        Ok(CompactMapSerializer { ser: self })
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        self.writer.begin_struct(false); // Never root in compact mode
        Ok(CompactStructSerializer { ser: self })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.writer.write_variant_tag(variant);
        self.writer.begin_struct(false);
        Ok(CompactStructSerializer { ser: self })
    }
}

/// Compact sequence serializer.
pub struct CompactSeqSerializer<'a> {
    ser: &'a mut CompactSerializer,
}

impl<'a> ser::SerializeSeq for CompactSeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

impl<'a> ser::SerializeTuple for CompactSeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

impl<'a> ser::SerializeTupleStruct for CompactSeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

impl<'a> ser::SerializeTupleVariant for CompactSeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_seq().map_err(Error::new)
    }
}

/// Compact map serializer.
pub struct CompactMapSerializer<'a> {
    ser: &'a mut CompactSerializer,
}

impl<'a> ser::SerializeMap for CompactMapSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        let key_str = KeySerializer::serialize(key)?;
        self.ser.writer.field_key(&key_str).map_err(Error::new)
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_struct().map_err(Error::new)
    }
}

/// Compact struct serializer.
pub struct CompactStructSerializer<'a> {
    ser: &'a mut CompactSerializer,
}

impl<'a> ser::SerializeStruct for CompactStructSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.ser.writer.field_key(key).map_err(Error::new)?;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_struct().map_err(Error::new)
    }
}

impl<'a> ser::SerializeStructVariant for CompactStructSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.ser.writer.field_key(key).map_err(Error::new)?;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<()> {
        self.ser.writer.end_struct().map_err(Error::new)
    }
}
