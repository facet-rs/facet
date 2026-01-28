//! Test for roam-wire Message enum pattern.
//!
//! This reproduces the failure seen in roam where decoding a Message::Goodbye
//! fails with "got struct start, expected field key, ordered field, or struct end".
//!
//! IMPORTANT: roam-wire types only derive Facet, NOT serde. This means facet-postcard
//! uses the pure Facet deserialization path, not the serde compatibility path.

use facet::Facet;
use facet_postcard::{from_slice, to_vec};

// ============================================================================
// Types that ONLY derive Facet (matching roam-wire exactly)
// ============================================================================

/// Newtype for connection ID (Facet only, no serde).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(transparent)]
pub struct ConnectionId(pub u64);

/// Newtype for request ID (Facet only, no serde).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(transparent)]
pub struct RequestId(pub u64);

/// Newtype for method ID (Facet only, no serde).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(transparent)]
pub struct MethodId(pub u64);

/// Simplified Hello enum matching roam-wire (Facet only).
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Hello {
    V1 {
        max_payload_size: u32,
        initial_channel_credit: u32,
    } = 0,
    V2 {
        max_payload_size: u32,
        initial_channel_credit: u32,
    } = 1,
    V3 {
        max_payload_size: u32,
        initial_channel_credit: u32,
    } = 2,
}

/// Metadata value enum (Facet only).
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum MetadataValue {
    String(String) = 0,
    Bytes(Vec<u8>) = 1,
    U64(u64) = 2,
}

/// Metadata is a list of (key, value, flags) tuples.
pub type Metadata = Vec<(String, MetadataValue, u64)>;

/// Simplified Message enum matching roam-wire structure (Facet only).
/// The key is having multiple struct variants with different field counts.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Message {
    Hello(Hello) = 0,
    Connect {
        request_id: u64,
        metadata: Metadata,
    } = 1,
    Accept {
        request_id: u64,
        conn_id: ConnectionId,
        metadata: Metadata,
    } = 2,
    Reject {
        request_id: u64,
        reason: String,
        metadata: Metadata,
    } = 3,
    Goodbye {
        conn_id: ConnectionId,
        reason: String,
    } = 4,
    Request {
        conn_id: ConnectionId,
        request_id: u64,
        method_id: u64,
        metadata: Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 5,
    Response {
        conn_id: ConnectionId,
        request_id: u64,
        metadata: Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 6,
    Cancel {
        conn_id: ConnectionId,
        request_id: u64,
    } = 7,
    Data {
        conn_id: ConnectionId,
        channel_id: u64,
        payload: Vec<u8>,
    } = 8,
    Close {
        conn_id: ConnectionId,
        channel_id: u64,
    } = 9,
    Reset {
        conn_id: ConnectionId,
        channel_id: u64,
    } = 10,
    Credit {
        conn_id: ConnectionId,
        channel_id: u64,
        bytes: u32,
    } = 11,
}

// ============================================================================
// Pure Facet roundtrip tests (no serde involved)
// ============================================================================

#[test]
fn test_hello_v3_roundtrip() {
    facet_testhelpers::setup();

    let hello = Hello::V3 {
        max_payload_size: 1024 * 1024,
        initial_channel_credit: 64 * 1024,
    };

    // Encode with facet-postcard
    let bytes = to_vec(&hello).expect("facet should encode Hello");
    // Decode with facet-postcard
    let decoded: Hello = from_slice(&bytes).expect("should deserialize Hello::V3");
    assert_eq!(decoded, hello);
}

#[test]
fn test_message_hello_roundtrip() {
    facet_testhelpers::setup();

    let msg = Message::Hello(Hello::V3 {
        max_payload_size: 1024 * 1024,
        initial_channel_credit: 64 * 1024,
    });

    let bytes = to_vec(&msg).expect("facet should encode Message::Hello");
    let decoded: Message = from_slice(&bytes).expect("should deserialize Message::Hello");
    assert_eq!(decoded, msg);
}

#[test]
fn test_connectionid_shape() {
    use facet_core::{Type, UserType};

    let shape = ConnectionId::SHAPE;
    eprintln!("ConnectionId shape:");
    eprintln!("  type_identifier: {}", shape.type_identifier);
    eprintln!("  def: {:?}", shape.def);
    eprintln!("  inner: {:?}", shape.inner.map(|s| s.type_identifier));
    eprintln!("  has_try_from: {}", shape.vtable.has_try_from());
    eprintln!("  is_transparent: {}", shape.is_transparent());

    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        eprintln!("  struct kind: {:?}", struct_def.kind);
        eprintln!("  fields: {:?}", struct_def.fields.len());
        for (i, f) in struct_def.fields.iter().enumerate() {
            eprintln!(
                "    field[{}]: name={}, shape={}",
                i,
                f.name,
                f.shape.get().type_identifier
            );
        }
    }
}

