//! Generate all golden vectors used by TypeScript and Swift tests.

use std::{fs, path::PathBuf};

use roam_types::{
    ChannelBody, ChannelClose, ChannelGrantCredit, ChannelId, ChannelItem, ChannelMessage,
    ChannelReset, ConnectionAccept, ConnectionClose, ConnectionId, ConnectionOpen,
    ConnectionReject, ConnectionSettings, Hello, HelloYourself, Message, MessagePayload, Metadata,
    MetadataEntry, MetadataFlags, MetadataValue, MethodId, Parity, Payload, ProtocolError,
    RequestBody, RequestCall, RequestCancel, RequestId, RequestMessage, RequestResponse, RoamError,
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
    facet_postcard::to_vec(message).expect("serialize message fixture")
}

fn sample_metadata() -> Metadata<'static> {
    vec![
        MetadataEntry {
            key: "trace-id",
            value: MetadataValue::String("abc123"),
            flags: MetadataFlags::NONE,
        },
        MetadataEntry {
            key: "auth",
            value: MetadataValue::Bytes(&[0xDE, 0xAD, 0xBE, 0xEF]),
            flags: MetadataFlags::SENSITIVE | MetadataFlags::NO_PROPAGATE,
        },
        MetadataEntry {
            key: "attempt",
            value: MetadataValue::U64(2),
            flags: MetadataFlags::NONE,
        },
    ]
}

fn main() {
    macro_rules! write_value {
        ($path:literal, $value:expr) => {{
            let bytes = facet_postcard::to_vec(&$value).expect("serialize fixture");
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
    // Result / RoamError
    // -------------------------------------------------------------------------
    write_value!(
        "result/ok_string.bin",
        Ok::<String, RoamError<std::convert::Infallible>>("hello".to_string())
    );
    write_value!(
        "result/ok_u32.bin",
        Ok::<u32, RoamError<std::convert::Infallible>>(42u32)
    );
    write_value!(
        "result/err_unknown_method.bin",
        Err::<(), RoamError<std::convert::Infallible>>(RoamError::UnknownMethod)
    );
    write_value!(
        "result/err_invalid_payload.bin",
        Err::<(), RoamError<std::convert::Infallible>>(RoamError::InvalidPayload)
    );
    write_value!(
        "result/err_cancelled.bin",
        Err::<(), RoamError<std::convert::Infallible>>(RoamError::Cancelled)
    );
    write_value!(
        "result/err_user_string.bin",
        Err::<(), RoamError<String>>(RoamError::User("oops".to_string()))
    );

    // -------------------------------------------------------------------------
    // Wire v7 messages
    // -------------------------------------------------------------------------
    let conn_settings = ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
    };
    let meta = sample_metadata();

    write_fixture(
        "wire-v7/message_hello.bin",
        &encode_message(&Message {
            connection_id: ConnectionId::ROOT,
            payload: MessagePayload::Hello(Hello {
                version: 7,
                connection_settings: conn_settings.clone(),
                metadata: meta.clone(),
            }),
        }),
    );

    write_fixture(
        "wire-v7/message_hello_yourself.bin",
        &encode_message(&Message {
            connection_id: ConnectionId::ROOT,
            payload: MessagePayload::HelloYourself(HelloYourself {
                connection_settings: ConnectionSettings {
                    parity: Parity::Even,
                    max_concurrent_requests: 32,
                },
                metadata: meta.clone(),
            }),
        }),
    );

    write_fixture(
        "wire-v7/message_protocol_error.bin",
        &encode_message(&Message {
            connection_id: ConnectionId::ROOT,
            payload: MessagePayload::ProtocolError(ProtocolError {
                description: "bad frame sequence",
            }),
        }),
    );

    write_fixture(
        "wire-v7/message_connection_open.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ConnectionOpen(ConnectionOpen {
                connection_settings: conn_settings.clone(),
                metadata: meta.clone(),
            }),
        }),
    );

    write_fixture(
        "wire-v7/message_connection_accept.bin",
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
        "wire-v7/message_connection_reject.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(4),
            payload: MessagePayload::ConnectionReject(ConnectionReject {
                metadata: meta.clone(),
            }),
        }),
    );

    write_fixture(
        "wire-v7/message_connection_close.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ConnectionClose(ConnectionClose {
                metadata: meta.clone(),
            }),
        }),
    );

    let args_call: u32 = 0x1234_5678;
    write_fixture(
        "wire-v7/message_request_call.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(11),
                body: RequestBody::Call(RequestCall {
                    method_id: MethodId(0xE5A1_D6B2_C390_F001),
                    args: Payload::outgoing(&args_call),
                    channels: vec![ChannelId(3), ChannelId(5)],
                    metadata: meta.clone(),
                }),
            }),
        }),
    );

    let ret_response: u64 = 0xFACE_B00C;
    write_fixture(
        "wire-v7/message_request_response.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(11),
                body: RequestBody::Response(RequestResponse {
                    ret: Payload::outgoing(&ret_response),
                    channels: vec![ChannelId(7)],
                    metadata: meta.clone(),
                }),
            }),
        }),
    );

    write_fixture(
        "wire-v7/message_request_cancel.bin",
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
        "wire-v7/message_channel_item.bin",
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
        "wire-v7/message_channel_close.bin",
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
        "wire-v7/message_channel_reset.bin",
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
        "wire-v7/message_channel_grant_credit.bin",
        &encode_message(&Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::ChannelMessage(ChannelMessage {
                id: ChannelId(3),
                body: ChannelBody::GrantCredit(ChannelGrantCredit { additional: 1024 }),
            }),
        }),
    );
}
