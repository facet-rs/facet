//! Generate golden vectors for Swift encoding tests.
//!
//! Writes binary files to a shared fixtures directory that Swift tests can read.

use roam_wire::{ConnectionId, Hello, Message, MetadataValue};
use std::fs;
use std::path::Path;

fn write_vector(dir: &Path, name: &str, bytes: &[u8]) {
    let path = dir.join(format!("{}.bin", name));
    fs::write(&path, bytes).expect("failed to write golden vector");
    println!("Wrote {} bytes to {}", bytes.len(), path.display());
}

fn main() {
    // Output directory for golden vectors
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures/golden-vectors");

    fs::create_dir_all(&out_dir).expect("failed to create output directory");
    println!("Writing golden vectors to {}\n", out_dir.display());

    // === Wire protocol messages ===
    let wire_dir = out_dir.join("wire");
    fs::create_dir_all(&wire_dir).expect("failed to create wire directory");

    // Hello messages (V2 for spec v2.0.0)
    let hello_small = Hello::V2 {
        max_payload_size: 1024,
        initial_channel_credit: 64,
    };
    write_vector(
        &wire_dir,
        "hello_v2_small",
        &facet_postcard::to_vec(&hello_small).unwrap(),
    );

    let hello_typical = Hello::V2 {
        max_payload_size: 1024 * 1024,
        initial_channel_credit: 64 * 1024,
    };
    write_vector(
        &wire_dir,
        "hello_v2_typical",
        &facet_postcard::to_vec(&hello_typical).unwrap(),
    );

    // Message::Hello
    let msg = Message::Hello(hello_small.clone());
    write_vector(
        &wire_dir,
        "message_hello_small",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    let msg = Message::Hello(hello_typical.clone());
    write_vector(
        &wire_dir,
        "message_hello_typical",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Connect/Accept/Reject (new in v2)
    let msg = Message::Connect {
        request_id: 1,
        metadata: vec![],
    };
    write_vector(
        &wire_dir,
        "message_connect_empty",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    let msg = Message::Connect {
        request_id: 2,
        metadata: vec![(
            "auth".to_string(),
            MetadataValue::String("token123".to_string()),
        )],
    };
    write_vector(
        &wire_dir,
        "message_connect_with_metadata",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    let msg = Message::Accept {
        request_id: 1,
        conn_id: ConnectionId::new(1),
        metadata: vec![],
    };
    write_vector(
        &wire_dir,
        "message_accept",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    let msg = Message::Reject {
        request_id: 1,
        reason: "not listening".to_string(),
        metadata: vec![],
    };
    write_vector(
        &wire_dir,
        "message_reject",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Goodbye (now has conn_id)
    let msg = Message::Goodbye {
        conn_id: ConnectionId::ROOT,
        reason: "test".to_string(),
    };
    write_vector(
        &wire_dir,
        "message_goodbye_conn0",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    let msg = Message::Goodbye {
        conn_id: ConnectionId::new(1),
        reason: "done".to_string(),
    };
    write_vector(
        &wire_dir,
        "message_goodbye_conn1",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Request variants (now has conn_id)
    let msg = Message::Request {
        conn_id: ConnectionId::ROOT,
        request_id: 1,
        method_id: 42,
        metadata: vec![],
        channels: vec![],
        payload: vec![],
    };
    write_vector(
        &wire_dir,
        "message_request_empty",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    let msg = Message::Request {
        conn_id: ConnectionId::ROOT,
        request_id: 1,
        method_id: 42,
        metadata: vec![],
        channels: vec![],
        payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };
    write_vector(
        &wire_dir,
        "message_request_with_payload",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    let msg = Message::Request {
        conn_id: ConnectionId::ROOT,
        request_id: 5,
        method_id: 100,
        metadata: vec![(
            "key".to_string(),
            MetadataValue::String("value".to_string()),
        )],
        channels: vec![],
        payload: vec![],
    };
    write_vector(
        &wire_dir,
        "message_request_with_metadata",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Request with channels (for proxy support)
    let msg = Message::Request {
        conn_id: ConnectionId::ROOT,
        request_id: 6,
        method_id: 200,
        metadata: vec![],
        channels: vec![1, 3],
        payload: vec![0x42],
    };
    write_vector(
        &wire_dir,
        "message_request_with_channels",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Request on virtual connection
    let msg = Message::Request {
        conn_id: ConnectionId::new(1),
        request_id: 1,
        method_id: 42,
        metadata: vec![],
        channels: vec![],
        payload: vec![0x42],
    };
    write_vector(
        &wire_dir,
        "message_request_conn1",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Response (now has conn_id)
    let msg = Message::Response {
        conn_id: ConnectionId::ROOT,
        request_id: 1,
        metadata: vec![],
        channels: vec![],
        payload: vec![0x42],
    };
    write_vector(
        &wire_dir,
        "message_response",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Cancel (now has conn_id)
    let msg = Message::Cancel {
        conn_id: ConnectionId::ROOT,
        request_id: 99,
    };
    write_vector(
        &wire_dir,
        "message_cancel",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Data (now has conn_id)
    let msg = Message::Data {
        conn_id: ConnectionId::ROOT,
        channel_id: 1,
        payload: vec![1, 2, 3],
    };
    write_vector(
        &wire_dir,
        "message_data",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Close (now has conn_id)
    let msg = Message::Close {
        conn_id: ConnectionId::ROOT,
        channel_id: 7,
    };
    write_vector(
        &wire_dir,
        "message_close",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Reset (now has conn_id)
    let msg = Message::Reset {
        conn_id: ConnectionId::ROOT,
        channel_id: 5,
    };
    write_vector(
        &wire_dir,
        "message_reset",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // Credit (now has conn_id)
    let msg = Message::Credit {
        conn_id: ConnectionId::ROOT,
        channel_id: 3,
        bytes: 4096,
    };
    write_vector(
        &wire_dir,
        "message_credit",
        &facet_postcard::to_vec(&msg).unwrap(),
    );

    // === Varints ===
    let varint_dir = out_dir.join("varint");
    fs::create_dir_all(&varint_dir).expect("failed to create varint directory");

    for val in [
        0u64, 1, 127, 128, 255, 256, 16383, 16384, 65535, 65536, 1048576,
    ] {
        let bytes = facet_postcard::to_vec(&val).unwrap();
        write_vector(&varint_dir, &format!("u64_{}", val), &bytes);
    }

    // === Primitives - comprehensive coverage ===
    let prim_dir = out_dir.join("primitives");
    fs::create_dir_all(&prim_dir).expect("failed to create primitives directory");

    // bool
    write_vector(
        &prim_dir,
        "bool_false",
        &facet_postcard::to_vec(&false).unwrap(),
    );
    write_vector(
        &prim_dir,
        "bool_true",
        &facet_postcard::to_vec(&true).unwrap(),
    );

    // u8
    write_vector(&prim_dir, "u8_0", &facet_postcard::to_vec(&0u8).unwrap());
    write_vector(
        &prim_dir,
        "u8_127",
        &facet_postcard::to_vec(&127u8).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u8_255",
        &facet_postcard::to_vec(&255u8).unwrap(),
    );

    // i8
    write_vector(&prim_dir, "i8_0", &facet_postcard::to_vec(&0i8).unwrap());
    write_vector(
        &prim_dir,
        "i8_neg1",
        &facet_postcard::to_vec(&(-1i8)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i8_127",
        &facet_postcard::to_vec(&127i8).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i8_neg128",
        &facet_postcard::to_vec(&(-128i8)).unwrap(),
    );

    // u16
    write_vector(&prim_dir, "u16_0", &facet_postcard::to_vec(&0u16).unwrap());
    write_vector(
        &prim_dir,
        "u16_127",
        &facet_postcard::to_vec(&127u16).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u16_128",
        &facet_postcard::to_vec(&128u16).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u16_255",
        &facet_postcard::to_vec(&255u16).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u16_256",
        &facet_postcard::to_vec(&256u16).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u16_max",
        &facet_postcard::to_vec(&u16::MAX).unwrap(),
    );

    // i16
    write_vector(&prim_dir, "i16_0", &facet_postcard::to_vec(&0i16).unwrap());
    write_vector(&prim_dir, "i16_1", &facet_postcard::to_vec(&1i16).unwrap());
    write_vector(
        &prim_dir,
        "i16_neg1",
        &facet_postcard::to_vec(&(-1i16)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i16_127",
        &facet_postcard::to_vec(&127i16).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i16_128",
        &facet_postcard::to_vec(&128i16).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i16_max",
        &facet_postcard::to_vec(&i16::MAX).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i16_min",
        &facet_postcard::to_vec(&i16::MIN).unwrap(),
    );

    // u32
    write_vector(&prim_dir, "u32_0", &facet_postcard::to_vec(&0u32).unwrap());
    write_vector(&prim_dir, "u32_1", &facet_postcard::to_vec(&1u32).unwrap());
    write_vector(
        &prim_dir,
        "u32_127",
        &facet_postcard::to_vec(&127u32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u32_128",
        &facet_postcard::to_vec(&128u32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u32_255",
        &facet_postcard::to_vec(&255u32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u32_256",
        &facet_postcard::to_vec(&256u32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u32_max",
        &facet_postcard::to_vec(&u32::MAX).unwrap(),
    );

    // i32
    write_vector(&prim_dir, "i32_0", &facet_postcard::to_vec(&0i32).unwrap());
    write_vector(&prim_dir, "i32_1", &facet_postcard::to_vec(&1i32).unwrap());
    write_vector(
        &prim_dir,
        "i32_neg1",
        &facet_postcard::to_vec(&(-1i32)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i32_127",
        &facet_postcard::to_vec(&127i32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i32_128",
        &facet_postcard::to_vec(&128i32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i32_neg128",
        &facet_postcard::to_vec(&(-128i32)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i32_max",
        &facet_postcard::to_vec(&i32::MAX).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i32_min",
        &facet_postcard::to_vec(&i32::MIN).unwrap(),
    );

    // u64
    write_vector(&prim_dir, "u64_0", &facet_postcard::to_vec(&0u64).unwrap());
    write_vector(&prim_dir, "u64_1", &facet_postcard::to_vec(&1u64).unwrap());
    write_vector(
        &prim_dir,
        "u64_127",
        &facet_postcard::to_vec(&127u64).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u64_128",
        &facet_postcard::to_vec(&128u64).unwrap(),
    );
    write_vector(
        &prim_dir,
        "u64_max",
        &facet_postcard::to_vec(&u64::MAX).unwrap(),
    );

    // i64
    write_vector(&prim_dir, "i64_0", &facet_postcard::to_vec(&0i64).unwrap());
    write_vector(&prim_dir, "i64_1", &facet_postcard::to_vec(&1i64).unwrap());
    write_vector(
        &prim_dir,
        "i64_neg1",
        &facet_postcard::to_vec(&(-1i64)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i64_15",
        &facet_postcard::to_vec(&15i64).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i64_42",
        &facet_postcard::to_vec(&42i64).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i64_max",
        &facet_postcard::to_vec(&i64::MAX).unwrap(),
    );
    write_vector(
        &prim_dir,
        "i64_min",
        &facet_postcard::to_vec(&i64::MIN).unwrap(),
    );

    // f32 - use exactly representable values only
    write_vector(
        &prim_dir,
        "f32_0",
        &facet_postcard::to_vec(&0.0f32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f32_1",
        &facet_postcard::to_vec(&1.0f32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f32_neg1",
        &facet_postcard::to_vec(&(-1.0f32)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f32_1_5",
        &facet_postcard::to_vec(&1.5f32).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f32_0_25",
        &facet_postcard::to_vec(&0.25f32).unwrap(),
    );

    // f64 - use exactly representable values only
    write_vector(
        &prim_dir,
        "f64_0",
        &facet_postcard::to_vec(&0.0f64).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f64_1",
        &facet_postcard::to_vec(&1.0f64).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f64_neg1",
        &facet_postcard::to_vec(&(-1.0f64)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f64_1_5",
        &facet_postcard::to_vec(&1.5f64).unwrap(),
    );
    write_vector(
        &prim_dir,
        "f64_0_25",
        &facet_postcard::to_vec(&0.25f64).unwrap(),
    );

    // String
    write_vector(
        &prim_dir,
        "string_empty",
        &facet_postcard::to_vec(&"").unwrap(),
    );
    write_vector(
        &prim_dir,
        "string_hello",
        &facet_postcard::to_vec(&"hello world").unwrap(),
    );
    write_vector(
        &prim_dir,
        "string_unicode",
        &facet_postcard::to_vec(&"héllo 世界 🦀").unwrap(),
    );

    // bytes (Vec<u8>)
    write_vector(
        &prim_dir,
        "bytes_empty",
        &facet_postcard::to_vec(&Vec::<u8>::new()).unwrap(),
    );
    write_vector(
        &prim_dir,
        "bytes_deadbeef",
        &facet_postcard::to_vec(&vec![0xDEu8, 0xAD, 0xBE, 0xEF]).unwrap(),
    );

    // Option
    write_vector(
        &prim_dir,
        "option_none_u32",
        &facet_postcard::to_vec(&None::<u32>).unwrap(),
    );
    write_vector(
        &prim_dir,
        "option_some_u32_42",
        &facet_postcard::to_vec(&Some(42u32)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "option_none_string",
        &facet_postcard::to_vec(&None::<String>).unwrap(),
    );
    write_vector(
        &prim_dir,
        "option_some_string",
        &facet_postcard::to_vec(&Some("hello".to_string())).unwrap(),
    );

    // Vec
    write_vector(
        &prim_dir,
        "vec_empty_u32",
        &facet_postcard::to_vec(&Vec::<u32>::new()).unwrap(),
    );
    write_vector(
        &prim_dir,
        "vec_u32_1_2_3",
        &facet_postcard::to_vec(&vec![1u32, 2, 3]).unwrap(),
    );
    write_vector(
        &prim_dir,
        "vec_i32_neg1_0_1",
        &facet_postcard::to_vec(&vec![-1i32, 0, 1]).unwrap(),
    );
    write_vector(
        &prim_dir,
        "vec_string",
        &facet_postcard::to_vec(&vec!["a".to_string(), "b".to_string()]).unwrap(),
    );

    // Result
    write_vector(
        &prim_dir,
        "result_ok_string",
        &facet_postcard::to_vec(&Ok::<String, String>("hello".to_string())).unwrap(),
    );
    write_vector(
        &prim_dir,
        "result_ok_i64_42",
        &facet_postcard::to_vec(&Ok::<i64, String>(42)).unwrap(),
    );
    write_vector(
        &prim_dir,
        "result_err_string",
        &facet_postcard::to_vec(&Err::<String, String>("error message".to_string())).unwrap(),
    );

    // Tuples
    write_vector(
        &prim_dir,
        "tuple_i32_string",
        &facet_postcard::to_vec(&(42i32, "hello".to_string())).unwrap(),
    );
    write_vector(
        &prim_dir,
        "tuple_string_i32",
        &facet_postcard::to_vec(&("hello".to_string(), 42i32)).unwrap(),
    );

    // === COBS framing ===
    // These test that COBS encoding/decoding matches the Rust cobs crate
    let cobs_dir = out_dir.join("cobs");
    fs::create_dir_all(&cobs_dir).expect("failed to create cobs directory");

    // Empty data
    let data: Vec<u8> = vec![];
    let encoded = cobs::encode_vec(&data);
    write_vector(&cobs_dir, "empty_encoded", &encoded);
    write_vector(&cobs_dir, "empty_raw", &data);

    // Simple data with no zeros
    let data: Vec<u8> = vec![1, 2, 3, 4, 5];
    let encoded = cobs::encode_vec(&data);
    write_vector(&cobs_dir, "no_zeros_encoded", &encoded);
    write_vector(&cobs_dir, "no_zeros_raw", &data);

    // Data with zeros
    let data: Vec<u8> = vec![0];
    let encoded = cobs::encode_vec(&data);
    write_vector(&cobs_dir, "single_zero_encoded", &encoded);
    write_vector(&cobs_dir, "single_zero_raw", &data);

    let data: Vec<u8> = vec![0, 0];
    let encoded = cobs::encode_vec(&data);
    write_vector(&cobs_dir, "two_zeros_encoded", &encoded);
    write_vector(&cobs_dir, "two_zeros_raw", &data);

    let data: Vec<u8> = vec![1, 0, 2];
    let encoded = cobs::encode_vec(&data);
    write_vector(&cobs_dir, "one_zero_middle_encoded", &encoded);
    write_vector(&cobs_dir, "one_zero_middle_raw", &data);

    let data: Vec<u8> = vec![0, 1, 2, 0, 3, 4, 0];
    let encoded = cobs::encode_vec(&data);
    write_vector(&cobs_dir, "multiple_zeros_encoded", &encoded);
    write_vector(&cobs_dir, "multiple_zeros_raw", &data);

    // Hello message (v2)
    let hello = Message::Hello(Hello::V2 {
        max_payload_size: 1024 * 1024,
        initial_channel_credit: 64 * 1024,
    });
    let payload = facet_postcard::to_vec(&hello).unwrap();
    let encoded = cobs::encode_vec(&payload);
    write_vector(&cobs_dir, "message_hello_typical_raw", &payload);
    write_vector(&cobs_dir, "message_hello_typical_encoded", &encoded);

    // Full frame (COBS + delimiter)
    let mut framed = encoded.clone();
    framed.push(0x00);
    write_vector(&cobs_dir, "message_hello_typical_framed", &framed);

    println!(
        "\nDone! Generated {} golden vector files.",
        fs::read_dir(&wire_dir).unwrap().count()
            + fs::read_dir(&varint_dir).unwrap().count()
            + fs::read_dir(&prim_dir).unwrap().count()
            + fs::read_dir(&cobs_dir).unwrap().count()
    );
}
