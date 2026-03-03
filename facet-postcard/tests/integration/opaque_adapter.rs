use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst};
use facet_postcard::{from_slice, from_slice_borrowed, to_vec};

#[derive(Debug, Facet)]
#[repr(u8)]
#[facet(opaque = PayloadAdapter, traits(Debug))]
enum Payload<'a> {
    Borrowed(&'a [u8]),
    RawBorrowed(&'a [u8]),
    RawOwned(Vec<u8>),
}

struct PayloadAdapter;

impl FacetOpaqueAdapter for PayloadAdapter {
    type Error = String;
    type SendValue<'a> = Payload<'a>;
    type RecvValue<'de> = Payload<'de>;

    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
        match value {
            Payload::Borrowed(bytes) => OpaqueSerialize {
                ptr: PtrConst::new(bytes as *const &[u8]),
                shape: <&[u8] as Facet>::SHAPE,
            },
            _ => unreachable!("serialize_map is only used for outgoing payload values"),
        }
    }

    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        Ok(match input {
            OpaqueDeserialize::Borrowed(bytes) => Payload::RawBorrowed(bytes),
            OpaqueDeserialize::Owned(bytes) => Payload::RawOwned(bytes),
        })
    }
}

#[derive(Debug, Facet)]
struct Envelope<'a> {
    payload: Payload<'a>,
}

#[derive(Debug, Facet)]
struct Framed<'a> {
    id: u8,
    payload: Payload<'a>,
}

#[test]
fn test_opaque_adapter_roundtrip_borrowed_and_owned() {
    let value = Envelope {
        payload: Payload::Borrowed(&[0xAA, 0xBB, 0xCC]),
    };
    let bytes = to_vec(&value).expect("serialization should succeed");
    assert_eq!(bytes, vec![4, 3, 0xAA, 0xBB, 0xCC]);

    let decoded_borrowed: Envelope<'_> =
        from_slice_borrowed(&bytes).expect("borrowed deserialization should succeed");
    match decoded_borrowed.payload {
        Payload::RawBorrowed(slice) => {
            assert_eq!(slice, &[3, 0xAA, 0xBB, 0xCC]);
            assert_eq!(slice.as_ptr(), bytes[1..].as_ptr());
        }
        other => panic!("expected RawBorrowed, got {other:?}"),
    }

    let decoded_owned: Envelope<'static> =
        from_slice(&bytes).expect("owned deserialization should succeed");
    match decoded_owned.payload {
        Payload::RawOwned(buf) => assert_eq!(buf, vec![3, 0xAA, 0xBB, 0xCC]),
        other => panic!("expected RawOwned, got {other:?}"),
    }
}

#[test]
fn test_opaque_adapter_uses_postcard_byte_sequence_framing() {
    let value = Framed {
        id: 7,
        payload: Payload::Borrowed(&[0x10, 0x20]),
    };
    let bytes = to_vec(&value).expect("serialization should succeed");
    assert_eq!(bytes, vec![7, 3, 2, 0x10, 0x20]);
}