#[test]
fn test_message_goodbye_roundtrip() {
    facet_testhelpers::setup();

    let msg = Message::Goodbye {
        conn_id: ConnectionId(0),
        reason: "test reason".to_string(),
    };

    let bytes = to_vec(&msg).expect("facet should encode Message::Goodbye");
    eprintln!("Goodbye encoded as {} bytes: {:02x?}", bytes.len(), bytes);
    eprintln!("  byte[0] = {:02x} (variant discriminant)", bytes[0]);
    eprintln!("  byte[1] = {:02x} (conn_id varint)", bytes[1]);
    eprintln!("  byte[2] = {:02x} (string length)", bytes[2]);
    let decoded: Message = from_slice(&bytes).expect("should deserialize Message::Goodbye");
    assert_eq!(decoded, msg);
}

/// Test decoding exact bytes from roam failure
#[test]
fn test_decode_exact_goodbye_bytes() {
    facet_testhelpers::setup();

    // These are the exact bytes that failed in roam:
    // 04 = variant 4 (Goodbye)
    // 00 = conn_id varint (0)
    // 14 = string length varint (20)
    // followed by "message.decode-error"
    let bytes: Vec<u8> = vec![
        0x04, 0x00, 0x14, 0x6d, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, 0x2e, 0x64, 0x65, 0x63, 0x6f,
        0x64, 0x65, 0x2d, 0x65, 0x72, 0x72, 0x6f, 0x72,
    ];

    let decoded: Message =
        from_slice(&bytes).expect("should deserialize Message::Goodbye from exact bytes");
    assert_eq!(
        decoded,
        Message::Goodbye {
            conn_id: ConnectionId(0),
            reason: "message.decode-error".to_string(),
        }
    );
}

#[test]
fn test_all_message_variants() {
    facet_testhelpers::setup();

    let messages: Vec<Message> = vec![
        Message::Hello(Hello::V3 {
            max_payload_size: 1024,
            initial_channel_credit: 512,
        }),
        Message::Connect {
            request_id: 1,
            metadata: vec![],
        },
        Message::Accept {
            request_id: 1,
            conn_id: ConnectionId(1),
            metadata: vec![],
        },
        Message::Reject {
            request_id: 1,
            reason: "rejected".to_string(),
            metadata: vec![],
        },
        Message::Goodbye {
            conn_id: ConnectionId(0),
            reason: "bye".to_string(),
        },
        Message::Request {
            conn_id: ConnectionId(0),
            request_id: 1,
            method_id: 1,
            metadata: vec![],
            channels: vec![],
            payload: vec![],
        },
        Message::Response {
            conn_id: ConnectionId(0),
            request_id: 1,
            metadata: vec![],
            channels: vec![],
            payload: vec![],
        },
        Message::Cancel {
            conn_id: ConnectionId(0),
            request_id: 1,
        },
        Message::Data {
            conn_id: ConnectionId(0),
            channel_id: 1,
            payload: vec![1, 2, 3],
        },
        Message::Close {
            conn_id: ConnectionId(0),
            channel_id: 1,
        },
        Message::Reset {
            conn_id: ConnectionId(0),
            channel_id: 1,
        },
        Message::Credit {
            conn_id: ConnectionId(0),
            channel_id: 1,
            bytes: 1024,
        },
    ];

    for (i, msg) in messages.iter().enumerate() {
        // Encode with facet-postcard (pure Facet, no serde)
        let bytes = to_vec(msg).unwrap_or_else(|e| panic!("should encode variant {i}: {e}"));
        // Decode with facet-postcard
        let decoded: Message =
            from_slice(&bytes).unwrap_or_else(|e| panic!("should deserialize variant {i}: {e}"));
        assert_eq!(&decoded, msg, "variant {i} mismatch");
    }
}

