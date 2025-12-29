//! End-to-end test for Vec<bool> deserialization using Tier-2 JIT.

#![cfg(feature = "jit")]

use facet_postcard::from_slice;

/// Test deserializing an empty Vec<bool>
#[test]
fn test_empty_vec_bool() {
    // Postcard encoding: [0x00] (varint length = 0)
    let input = [0x00u8];
    let result: Vec<bool> = from_slice(&input).expect("should deserialize empty vec");
    assert_eq!(result, vec![]);
}

/// Test deserializing a Vec<bool> with a single element
#[test]
fn test_single_bool_true() {
    // Postcard encoding: [0x01, 0x01] (length=1, value=true)
    let input = [0x01, 0x01];
    let result: Vec<bool> = from_slice(&input).expect("should deserialize [true]");
    assert_eq!(result, vec![true]);
}

#[test]
fn test_single_bool_false() {
    // Postcard encoding: [0x01, 0x00] (length=1, value=false)
    let input = [0x01, 0x00];
    let result: Vec<bool> = from_slice(&input).expect("should deserialize [false]");
    assert_eq!(result, vec![false]);
}

/// Test deserializing a Vec<bool> with multiple elements
#[test]
fn test_multiple_bools() {
    // Postcard encoding: [0x03, 0x01, 0x00, 0x01] (length=3, true, false, true)
    let input = [0x03, 0x01, 0x00, 0x01];
    let result: Vec<bool> = from_slice(&input).expect("should deserialize [true, false, true]");
    assert_eq!(result, vec![true, false, true]);
}

/// Test deserializing a larger Vec<bool>
#[test]
fn test_many_bools() {
    // Create a vec with 10 alternating bools
    let mut input = vec![0x0A]; // length = 10
    for i in 0..10 {
        input.push(if i % 2 == 0 { 1 } else { 0 });
    }

    let result: Vec<bool> = from_slice(&input).expect("should deserialize 10 bools");
    let expected: Vec<bool> = (0..10).map(|i| i % 2 == 0).collect();
    assert_eq!(result, expected);
}

/// Test deserializing a Vec<bool> with length requiring multi-byte varint
#[test]
fn test_vec_bool_large_length() {
    // Length = 128 requires 2 bytes: [0x80, 0x01]
    let mut input = vec![0x80, 0x01]; // varint 128
    for i in 0..128 {
        input.push(if i % 3 == 0 { 1 } else { 0 });
    }

    let result: Vec<bool> = from_slice(&input).expect("should deserialize 128 bools");
    assert_eq!(result.len(), 128);
    for (i, &b) in result.iter().enumerate() {
        assert_eq!(b, i % 3 == 0, "mismatch at index {}", i);
    }
}

/// Test error handling for invalid bool value
#[test]
fn test_invalid_bool_value() {
    // Postcard encoding: [0x01, 0x02] (length=1, invalid bool value 2)
    let input = [0x01, 0x02];
    let result: Result<Vec<bool>, _> = from_slice(&input);
    assert!(result.is_err(), "should fail on invalid bool value");
}

/// Test error handling for truncated input
#[test]
fn test_truncated_input() {
    // Postcard encoding: [0x03, 0x01] (claims 3 elements but only has 1)
    let input = [0x03, 0x01];
    let result: Result<Vec<bool>, _> = from_slice(&input);
    assert!(result.is_err(), "should fail on truncated input");
}

/// Test that postcard encoding matches the reference implementation
#[test]
fn test_matches_reference_postcard() {
    // Use the postcard crate to serialize, then deserialize with our implementation
    let original = vec![true, false, true, false, true];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");

    // Verify our understanding of the format
    assert_eq!(encoded, vec![0x05, 0x01, 0x00, 0x01, 0x00, 0x01]);

    // Deserialize with our implementation
    let result: Vec<bool> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

/// Test round-trip with reference postcard for various sizes
#[test]
fn test_roundtrip_various_sizes() {
    for size in [0, 1, 2, 10, 100, 127, 128, 255, 256] {
        let original: Vec<bool> = (0..size).map(|i| i % 2 == 0).collect();
        let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
        let result: Vec<bool> = from_slice(&encoded).expect("should deserialize");
        assert_eq!(result, original, "mismatch for size {}", size);
    }
}
