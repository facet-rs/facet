//! End-to-end validation that phon can carry vox's `Message` envelope: the
//! opaque `Payload` field, `Cow` metadata, transparent id newtypes, and borrowed
//! decode of the whole thing.

use facet::Facet;
use vox_types::*;

fn probe<T: Facet<'static>>(name: &str) {
    match phon::derive::of::<T>() {
        Ok(d) => println!("OK   {name}: root={:?} schemas={}", d.root, d.schemas.len()),
        Err(e) => println!("FAIL {name}: {e}"),
    }
}

#[test]
fn probe_vox_wire_types() {
    probe::<ConnectionId>("ConnectionId");
    probe::<RequestCall>("RequestCall");
    probe::<MessagePayload>("MessagePayload");
    probe::<Message>("Message");
}

/// A full `Message` (RequestCall carrying an inline `Payload::Value`) round-trips
/// through phon: encode the envelope (opaque payload sub-encoded inline), then
/// borrowed-decode it back. The payload becomes a borrowed span pointing INTO the
/// wire, metadata strings borrow the wire, and the span re-decodes to the args.
#[test]
fn message_with_value_payload_roundtrips() {
    let args: u32 = 42;
    let msg = Message {
        connection_id: ConnectionId(1),
        payload: MessagePayload::RequestMessage(RequestMessage {
            id: RequestId(7),
            body: RequestBody::Call(RequestCall {
                method_id: MethodId(0xABCD),
                channels: Vec::new(),
                metadata: vox_types::metadata()
                    .str("trace", "abc")
                    .u64("n", 99)
                    .build(),
                args: Payload::outgoing(&args),
                schemas: SchemaBytes::default(),
            }),
        }),
    };

    let bytes = vox_phon::to_vec(&msg).expect("encode Message");

    let decoded: Message = vox_phon::from_slice_borrowed(&bytes).expect("decode Message");
    assert_eq!(decoded.connection_id, ConnectionId(1));

    let MessagePayload::RequestMessage(rm) = &decoded.payload else {
        panic!("expected RequestMessage, got {:?}", decoded.payload);
    };
    assert_eq!(rm.id, RequestId(7));

    let RequestBody::Call(call) = &rm.body else {
        panic!("expected Call");
    };
    assert_eq!(call.method_id, MethodId(0xABCD));

    // Metadata: a self-describing Value map decoded from the wire.
    use vox_types::MetadataExt;
    assert_eq!(call.metadata.meta_len(), 2);
    assert_eq!(call.metadata.meta_str("trace"), Some("abc"));
    assert_eq!(call.metadata.meta_u64("n"), Some(99));

    // The opaque payload decoded to a borrowed span pointing INTO the wire.
    let Payload::Encoded(span) = &call.args else {
        panic!("expected a borrowed payload span");
    };
    let wire_start = bytes.as_ptr() as usize;
    assert!(
        (wire_start..wire_start + bytes.len()).contains(&(span.as_ptr() as usize)),
        "payload span must point into the wire buffer"
    );
    let span_offset = (span.as_ptr() as usize) - wire_start;
    assert!(
        span_offset >= 4,
        "opaque payload span must follow its u32 length prefix"
    );
    let prefix: [u8; 4] = bytes[span_offset - 4..span_offset]
        .try_into()
        .expect("length prefix slice");
    assert_eq!(u32::from_le_bytes(prefix), span.len() as u32);

    // And the span re-decodes to the original args.
    let back: u32 = vox_phon::from_slice(span).expect("decode payload span");
    assert_eq!(back, 42);
}

#[test]
fn probe_payload_wire_types() {
    use vox_types::VoxError;
    probe::<(u32,)>("(u32,)");
    probe::<((i32, String),)>("((i32,String),)");
    probe::<VoxError<String>>("VoxError<String>");
    probe::<Result<u32, VoxError<String>>>("Result<u32, VoxError<String>>");
    probe::<Result<String, VoxError<String>>>("Result<String, VoxError<String>>");
}
