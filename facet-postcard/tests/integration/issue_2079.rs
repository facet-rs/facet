use facet::Facet;
use facet_postcard::{SerializeError, from_slice_borrowed, to_vec, to_vec_with_shape};

#[derive(Debug, Facet, PartialEq)]
struct Envelope<'a> {
    id: u16,
    label: &'a str,
    payload: &'a [u8],
}

fn serialize_envelope<'a>(label: &'a str, payload: &'a [u8]) -> Vec<u8> {
    let envelope = Envelope {
        id: 7,
        label,
        payload,
    };
    to_vec(&envelope).expect("serialization should succeed for borrowed data")
}

fn serialize_envelope_with_shape<'a>(
    label: &'a str,
    payload: &'a [u8],
) -> Result<Vec<u8>, SerializeError> {
    let envelope = Envelope {
        id: 7,
        label,
        payload,
    };
    to_vec_with_shape(&envelope, Envelope::SHAPE)
}

#[test]
fn issue_2079_to_vec_accepts_non_static_borrows() {
    facet_testhelpers::setup();

    let label = String::from("transient-label");
    let payload = vec![9_u8, 8, 7, 6, 5];

    let bytes = serialize_envelope(label.as_str(), payload.as_slice());
    let decoded: Envelope<'_> =
        from_slice_borrowed(&bytes).expect("deserialization should succeed for borrowed fields");

    assert_eq!(decoded.id, 7);
    assert_eq!(decoded.label, label.as_str());
    assert_eq!(decoded.payload, payload.as_slice());
}

#[test]
fn issue_2079_to_vec_with_shape_accepts_non_static_borrows() {
    facet_testhelpers::setup();

    let label = String::from("transient-shape-label");
    let payload = vec![1_u8, 2, 3];

    let err = serialize_envelope_with_shape(label.as_str(), payload.as_slice())
        .expect_err("typed values are not dynamic and should fail shape-based serialization");
    match err {
        SerializeError::Custom(message) => {
            assert!(
                message.contains("DynamicValue"),
                "unexpected error: {message}"
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}
