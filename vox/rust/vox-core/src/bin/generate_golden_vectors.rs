//! Generate all golden vectors used by TypeScript and Swift tests.

use std::{fs, path::PathBuf};

use vox_types::{
    ChannelBody, ChannelClose, ChannelGrantCredit, ChannelId, ChannelItem, ChannelMessage,
    ChannelReset, ConnectionAccept, ConnectionClose, ConnectionId, ConnectionOpen,
    ConnectionReject, ConnectionSettings, Message, MessagePayload, Metadata, MetadataEntry,
    MetadataFlags, MetadataValue, MethodId, Parity, Payload, ProtocolError, RequestBody,
    RequestCall, RequestCancel, RequestId, RequestMessage, RequestResponse, VoxError,
};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("test-fixtures")
        .join("golden-vectors")
}

fn write_fixture(path: &str, bytes: &[u8]) {
    let out_path = fixture_root().join(path);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).expect("failed to create fixture directory");
    }
    fs::write(&out_path, bytes).expect("failed to write fixture");
    eprintln!("wrote {} ({} bytes)", out_path.display(), bytes.len());
}

fn encode_message(message: &Message<'_>) -> Vec<u8> {
    vox_postcard::to_vec(message).expect("serialize message fixture")
}

fn sample_metadata() -> Metadata<'static> {
    vec![
        MetadataEntry {
            key: "trace-id".into(),
            value: MetadataValue::String("abc123".into()),
            flags: MetadataFlags::NONE,
        },
        MetadataEntry {
            key: "auth".into(),
            value: MetadataValue::Bytes((&[0xDE, 0xAD, 0xBE, 0xEF][..]).into()),
            flags: MetadataFlags::SENSITIVE | MetadataFlags::NO_PROPAGATE,
        },
        MetadataEntry {
            key: "attempt".into(),
            value: MetadataValue::U64(2),
            flags: MetadataFlags::NONE,
        },
    ]
}

