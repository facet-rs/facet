extern crate alloc;

use alloc::vec::Vec;

use facet_format::{FormatSerializer, ScalarValue, SerializeError};

#[derive(Debug)]
pub struct CborSerializeError;

impl core::fmt::Display for CborSerializeError {
    fn fmt(&self, _f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        todo!()
    }
}

impl std::error::Error for CborSerializeError {}

pub struct CborSerializer;

impl CborSerializer {
    pub fn new() -> Self {
        todo!()
    }

    pub fn finish(self) -> Vec<u8> {
        todo!()
    }
}

impl Default for CborSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for CborSerializer {
    type Error = CborSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn field_key(&mut self, _key: &str) -> Result<(), Self::Error> {
        todo!()
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn scalar(&mut self, _scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    fn is_self_describing(&self) -> bool {
        false
    }
}

pub fn to_vec<'facet, T>(_value: &T) -> Result<Vec<u8>, SerializeError<CborSerializeError>>
where
    T: facet_core::Facet<'facet>,
{
    todo!()
}

pub fn to_writer<'facet, T, W>(_writer: &mut W, _value: &T) -> Result<(), std::io::Error>
where
    T: facet_core::Facet<'facet>,
    W: std::io::Write,
{
    todo!()
}