// ============================================================================
// Regression tests for transparent newtype handling in enum struct variants
// ============================================================================
//
// The bug: When deserializing a struct variant containing a transparent newtype field,
// `deserialize_tuple` was calling `hint_struct_fields` BEFORE checking if the type
// was transparent. For transparent newtypes, we don't consume struct events - we
// deserialize the inner value directly. But if `hint_struct_fields` was already called,
// non-self-describing parsers would emit StructStart when the inner value deserializer
// expected a scalar, causing "unexpected token: got struct start" errors.

/// Test transparent newtype in isolation
#[test]
fn test_transparent_newtype_standalone() {
    facet_testhelpers::setup();

    let id = ConnectionId(42);
    let bytes = to_vec(&id).expect("should encode ConnectionId");
    // Should be just a varint for 42
    assert_eq!(bytes, vec![42]);

    let decoded: ConnectionId = from_slice(&bytes).expect("should decode ConnectionId");
    assert_eq!(decoded, id);
}

/// Test transparent newtype in a simple struct
#[test]
fn test_transparent_newtype_in_struct() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    struct SimpleStruct {
        id: ConnectionId,
        name: String,
    }

    let s = SimpleStruct {
        id: ConnectionId(123),
        name: "test".to_string(),
    };

    let bytes = to_vec(&s).expect("should encode SimpleStruct");
    let decoded: SimpleStruct = from_slice(&bytes).expect("should decode SimpleStruct");
    assert_eq!(decoded, s);
}

/// Test transparent newtype as first field in enum struct variant
/// This is the exact pattern that was failing in roam-wire
#[test]
fn test_transparent_newtype_first_field_in_enum_struct_variant() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct MyId(u64);

    #[repr(u8)]
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    enum MyEnum {
        Unit = 0,
        WithId { id: MyId, data: String } = 1,
    }

    let msg = MyEnum::WithId {
        id: MyId(42),
        data: "hello".to_string(),
    };

    let bytes = to_vec(&msg).expect("should encode MyEnum::WithId");
    let decoded: MyEnum = from_slice(&bytes).expect("should decode MyEnum::WithId");
    assert_eq!(decoded, msg);
}

/// Test multiple transparent newtypes in enum struct variant
#[test]
fn test_multiple_transparent_newtypes_in_enum_struct_variant() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct UserId(u64);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct SessionId(u64);

    #[repr(u8)]
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    enum Event {
        Login {
            user: UserId,
            session: SessionId,
        } = 0,
        Logout {
            user: UserId,
            session: SessionId,
            reason: String,
        } = 1,
    }

    let login = Event::Login {
        user: UserId(1),
        session: SessionId(100),
    };
    let bytes = to_vec(&login).expect("encode Login");
    let decoded: Event = from_slice(&bytes).expect("decode Login");
    assert_eq!(decoded, login);

    let logout = Event::Logout {
        user: UserId(1),
        session: SessionId(100),
        reason: "timeout".to_string(),
    };
    let bytes = to_vec(&logout).expect("encode Logout");
    let decoded: Event = from_slice(&bytes).expect("decode Logout");
    assert_eq!(decoded, logout);
}

/// Test nested transparent newtypes
#[test]
fn test_nested_transparent_newtypes() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct Inner(u32);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct Outer(Inner);

    let val = Outer(Inner(42));
    let bytes = to_vec(&val).expect("encode Outer");
    // Should be just a varint for 42
    assert_eq!(bytes, vec![42]);

    let decoded: Outer = from_slice(&bytes).expect("decode Outer");
    assert_eq!(decoded, val);
}

/// Test transparent newtype wrapping a String
#[test]
fn test_transparent_string_newtype() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct Name(String);

    #[repr(u8)]
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    enum Greeting {
        Hello { name: Name } = 0,
        Goodbye { name: Name, message: String } = 1,
    }

    let hello = Greeting::Hello {
        name: Name("Alice".to_string()),
    };
    let bytes = to_vec(&hello).expect("encode Hello");
    let decoded: Greeting = from_slice(&bytes).expect("decode Hello");
    assert_eq!(decoded, hello);

    let goodbye = Greeting::Goodbye {
        name: Name("Bob".to_string()),
        message: "See you later".to_string(),
    };
    let bytes = to_vec(&goodbye).expect("encode Goodbye");
    let decoded: Greeting = from_slice(&bytes).expect("decode Goodbye");
    assert_eq!(decoded, goodbye);
}

