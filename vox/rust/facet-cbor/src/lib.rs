#![deny(unsafe_code)]

mod encode;
mod error;
mod serialize;

pub use error::CborError;
pub use serialize::{serialize_peek, to_vec};
