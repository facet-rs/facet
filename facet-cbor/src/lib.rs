extern crate alloc;

mod error;
mod parser;
mod serializer;

#[cfg(feature = "axum")]
mod axum;

pub use error::CborError;

#[cfg(feature = "axum")]
pub use axum::{Cbor, CborRejection, CborSerializeRejection};
pub use parser::CborParser;
pub use serializer::{CborSerializeError, CborSerializer, to_vec, to_writer};

pub use facet_format::DeserializeError;

pub fn from_slice<T>(_input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    todo!()
}

pub fn from_slice_borrowed<'input, 'facet, T>(_input: &'input [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    todo!()
}

pub fn from_slice_into<'facet>(
    _input: &[u8],
    _partial: facet_reflect::Partial<'facet, false>,
) -> Result<facet_reflect::Partial<'facet, false>, DeserializeError> {
    todo!()
}

pub fn from_slice_into_borrowed<'input, 'facet>(
    _input: &'input [u8],
    _partial: facet_reflect::Partial<'facet, true>,
) -> Result<facet_reflect::Partial<'facet, true>, DeserializeError>
where
    'input: 'facet,
{
    todo!()
}