/// Test transparent newtype wrapping Vec<u8>
#[test]
fn test_transparent_bytes_newtype() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct Payload(Vec<u8>);

    #[repr(u8)]
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    enum Packet {
        Data { payload: Payload } = 0,
        DataWithMeta { payload: Payload, seq: u64 } = 1,
    }

    let data = Packet::Data {
        payload: Payload(vec![1, 2, 3, 4, 5]),
    };
    let bytes = to_vec(&data).expect("encode Data");
    let decoded: Packet = from_slice(&bytes).expect("decode Data");
    assert_eq!(decoded, data);

    let data_with_meta = Packet::DataWithMeta {
        payload: Payload(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        seq: 42,
    };
    let bytes = to_vec(&data_with_meta).expect("encode DataWithMeta");
    let decoded: Packet = from_slice(&bytes).expect("decode DataWithMeta");
    assert_eq!(decoded, data_with_meta);
}

/// Test enum with mix of unit, tuple, and struct variants containing transparent newtypes
#[test]
fn test_mixed_enum_variants_with_transparent_newtypes() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct Id(u64);

    #[repr(u8)]
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    enum MixedEnum {
        Unit = 0,
        Tuple(Id) = 1,
        TupleTwo(Id, String) = 2,
        Struct { id: Id } = 3,
        StructTwo { id: Id, name: String } = 4,
        StructThree { id: Id, name: String, active: bool } = 5,
    }

    let variants: Vec<MixedEnum> = vec![
        MixedEnum::Unit,
        MixedEnum::Tuple(Id(1)),
        MixedEnum::TupleTwo(Id(2), "two".to_string()),
        MixedEnum::Struct { id: Id(3) },
        MixedEnum::StructTwo {
            id: Id(4),
            name: "four".to_string(),
        },
        MixedEnum::StructThree {
            id: Id(5),
            name: "five".to_string(),
            active: true,
        },
    ];

    for (i, variant) in variants.iter().enumerate() {
        let bytes = to_vec(variant).unwrap_or_else(|e| panic!("encode variant {i}: {e}"));
        let decoded: MixedEnum =
            from_slice(&bytes).unwrap_or_else(|e| panic!("decode variant {i}: {e}"));
        assert_eq!(&decoded, variant, "variant {i} mismatch");
    }
}

/// Test deeply nested enum containing struct variants with transparent newtypes
#[test]
fn test_nested_enum_with_transparent_newtypes() {
    facet_testhelpers::setup();

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
    #[repr(transparent)]
    struct ConnId(u64);

    #[repr(u8)]
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    enum Inner {
        A { conn: ConnId } = 0,
        B { conn: ConnId, data: u32 } = 1,
    }

    #[repr(u8)]
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    enum Outer {
        Wrap(Inner) = 0,
        Direct { conn: ConnId } = 1,
    }

    let wrap_a = Outer::Wrap(Inner::A { conn: ConnId(1) });
    let bytes = to_vec(&wrap_a).expect("encode Wrap(A)");
    let decoded: Outer = from_slice(&bytes).expect("decode Wrap(A)");
    assert_eq!(decoded, wrap_a);

    let wrap_b = Outer::Wrap(Inner::B {
        conn: ConnId(2),
        data: 42,
    });
    let bytes = to_vec(&wrap_b).expect("encode Wrap(B)");
    let decoded: Outer = from_slice(&bytes).expect("decode Wrap(B)");
    assert_eq!(decoded, wrap_b);

    let direct = Outer::Direct { conn: ConnId(3) };
    let bytes = to_vec(&direct).expect("encode Direct");
    let decoded: Outer = from_slice(&bytes).expect("decode Direct");
    assert_eq!(decoded, direct);
}

// ============================================================================
// Full roam-wire Message enum with typed IDs (matching actual roam-wire)
// ============================================================================

