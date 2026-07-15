//! Registered Rust effect primitives (r[machine.primitive.trait] and family).
mod bridge;
mod convert;
mod descriptor;
mod traits;
pub use bridge::*;
pub use convert::*;
pub use descriptor::*;
pub use traits::*;
