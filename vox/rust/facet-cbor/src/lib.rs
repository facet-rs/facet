#![deny(unsafe_code)]

mod decode;
mod deserialize;
mod encode;
mod error;
mod serialize;

pub use deserialize::from_slice;
pub use error::CborError;
pub use serialize::{serialize_peek, to_vec};