/// Full Message enum matching roam-wire exactly, with typed ID fields.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum FullMessage {
    Hello(Hello) = 0,
    Connect {
        request_id: RequestId,
        metadata: Metadata,
    } = 1,
    Accept {
        request_id: RequestId,
        conn_id: ConnectionId,
        metadata: Metadata,
    } = 2,
    Reject {
        request_id: RequestId,
        reason: String,
        metadata: Metadata,
    } = 3,
    Goodbye {
        conn_id: ConnectionId,
        reason: String,
    } = 4,
    Request {
        conn_id: ConnectionId,
        request_id: RequestId,
        method_id: MethodId,
        metadata: Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 5,
    Response {
        conn_id: ConnectionId,
        request_id: RequestId,
        metadata: Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 6,
    Cancel {
        conn_id: ConnectionId,
        request_id: RequestId,
    } = 7,
    Data {
        conn_id: ConnectionId,
        channel_id: u64,
        payload: Vec<u8>,
    } = 8,
    Close {
        conn_id: ConnectionId,
        channel_id: u64,
    } = 9,
    Reset {
        conn_id: ConnectionId,
        channel_id: u64,
    } = 10,
    Credit {
        conn_id: ConnectionId,
        channel_id: u64,
        bytes: u32,
    } = 11,
}

/// Test all FullMessage variants with typed IDs
#[test]
fn test_full_message_all_variants() {
    facet_testhelpers::setup();

    let messages: Vec<FullMessage> = vec![
        FullMessage::Hello(Hello::V3 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        }),
        FullMessage::Connect {
            request_id: RequestId(1),
            metadata: vec![],
        },
        FullMessage::Accept {
            request_id: RequestId(1),
            conn_id: ConnectionId(1),
            metadata: vec![],
        },
        FullMessage::Reject {
            request_id: RequestId(1),
            reason: "rejected".to_string(),
            metadata: vec![],
        },
        FullMessage::Goodbye {
            conn_id: ConnectionId(0),
            reason: "bye".to_string(),
        },
        FullMessage::Request {
            conn_id: ConnectionId(0),
            request_id: RequestId(1),
            method_id: MethodId(12345),
            metadata: vec![],
            channels: vec![],
            payload: vec![],
        },
        FullMessage::Response {
            conn_id: ConnectionId(0),
            request_id: RequestId(1),
            metadata: vec![],
            channels: vec![],
            payload: vec![],
        },
        FullMessage::Cancel {
            conn_id: ConnectionId(0),
            request_id: RequestId(1),
        },
        FullMessage::Data {
            conn_id: ConnectionId(0),
            channel_id: 1,
            payload: vec![1, 2, 3],
        },
        FullMessage::Close {
            conn_id: ConnectionId(0),
            channel_id: 1,
        },
        FullMessage::Reset {
            conn_id: ConnectionId(0),
            channel_id: 1,
        },
        FullMessage::Credit {
            conn_id: ConnectionId(0),
            channel_id: 1,
            bytes: 1024,
        },
    ];

    for (i, msg) in messages.iter().enumerate() {
        let bytes = to_vec(msg).unwrap_or_else(|e| panic!("encode variant {i}: {e}"));
        let decoded: FullMessage =
            from_slice(&bytes).unwrap_or_else(|e| panic!("decode variant {i}: {e}"));
        assert_eq!(&decoded, msg, "variant {i} mismatch");
    }
}

/// Test Request message with channels and payload (streaming pattern)
#[test]
fn test_request_with_channels_and_payload() {
    facet_testhelpers::setup();

    let msg = FullMessage::Request {
        conn_id: ConnectionId(0),
        request_id: RequestId(42),
        method_id: MethodId(0x123456789ABCDEF0),
        metadata: vec![
            (
                "key1".to_string(),
                MetadataValue::String("value1".to_string()),
                0,
            ),
            ("key2".to_string(), MetadataValue::U64(12345), 0),
        ],
        channels: vec![1, 2, 3],
        payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };

    let bytes = to_vec(&msg).expect("encode Request");
    let decoded: FullMessage = from_slice(&bytes).expect("decode Request");
    assert_eq!(decoded, msg);
}

/// Test Response message with channels and payload
#[test]
fn test_response_with_channels_and_payload() {
    facet_testhelpers::setup();

    let msg = FullMessage::Response {
        conn_id: ConnectionId(1),
        request_id: RequestId(42),
        metadata: vec![(
            "status".to_string(),
            MetadataValue::String("ok".to_string()),
            0,
        )],
        channels: vec![100, 101],
        payload: b"response payload data".to_vec(),
    };

    let bytes = to_vec(&msg).expect("encode Response");
    let decoded: FullMessage = from_slice(&bytes).expect("decode Response");
    assert_eq!(decoded, msg);
}

