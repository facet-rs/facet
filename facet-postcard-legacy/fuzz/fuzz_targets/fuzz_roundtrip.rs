#![no_main]

use arbitrary::Arbitrary;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use libfuzzer_sys::fuzz_target;

/// A complex struct with various types for thorough fuzzing
#[derive(Debug, Clone, PartialEq, Arbitrary, Facet)]
struct FuzzStruct {
    u8_val: u8,
    u16_val: u16,
    u32_val: u32,
    u64_val: u64,
    i8_val: i8,
    i16_val: i16,
    i32_val: i32,
    i64_val: i64,
    bool_val: bool,
    string_val: String,
    opt_u32: Option<u32>,
    opt_string: Option<String>,
    vec_u8: Vec<u8>,
    vec_u32: Vec<u32>,
}

fuzz_target!(|data: FuzzStruct| {
    // Serialize the arbitrary data
    if let Ok(bytes) = to_vec(&data) {
        // Deserialize it back
        if let Ok(decoded) = from_slice::<FuzzStruct>(&bytes) {
            // Verify roundtrip
            assert_eq!(data, decoded, "Roundtrip mismatch!");
        }
    }
});
