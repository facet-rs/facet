#![no_main]

use facet::Facet;
use facet_postcard::from_bytes;
use libfuzzer_sys::fuzz_target;

/// Test deserialization against arbitrary bytes
/// This tests that we don't panic or crash on malformed input

#[derive(Debug, Facet)]
struct SimpleStruct {
    a: u32,
    b: String,
    c: bool,
}

#[derive(Debug, Facet)]
struct NestedStruct {
    inner: SimpleStruct,
    value: u64,
}

#[derive(Debug, Facet)]
struct VecStruct {
    values: Vec<u32>,
    name: String,
}

#[derive(Debug, Facet)]
struct OptionStruct {
    opt_u32: Option<u32>,
    opt_string: Option<String>,
}

#[derive(Debug, Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum TestEnum {
    Unit,
    Newtype(u32),
    Tuple(u32, String),
    Struct { x: i32, y: i32 },
}

fuzz_target!(|data: &[u8]| {
    // Try to deserialize as various types - should not panic
    let _ = from_bytes::<SimpleStruct>(data);
    let _ = from_bytes::<NestedStruct>(data);
    let _ = from_bytes::<VecStruct>(data);
    let _ = from_bytes::<OptionStruct>(data);
    let _ = from_bytes::<TestEnum>(data);
    let _ = from_bytes::<u8>(data);
    let _ = from_bytes::<u32>(data);
    let _ = from_bytes::<u64>(data);
    let _ = from_bytes::<i32>(data);
    let _ = from_bytes::<String>(data);
    let _ = from_bytes::<Vec<u8>>(data);
    let _ = from_bytes::<Vec<u32>>(data);
    let _ = from_bytes::<Option<u32>>(data);
    let _ = from_bytes::<(u32, String)>(data);
});