/// Test Data message (used in streaming)
#[test]
fn test_data_message_streaming() {
    facet_testhelpers::setup();

    // Test various payload sizes
    for size in [0, 1, 10, 100, 1000, 10000] {
        let msg = FullMessage::Data {
            conn_id: ConnectionId(0),
            channel_id: 42,
            payload: vec![0xAB; size],
        };

        let bytes = to_vec(&msg).expect("encode Data");
        let decoded: FullMessage = from_slice(&bytes).expect("decode Data");
        assert_eq!(decoded, msg, "Data with {size} byte payload mismatch");
    }
}

/// Test sequence of streaming messages (simulating a streaming RPC)
#[test]
fn test_streaming_message_sequence() {
    facet_testhelpers::setup();

    // Simulate: Request -> Data -> Data -> Data -> Close
    let messages: Vec<FullMessage> = vec![
        FullMessage::Request {
            conn_id: ConnectionId(0),
            request_id: RequestId(1),
            method_id: MethodId(100),
            metadata: vec![],
            channels: vec![1], // One channel for streaming
            payload: vec![],
        },
        FullMessage::Data {
            conn_id: ConnectionId(0),
            channel_id: 1,
            payload: b"chunk1".to_vec(),
        },
        FullMessage::Data {
            conn_id: ConnectionId(0),
            channel_id: 1,
            payload: b"chunk2".to_vec(),
        },
        FullMessage::Data {
            conn_id: ConnectionId(0),
            channel_id: 1,
            payload: b"chunk3".to_vec(),
        },
        FullMessage::Close {
            conn_id: ConnectionId(0),
            channel_id: 1,
        },
        FullMessage::Response {
            conn_id: ConnectionId(0),
            request_id: RequestId(1),
            metadata: vec![],
            channels: vec![],
            payload: b"final result".to_vec(),
        },
    ];

    for (i, msg) in messages.iter().enumerate() {
        let bytes = to_vec(msg).unwrap_or_else(|e| panic!("encode message {i}: {e}"));
        let decoded: FullMessage =
            from_slice(&bytes).unwrap_or_else(|e| panic!("decode message {i}: {e}"));
        assert_eq!(&decoded, msg, "message {i} mismatch");
    }
}

/// Test Cancel message
#[test]
fn test_cancel_message() {
    facet_testhelpers::setup();

    let msg = FullMessage::Cancel {
        conn_id: ConnectionId(5),
        request_id: RequestId(999),
    };

    let bytes = to_vec(&msg).expect("encode Cancel");
    let decoded: FullMessage = from_slice(&bytes).expect("decode Cancel");
    assert_eq!(decoded, msg);
}

/// Test Credit message (flow control)
#[test]
fn test_credit_message() {
    facet_testhelpers::setup();

    let msg = FullMessage::Credit {
        conn_id: ConnectionId(0),
        channel_id: 42,
        bytes: 65536,
    };

    let bytes = to_vec(&msg).expect("encode Credit");
    let decoded: FullMessage = from_slice(&bytes).expect("decode Credit");
    assert_eq!(decoded, msg);
}

/// Test Reset message
#[test]
fn test_reset_message() {
    facet_testhelpers::setup();

    let msg = FullMessage::Reset {
        conn_id: ConnectionId(0),
        channel_id: 42,
    };

    let bytes = to_vec(&msg).expect("encode Reset");
    let decoded: FullMessage = from_slice(&bytes).expect("decode Reset");
    assert_eq!(decoded, msg);
}

/// Test metadata with all value types
#[test]
fn test_metadata_all_value_types() {
    facet_testhelpers::setup();

    let msg = FullMessage::Request {
        conn_id: ConnectionId(0),
        request_id: RequestId(1),
        method_id: MethodId(1),
        metadata: vec![
            (
                "string_key".to_string(),
                MetadataValue::String("string value".to_string()),
                0,
            ),
            (
                "bytes_key".to_string(),
                MetadataValue::Bytes(vec![1, 2, 3, 4, 5]),
                0,
            ),
            (
                "u64_key".to_string(),
                MetadataValue::U64(0xFFFFFFFFFFFFFFFF),
                0,
            ),
            (
                "with_flags".to_string(),
                MetadataValue::String("sensitive".to_string()),
                1,
            ), // SENSITIVE flag
        ],
        channels: vec![],
        payload: vec![],
    };

    let bytes = to_vec(&msg).expect("encode Request with metadata");
    let decoded: FullMessage = from_slice(&bytes).expect("decode Request with metadata");
    assert_eq!(decoded, msg);
}

