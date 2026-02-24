use std::collections::HashMap;

use facet_postcard::{DEFAULT_MAX_COLLECTION_ELEMENTS, from_slice};

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

#[test]
fn oversized_vec_length_is_rejected() {
    let payload = encode_varint(DEFAULT_MAX_COLLECTION_ELEMENTS + 1);
    let err = from_slice::<Vec<()>>(&payload).expect_err("oversized Vec length should fail");
    assert!(
        err.to_string().contains("collection length"),
        "expected collection length error, got: {err}"
    );
}

#[test]
fn oversized_map_length_is_rejected() {
    let payload = encode_varint(DEFAULT_MAX_COLLECTION_ELEMENTS + 1);
    let err = from_slice::<HashMap<String, String>>(&payload)
        .expect_err("oversized map length should fail");
    assert!(
        err.to_string().contains("collection length"),
        "expected collection length error, got: {err}"
    );
}
