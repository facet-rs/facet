//! End-to-end tests for Vec<integer> deserialization using Tier-2 JIT.

use facet_format_postcard::from_slice;

// =============================================================================
// Vec<u8> tests - single bytes, no varint encoding for elements
// =============================================================================

#[test]
fn test_vec_u8_empty() {
    let input = [0x00u8]; // length = 0
    let result: Vec<u8> = from_slice(&input).expect("should deserialize empty vec");
    assert_eq!(result, vec![]);
}

#[test]
fn test_vec_u8_single() {
    // Postcard u8 is raw byte, not varint
    let input = [0x01, 0x42]; // length=1, value=0x42
    let result: Vec<u8> = from_slice(&input).expect("should deserialize [0x42]");
    assert_eq!(result, vec![0x42]);
}

#[test]
fn test_vec_u8_multiple() {
    let input = [0x03, 0x01, 0x02, 0x03]; // length=3
    let result: Vec<u8> = from_slice(&input).expect("should deserialize");
    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn test_vec_u8_roundtrip() {
    let original: Vec<u8> = (0..=255).collect();
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u8> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

// =============================================================================
// Vec<u32> tests - varint encoding
// =============================================================================

#[test]
fn test_vec_u32_empty() {
    let input = [0x00u8];
    let result: Vec<u32> = from_slice(&input).expect("should deserialize empty vec");
    assert_eq!(result, vec![]);
}

#[test]
fn test_vec_u32_small_values() {
    // Small values (< 128) are single byte varints
    let original = vec![0u32, 1, 127];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_u32_multi_byte_varint() {
    // 128 requires 2 bytes, 16384 requires 3 bytes
    let original = vec![128u32, 300, 16384, 65535];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_u32_max_value() {
    let original = vec![u32::MAX];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_u32_roundtrip_various() {
    let original: Vec<u32> = vec![0, 1, 127, 128, 255, 256, 16383, 16384, 1_000_000, u32::MAX];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

// =============================================================================
// Vec<u64> tests - varint encoding
// =============================================================================

#[test]
fn test_vec_u64_empty() {
    let input = [0x00u8];
    let result: Vec<u64> = from_slice(&input).expect("should deserialize empty vec");
    assert_eq!(result, vec![]);
}

#[test]
fn test_vec_u64_small_values() {
    let original = vec![0u64, 1, 127];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u64> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_u64_large_values() {
    let original = vec![u64::MAX, u64::MAX / 2, 1_000_000_000_000u64];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u64> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_u64_roundtrip() {
    let original: Vec<u64> = vec![
        0,
        1,
        127,
        128,
        255,
        256,
        65535,
        65536,
        u32::MAX as u64,
        u64::MAX,
    ];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<u64> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

// =============================================================================
// Vec<i32> tests - ZigZag + varint encoding
// =============================================================================

#[test]
fn test_vec_i32_empty() {
    let input = [0x00u8];
    let result: Vec<i32> = from_slice(&input).expect("should deserialize empty vec");
    assert_eq!(result, vec![]);
}

#[test]
fn test_vec_i32_positive() {
    let original = vec![0i32, 1, 127, 128, 1000];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<i32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_i32_negative() {
    let original = vec![-1i32, -127, -128, -1000];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<i32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_i32_mixed() {
    let original = vec![-100i32, 0, 100, -1, 1, i32::MIN, i32::MAX];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<i32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_i32_zigzag_encoding() {
    // Verify ZigZag encoding: 0->0, -1->1, 1->2, -2->3, 2->4, etc.
    // By checking that small negative numbers are efficiently encoded
    let original = vec![-1i32]; // ZigZag encodes to 1, which is single byte
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    // Length (1) + ZigZag(-1) = varint(1) = [0x01, 0x01]
    assert_eq!(encoded, vec![0x01, 0x01]);

    let result: Vec<i32> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

// =============================================================================
// Vec<i64> tests - ZigZag + varint encoding
// =============================================================================

#[test]
fn test_vec_i64_extremes() {
    let original = vec![i64::MIN, i64::MAX, 0i64];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<i64> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}

#[test]
fn test_vec_i64_mixed() {
    let original = vec![-1i64, 0, 1, -1000, 1000, i64::MIN / 2, i64::MAX / 2];
    let encoded = postcard::to_allocvec(&original).expect("postcard should serialize");
    let result: Vec<i64> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(result, original);
}
