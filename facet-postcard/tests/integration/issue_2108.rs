use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst};
use facet_postcard::{Segment, from_slice, from_slice_borrowed, to_scatter_plan, to_vec};

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
            Payload::Borrowed(bytes) => OpaqueSerialize::Mapped {
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
fn issue_2108_trailing_opaque_omits_outer_length_framing() {
    let value = FramedTrailing {
        id: 7,
        payload: Payload::Borrowed(&[0x10, 0x20]),
    };

    let bytes = to_vec(&value).expect("serialization should succeed");
    assert_eq!(bytes, vec![7, 2, 0x10, 0x20]);
}

#[test]
fn issue_2108_trailing_opaque_deserialize_consumes_remaining_bytes() {
    let bytes = vec![7, 2, 0xAB, 0xCD];

    let decoded_borrowed: FramedTrailing<'_> =
        from_slice_borrowed(&bytes).expect("borrowed deserialization should succeed");
    match decoded_borrowed.payload {
        Payload::RawBorrowed(slice) => {
            assert_eq!(slice, &[2, 0xAB, 0xCD]);
            assert_eq!(slice.as_ptr(), bytes[1..].as_ptr());
        }
        other => panic!("expected RawBorrowed, got {other:?}"),
    }

    let decoded_owned: FramedTrailing<'static> =
        from_slice(&bytes).expect("owned deserialization should succeed");
    match decoded_owned.payload {
        Payload::RawOwned(buf) => assert_eq!(buf, vec![2, 0xAB, 0xCD]),
        other => panic!("expected RawOwned, got {other:?}"),
    }
}

#[test]
fn issue_2108_trailing_opaque_preserves_scatter_gather_references() {
    let payload = [0x44, 0x55, 0x66];
    let value = FramedTrailing {
        id: 9,
        payload: Payload::Borrowed(&payload),
    };

    let plan = to_scatter_plan(&value).expect("scatter plan should serialize");
    let expected = to_vec(&value).expect("regular serialization should succeed");

    assert_eq!(plan.total_size(), expected.len());
    assert_eq!(flatten(&plan), expected);

    let has_payload_ref = plan.segments().iter().any(|segment| match segment {
        Segment::Reference { bytes } => {
            bytes.len() == payload.len() && bytes.as_ptr() == payload.as_ptr()
        }
        Segment::Staged { .. } => false,
    });
    assert!(
        has_payload_ref,
        "expected a scatter-gather reference segment for borrowed payload bytes"
    );
}