fn main() {
    macro_rules! write_value {
        ($path:literal, $value:expr) => {{
            let bytes = vox_postcard::to_vec(&$value).expect("serialize fixture");
            write_fixture($path, &bytes);
        }};
    }

    // -------------------------------------------------------------------------
    // Varint
    // -------------------------------------------------------------------------
    write_value!("varint/u64_0.bin", 0u64);
    write_value!("varint/u64_1.bin", 1u64);
    write_value!("varint/u64_127.bin", 127u64);
    write_value!("varint/u64_128.bin", 128u64);
    write_value!("varint/u64_255.bin", 255u64);
    write_value!("varint/u64_256.bin", 256u64);
    write_value!("varint/u64_16383.bin", 16383u64);
    write_value!("varint/u64_16384.bin", 16384u64);
    write_value!("varint/u64_65535.bin", 65535u64);
    write_value!("varint/u64_65536.bin", 65536u64);
    write_value!("varint/u64_1048576.bin", 1_048_576u64);

    // -------------------------------------------------------------------------
    // Primitives
    // -------------------------------------------------------------------------
    write_value!("primitives/bool_false.bin", false);
    write_value!("primitives/bool_true.bin", true);

    write_value!("primitives/u8_0.bin", 0u8);
    write_value!("primitives/u8_127.bin", 127u8);
    write_value!("primitives/u8_255.bin", 255u8);

    write_value!("primitives/i8_0.bin", 0i8);
    write_value!("primitives/i8_neg1.bin", -1i8);
    write_value!("primitives/i8_127.bin", 127i8);
    write_value!("primitives/i8_neg128.bin", -128i8);

    write_value!("primitives/u16_0.bin", 0u16);
    write_value!("primitives/u16_127.bin", 127u16);
    write_value!("primitives/u16_128.bin", 128u16);
    write_value!("primitives/u16_255.bin", 255u16);
    write_value!("primitives/u16_256.bin", 256u16);
    write_value!("primitives/u16_max.bin", u16::MAX);

    write_value!("primitives/i16_0.bin", 0i16);
    write_value!("primitives/i16_1.bin", 1i16);
    write_value!("primitives/i16_neg1.bin", -1i16);
    write_value!("primitives/i16_127.bin", 127i16);
    write_value!("primitives/i16_128.bin", 128i16);
    write_value!("primitives/i16_max.bin", i16::MAX);
    write_value!("primitives/i16_min.bin", i16::MIN);

    write_value!("primitives/u32_0.bin", 0u32);
    write_value!("primitives/u32_1.bin", 1u32);
    write_value!("primitives/u32_127.bin", 127u32);
    write_value!("primitives/u32_128.bin", 128u32);
    write_value!("primitives/u32_255.bin", 255u32);
    write_value!("primitives/u32_256.bin", 256u32);
    write_value!("primitives/u32_max.bin", u32::MAX);

    write_value!("primitives/i32_0.bin", 0i32);
    write_value!("primitives/i32_1.bin", 1i32);
    write_value!("primitives/i32_neg1.bin", -1i32);
    write_value!("primitives/i32_127.bin", 127i32);
    write_value!("primitives/i32_128.bin", 128i32);
    write_value!("primitives/i32_neg128.bin", -128i32);
    write_value!("primitives/i32_max.bin", i32::MAX);
    write_value!("primitives/i32_min.bin", i32::MIN);

    write_value!("primitives/u64_0.bin", 0u64);
    write_value!("primitives/u64_1.bin", 1u64);
    write_value!("primitives/u64_127.bin", 127u64);
    write_value!("primitives/u64_128.bin", 128u64);
    write_value!("primitives/u64_max.bin", u64::MAX);

    write_value!("primitives/i64_0.bin", 0i64);
    write_value!("primitives/i64_1.bin", 1i64);
    write_value!("primitives/i64_neg1.bin", -1i64);
    write_value!("primitives/i64_15.bin", 15i64);
    write_value!("primitives/i64_42.bin", 42i64);
    write_value!("primitives/i64_max.bin", i64::MAX);
    write_value!("primitives/i64_min.bin", i64::MIN);

    write_value!("primitives/f32_0.bin", 0.0f32);
    write_value!("primitives/f32_1.bin", 1.0f32);
    write_value!("primitives/f32_neg1.bin", -1.0f32);
    write_value!("primitives/f32_1_5.bin", 1.5f32);
    write_value!("primitives/f32_0_25.bin", 0.25f32);

    write_value!("primitives/f64_0.bin", 0.0f64);
    write_value!("primitives/f64_1.bin", 1.0f64);
    write_value!("primitives/f64_neg1.bin", -1.0f64);
    write_value!("primitives/f64_1_5.bin", 1.5f64);
    write_value!("primitives/f64_0_25.bin", 0.25f64);

    write_value!("primitives/string_empty.bin", String::new());
    write_value!("primitives/string_hello.bin", "hello world".to_string());
    write_value!("primitives/string_unicode.bin", "héllo 世界 🦀".to_string());

    write_value!("primitives/bytes_empty.bin", Vec::<u8>::new());
    write_value!(
        "primitives/bytes_deadbeef.bin",
        vec![0xDEu8, 0xAD, 0xBE, 0xEF]
    );

    write_value!("primitives/option_none_u32.bin", Option::<u32>::None);
    write_value!("primitives/option_some_u32_42.bin", Some(42u32));
    write_value!("primitives/option_none_string.bin", Option::<String>::None);
    write_value!(
        "primitives/option_some_string.bin",
        Some("hello".to_string())
    );

    write_value!("primitives/vec_empty_u32.bin", Vec::<u32>::new());
    write_value!("primitives/vec_u32_1_2_3.bin", vec![1u32, 2, 3]);
    write_value!("primitives/vec_i32_neg1_0_1.bin", vec![-1i32, 0, 1]);
    write_value!(
        "primitives/vec_string.bin",
        vec!["a".to_string(), "b".to_string()]
    );

    // -------------------------------------------------------------------------
    // Composite types (structs, enums, tuples)
    // -------------------------------------------------------------------------
    {
        use facet::Facet;

        #[derive(Facet)]
        struct Point {
            x: i32,
            y: i32,
        }
        write_value!("composite/struct_point.bin", Point { x: 10, y: -20 });

        #[derive(Facet)]
        struct Nested {
            name: String,
            point: Point,
            tags: Vec<String>,
        }
        write_value!(
            "composite/struct_nested.bin",
            Nested {
                name: "test".to_string(),
                point: Point { x: 1, y: 2 },
                tags: vec!["a".to_string(), "bb".to_string()],
            }
        );

        #[derive(Facet)]
        #[repr(u8)]
        enum Color {
            Red,
            Green,
            Blue,
        }
        write_value!("composite/enum_red.bin", Color::Red);
        write_value!("composite/enum_green.bin", Color::Green);
        write_value!("composite/enum_blue.bin", Color::Blue);

        #[derive(Facet)]
        #[repr(u8)]
        #[allow(unused)]
        enum Shape {
            Circle(f64),
            Rect { w: f64, h: f64 },
            Empty,
        }
        write_value!(
            "composite/enum_circle.bin",
            Shape::Circle(
                #[allow(clippy::approx_constant)]
                3.14
            )
        );
        write_value!("composite/enum_rect.bin", Shape::Rect { w: 10.0, h: 20.0 });
        write_value!("composite/enum_empty.bin", Shape::Empty);

        write_value!(
            "composite/tuple_u32_string.bin",
            (42u32, "hello".to_string())
        );
        write_value!("composite/tuple_bool_i64.bin", (true, -99i64));

        write_value!(
            "composite/option_some_point.bin",
            Some(Point { x: 5, y: 6 })
        );
        write_value!("composite/option_none_point.bin", Option::<Point>::None);

        write_value!(
            "composite/vec_points.bin",
            vec![
                Point { x: 1, y: 2 },
                Point { x: 3, y: 4 },
                Point { x: 5, y: 6 },
            ]
        );

        // Maps — use BTreeMap for deterministic ordering
        use std::collections::BTreeMap;
        let mut map1 = BTreeMap::new();
        map1.insert("alpha".to_string(), 1u32);
        map1.insert("beta".to_string(), 2);
        write_value!("composite/map_string_u32.bin", map1);

        let mut map2 = BTreeMap::new();
        map2.insert("key".to_string(), Point { x: 10, y: 20 });
        write_value!("composite/map_string_point.bin", map2);
    }

    // -------------------------------------------------------------------------
    // Result / VoxError
    // -------------------------------------------------------------------------
    write_value!(
        "result/ok_string.bin",
        Ok::<String, VoxError<std::convert::Infallible>>("hello".to_string())
    );
    write_value!(
        "result/ok_u32.bin",
        Ok::<u32, VoxError<std::convert::Infallible>>(42u32)
    );
    write_value!(
        "result/err_unknown_method.bin",
        Err::<(), VoxError<std::convert::Infallible>>(VoxError::UnknownMethod)
    );
    write_value!(
        "result/err_invalid_payload.bin",
        Err::<(), VoxError<std::convert::Infallible>>(VoxError::InvalidPayload(String::new()))
    );
    write_value!(
        "result/err_cancelled.bin",
        Err::<(), VoxError<std::convert::Infallible>>(VoxError::Cancelled)
    );
    write_value!(
        "result/err_user_string.bin",
        Err::<(), VoxError<String>>(VoxError::User("oops".to_string()))
    );

    // -------------------------------------------------------------------------
    // Wire messages
    // -------------------------------------------------------------------------
    let conn_settings = ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
    };
    let meta = sample_metadata();

    // Hello and HelloYourself are no longer MessagePayload variants.
    // They are CBOR-encoded handshake messages exchanged before postcard traffic.

    write_fixture(
        "wire/message_protocol_error.bin",
        &encode_message(&Message {
            connection_id: ConnectionId::ROOT,
            payload: MessagePayload::ProtocolError(ProtocolError {
                description: "bad frame sequence",
            }),
        }),
    );

    write_fixture(
        "wire/message_connection_open.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ConnectionOpen(ConnectionOpen {
                connection_settings: conn_settings.clone(),
                metadata: meta.clone(),
            }),
        }),
    );

    write_fixture(
        "wire/message_connection_accept.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ConnectionAccept(ConnectionAccept {
                connection_settings: ConnectionSettings {
                    parity: Parity::Even,
                    max_concurrent_requests: 96,
                },
                metadata: meta.clone(),
            }),
        }),
    );

    write_fixture(
        "wire/message_connection_reject.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(4),
            payload: MessagePayload::ConnectionReject(ConnectionReject {
                metadata: meta.clone(),
            }),
        }),
    );

    write_fixture(
        "wire/message_connection_close.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ConnectionClose(ConnectionClose {
                metadata: meta.clone(),
            }),
        }),
    );

    let args_call: u32 = 0x1234_5678;
    write_fixture(
        "wire/message_request_call.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(11),
                body: RequestBody::Call(RequestCall {
                    method_id: MethodId(0xE5A1_D6B2_C390_F001),
                    args: Payload::outgoing(&args_call),
                    metadata: meta.clone(),
                    schemas: Default::default(),
                }),
            }),
        }),
    );

    let ret_response: u64 = 0xFACE_B00C;
    write_fixture(
        "wire/message_request_response.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(11),
                body: RequestBody::Response(RequestResponse {
                    ret: Payload::outgoing(&ret_response),
                    metadata: meta.clone(),
                    schemas: Default::default(),
                }),
            }),
        }),
    );

    write_fixture(
        "wire/message_request_cancel.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(11),
                body: RequestBody::Cancel(RequestCancel {
                    metadata: meta.clone(),
                }),
            }),
        }),
    );

    let channel_item_value: u16 = 77;
    write_fixture(
        "wire/message_channel_item.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ChannelMessage(ChannelMessage {
                id: ChannelId(3),
                body: ChannelBody::Item(ChannelItem {
                    item: Payload::outgoing(&channel_item_value),
                }),
            }),
        }),
    );

    write_fixture(
        "wire/message_channel_close.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ChannelMessage(ChannelMessage {
                id: ChannelId(3),
                body: ChannelBody::Close(ChannelClose {
                    metadata: meta.clone(),
                }),
            }),
        }),
    );

    write_fixture(
        "wire/message_channel_reset.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ChannelMessage(ChannelMessage {
                id: ChannelId(3),
                body: ChannelBody::Reset(ChannelReset {
                    metadata: meta.clone(),
                }),
            }),
        }),
    );

    write_fixture(
        "wire/message_channel_grant_credit.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ChannelMessage(ChannelMessage {
                id: ChannelId(3),
                body: ChannelBody::GrantCredit(ChannelGrantCredit { additional: 1024 }),
            }),
        }),
    );
}
