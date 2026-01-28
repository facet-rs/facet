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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
pub struct ConnectionId(pub u64);

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
