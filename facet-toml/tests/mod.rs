#[cfg(feature = "alloc")]
extern crate alloc;

// All current tests require strings to write the data to
#[cfg(feature = "alloc")]
mod deserialize;
// We deserialize the serialized data as well so we need both feature flags
#[cfg(feature = "alloc")]
mod serialize;