/// Test large channel lists
#[test]
fn test_large_channel_list() {
    facet_testhelpers::setup();

    let msg = FullMessage::Request {
        conn_id: ConnectionId(0),
        request_id: RequestId(1),
        method_id: MethodId(1),
        metadata: vec![],
        channels: (0..100).collect(), // 100 channels
        payload: vec![],
    };

    let bytes = to_vec(&msg).expect("encode Request with many channels");
    let decoded: FullMessage = from_slice(&bytes).expect("decode Request with many channels");
    assert_eq!(decoded, msg);
}

/// Test Connect/Accept/Reject sequence (virtual connection establishment)
#[test]
fn test_virtual_connection_sequence() {
    facet_testhelpers::setup();

    let connect = FullMessage::Connect {
        request_id: RequestId(1),
        metadata: vec![(
            "service".to_string(),
            MetadataValue::String("my.service".to_string()),
            0,
        )],
    };

    let accept = FullMessage::Accept {
        request_id: RequestId(1),
        conn_id: ConnectionId(42),
        metadata: vec![],
    };

    let reject = FullMessage::Reject {
        request_id: RequestId(2),
        reason: "service not found".to_string(),
        metadata: vec![],
    };

    for msg in [connect, accept, reject] {
        let bytes = to_vec(&msg).expect("encode");
        let decoded: FullMessage = from_slice(&bytes).expect("decode");
        assert_eq!(decoded, msg);
    }
}

// ============================================================================
// Edge cases and stress tests
// ============================================================================

/// Test with maximum u64 values in IDs
#[test]
fn test_max_u64_ids() {
    facet_testhelpers::setup();

    let msg = FullMessage::Request {
        conn_id: ConnectionId(u64::MAX),
        request_id: RequestId(u64::MAX),
        method_id: MethodId(u64::MAX),
        metadata: vec![],
        channels: vec![u64::MAX],
        payload: vec![],
    };

    let bytes = to_vec(&msg).expect("encode with max IDs");
    let decoded: FullMessage = from_slice(&bytes).expect("decode with max IDs");
    assert_eq!(decoded, msg);
}

/// Test with zero values everywhere
#[test]
fn test_zero_ids() {
    facet_testhelpers::setup();

    let msg = FullMessage::Request {
        conn_id: ConnectionId(0),
        request_id: RequestId(0),
        method_id: MethodId(0),
        metadata: vec![],
        channels: vec![0],
        payload: vec![0],
    };

    let bytes = to_vec(&msg).expect("encode with zero IDs");
    let decoded: FullMessage = from_slice(&bytes).expect("decode with zero IDs");
    assert_eq!(decoded, msg);
}

/// Test empty payload and channels
#[test]
fn test_empty_collections() {
    facet_testhelpers::setup();

    let msg = FullMessage::Request {
        conn_id: ConnectionId(0),
        request_id: RequestId(1),
        method_id: MethodId(1),
        metadata: vec![],
        channels: vec![],
        payload: vec![],
    };

    let bytes = to_vec(&msg).expect("encode with empty collections");
    let decoded: FullMessage = from_slice(&bytes).expect("decode with empty collections");
    assert_eq!(decoded, msg);
}

/// Test deeply nested metadata values
#[test]
fn test_complex_metadata() {
    facet_testhelpers::setup();

    let msg = FullMessage::Request {
        conn_id: ConnectionId(0),
        request_id: RequestId(1),
        method_id: MethodId(1),
        metadata: vec![
            // Empty string key and value
            ("".to_string(), MetadataValue::String("".to_string()), 0),
            // Unicode in key and value
            (
                "日本語キー".to_string(),
                MetadataValue::String("日本語値".to_string()),
                0,
            ),
            // Long key
            (
                "a".repeat(256),
                MetadataValue::String("long key".to_string()),
                0,
            ),
            // Large bytes value
            (
                "big_bytes".to_string(),
                MetadataValue::Bytes(vec![0xFF; 1000]),
                0,
            ),
            // Various flags
            (
                "flagged".to_string(),
                MetadataValue::U64(42),
                0xFFFFFFFFFFFFFFFF,
            ),
        ],
        channels: vec![],
        payload: vec![],
    };

    let bytes = to_vec(&msg).expect("encode with complex metadata");
    let decoded: FullMessage = from_slice(&bytes).expect("decode with complex metadata");
    assert_eq!(decoded, msg);
}
