//! Integration tests for MsgPack Tier-2 JIT deserialization.
//!
//! These tests verify that the full end-to-end JIT deserialization works correctly.
//! We use rmp-serde to encode test data and then decode with facet-format-msgpack.

use facet_format_msgpack::from_slice;

// =============================================================================
// Vec<bool> Tests
// =============================================================================

#[test]
fn test_vec_bool_empty() {
    // Empty array: [0x90] (fixarray with 0 elements)
    let bytes: Vec<u8> = rmp_serde::to_vec(&Vec::<bool>::new()).unwrap();
    let result: Vec<bool> = from_slice(&bytes).unwrap();
    assert_eq!(result, Vec::<bool>::new());
}

#[test]
fn test_vec_bool_single() {
    let data = vec![true];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<bool> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_bool_multiple() {
    let data = vec![true, false, true, false, true];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<bool> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_bool_large() {
    // Test with more than 15 elements (requires array16 encoding)
    let data: Vec<bool> = (0..1000).map(|i| i % 3 != 0).collect();
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<bool> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

// =============================================================================
// Vec<u64> Tests
// =============================================================================

#[test]
fn test_vec_u64_empty() {
    let bytes: Vec<u8> = rmp_serde::to_vec(&Vec::<u64>::new()).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, Vec::<u64>::new());
}

#[test]
fn test_vec_u64_fixints() {
    // Values 0-127 encode as positive fixint (single byte)
    let data: Vec<u64> = (0..=127).collect();
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_u64_u8() {
    // Values 128-255 use u8 encoding
    let data: Vec<u64> = vec![128, 200, 255];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_u64_u16() {
    // Values 256-65535 use u16 encoding
    let data: Vec<u64> = vec![256, 1000, 65535];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_u64_u32() {
    // Values 65536-4294967295 use u32 encoding
    let data: Vec<u64> = vec![65536, 1000000, 4294967295];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_u64_u64() {
    // Large values use u64 encoding
    let data: Vec<u64> = vec![4294967296, u64::MAX / 2, u64::MAX];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_u64_mixed() {
    // Mix of all encodings
    let data: Vec<u64> = vec![
        0,          // fixint
        127,        // fixint max
        128,        // u8
        255,        // u8 max
        256,        // u16
        65535,      // u16 max
        65536,      // u32
        4294967295, // u32 max
        4294967296, // u64
        u64::MAX,   // u64 max
    ];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

// =============================================================================
// Vec<i64> Tests
// =============================================================================

#[test]
fn test_vec_i64_empty() {
    let bytes: Vec<u8> = rmp_serde::to_vec(&Vec::<i64>::new()).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, Vec::<i64>::new());
}

#[test]
fn test_vec_i64_positive_fixints() {
    // Values 0-127 encode as positive fixint
    let data: Vec<i64> = (0..=127).collect();
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_i64_negative_fixints() {
    // Values -32 to -1 encode as negative fixint
    let data: Vec<i64> = (-32..=-1).collect();
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_i64_i8() {
    // Values -128 to -33 use i8 encoding
    let data: Vec<i64> = vec![-128, -100, -50, -33];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_i64_i16() {
    let data: Vec<i64> = vec![-32768, -1000, -129];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_i64_i32() {
    let data: Vec<i64> = vec![-2147483648, -100000, -32769];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_i64_i64() {
    let data: Vec<i64> = vec![i64::MIN, -2147483649, i64::MIN / 2];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_vec_i64_mixed() {
    // Mix of positive and negative values
    let data: Vec<i64> = vec![
        0,           // fixint
        127,         // fixint max
        -1,          // negative fixint
        -32,         // negative fixint min
        -128,        // i8
        128,         // u8 (permissive)
        -32768,      // i16
        32767,       // u16 (permissive)
        -2147483648, // i32
        2147483647,  // u32 (permissive)
        i64::MIN,    // i64
        i64::MAX,    // u64 (permissive, fits in i64)
    ];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

// =============================================================================
// Vec<u32> Tests
// =============================================================================

#[test]
fn test_vec_u32_roundtrip() {
    let data: Vec<u32> = vec![0, 127, 128, 255, 256, 65535, 65536, u32::MAX];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u32> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

// =============================================================================
// Vec<i32> Tests
// =============================================================================

#[test]
fn test_vec_i32_roundtrip() {
    let data: Vec<i32> = vec![
        0,
        127,
        -1,
        -32,
        -128,
        128,
        -32768,
        32767,
        i32::MIN,
        i32::MAX,
    ];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<i32> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

// =============================================================================
// Large Array Tests (exercises array16 encoding)
// =============================================================================

#[test]
fn test_large_vec_u64() {
    let data: Vec<u64> = (0..1000).map(|i| i * 12345).collect();
    let bytes = rmp_serde::to_vec(&data).unwrap();
    let result: Vec<u64> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_fixarray_max() {
    // 15 elements is the max for fixarray
    let data: Vec<bool> = vec![true; 15];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    // First byte should be 0x9F (fixarray with 15 elements)
    assert_eq!(bytes[0], 0x9F);
    let result: Vec<bool> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

#[test]
fn test_array16_min() {
    // 16 elements requires array16
    let data: Vec<bool> = vec![true; 16];
    let bytes = rmp_serde::to_vec(&data).unwrap();
    // First byte should be 0xDC (array16)
    assert_eq!(bytes[0], 0xDC);
    let result: Vec<bool> = from_slice(&bytes).unwrap();
    assert_eq!(result, data);
}

// =============================================================================
// Error Cases
// =============================================================================

#[test]
fn test_truncated_input() {
    // Empty input
    let result = from_slice::<Vec<bool>>(&[]);
    assert!(result.is_err());
}

#[test]
fn test_truncated_array() {
    // Array header says 3 elements but only 2 present
    let bytes = &[0x93, 0xC3, 0xC2]; // fixarray(3), true, false
    let result = from_slice::<Vec<bool>>(bytes);
    assert!(result.is_err());
}

#[test]
fn test_wrong_type() {
    // Integer instead of bool
    let bytes = &[0x91, 0x42]; // fixarray(1), 0x42 (fixint 66, not a bool)
    let result = from_slice::<Vec<bool>>(bytes);
    assert!(result.is_err());
}
