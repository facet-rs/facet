use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst};
use facet_postcard::{Segment, from_slice, from_slice_borrowed, to_scatter_plan, to_vec};

#[derive(Debug, Facet)]
#[repr(u8)]
#[facet(opaque = PayloadAdapter, traits(Debug))]
enum Payload<'a> {
    Outgoing(&'a [u8]),
    Incoming(&'a [u8]),
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
            Payload::Outgoing(bytes) => OpaqueSerialize::Mapped {
                ptr: PtrConst::new(bytes as *const &[u8]),
                shape: <&[u8] as Facet>::SHAPE,
            },
            Payload::Incoming(bytes) => OpaqueSerialize::encoded_bytes(bytes),
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
struct Framed<'a> {
    id: u8,
    payload: Payload<'a>,
}

#[derive(Debug, Facet)]
struct FramedTrailing<'a> {
    id: u8,
    #[facet(trailing)]
    payload: Payload<'a>,
}

fn flatten(plan: &facet_postcard::ScatterPlan<'_>) -> Vec<u8> {
    let mut out = vec![0u8; plan.total_size()];
    plan.write_into(&mut out)
        .expect("scatter plan should flatten");
    out
}

#[test]
fn issue_2118_non_trailing_encoded_bytes_match_typed_path() {
    let typed = Framed {
        id: 7,
        payload: Payload::Outgoing(&[0xAA, 0xBB]),
    };
    let passthrough = [2, 0xAA, 0xBB];
    let forwarded = Framed {
        id: 7,
        payload: Payload::Incoming(&passthrough),
    };

    let typed_bytes = to_vec(&typed).expect("typed serialization should succeed");
    let forwarded_bytes = to_vec(&forwarded).expect("passthrough serialization should succeed");
    assert_eq!(typed_bytes, forwarded_bytes);
    assert_eq!(forwarded_bytes, vec![7, 3, 2, 0xAA, 0xBB]);
}

#[test]
fn issue_2118_non_trailing_encoded_bytes_preserve_scatter_gather_reference() {
    let passthrough = [2, 0x44, 0x55, 0x66];
    let value = Framed {
        id: 9,
        payload: Payload::Incoming(&passthrough),
    };

    let plan = to_scatter_plan(&value).expect("scatter plan should serialize");
    let expected = to_vec(&value).expect("regular serialization should succeed");

    assert_eq!(plan.total_size(), expected.len());
    assert_eq!(flatten(&plan), expected);

    let has_payload_ref = plan.segments().iter().any(|segment| match segment {
        Segment::Reference { bytes } => {
            bytes.len() == passthrough.len() && bytes.as_ptr() == passthrough.as_ptr()
        }
        Segment::Staged { .. } => false,
    });
    assert!(
        has_payload_ref,
        "expected a scatter-gather reference segment for passthrough payload bytes"
    );
}

#[test]
fn issue_2118_trailing_encoded_bytes_omit_outer_framing() {
    let passthrough = [2, 0x10, 0x20];
    let value = FramedTrailing {
        id: 11,
        payload: Payload::Incoming(&passthrough),
    };

    let bytes = to_vec(&value).expect("serialization should succeed");
    assert_eq!(bytes, vec![11, 2, 0x10, 0x20]);
}

#[test]
fn issue_2118_roundtrip_deserialize_incoming_opaque_payload() {
    let bytes = vec![5, 4, 3, 0xAB, 0xCD, 0xEF];

    let decoded_borrowed: Framed<'_> =
        from_slice_borrowed(&bytes).expect("borrowed deserialization should succeed");
    match decoded_borrowed.payload {
        Payload::RawBorrowed(slice) => {
            assert_eq!(slice, &[3, 0xAB, 0xCD, 0xEF]);
            assert_eq!(slice.as_ptr(), bytes[2..].as_ptr());
        }
        other => panic!("expected RawBorrowed, got {other:?}"),
    }

    let decoded_owned: Framed<'static> =
        from_slice(&bytes).expect("owned deserialization should succeed");
    match decoded_owned.payload {
        Payload::RawOwned(buf) => assert_eq!(buf, vec![3, 0xAB, 0xCD, 0xEF]),
        other => panic!("expected RawOwned, got {other:?}"),
    }
}
