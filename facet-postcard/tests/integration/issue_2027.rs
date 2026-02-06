use facet_postcard::from_slice;

#[test]
fn malformed_byte_length_does_not_panic() {
    // Vec<u8> format: varint element count, then that many bytes.
    // Count = u64::MAX encoded as varint (10 bytes), but payload is empty.
    // This used to panic in debug due to usize overflow in bounds checking.
    let malformed = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];

    let result = from_slice::<Vec<u8>>(&malformed);
    assert!(result.is_err(), "malformed input should return an error");
}
