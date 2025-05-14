#[cfg(feature = "alloc")]
extern crate alloc;

// All current tests require strings to write the data to
#[cfg(all(feature = "alloc", feature = "deserialize"))]
mod deserialize;
// We deserialize the serialized data as well so we need both feature flags
#[cfg(all(feature = "alloc", feature = "serialize", feature = "deserialize"))]
mod serialize;
