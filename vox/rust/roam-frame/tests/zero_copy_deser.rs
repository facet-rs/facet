//! Tests for zero-copy deserialization with `OwnedMessage`.

use std::borrow::Cow;

use facet::Facet;
use rapace_frame::{Frame, MsgDesc, OwnedMessage};

#[derive(Debug, PartialEq, Facet)]
struct BorrowingResponse<'a> {
    message: Cow<'a, str>,
    data: &'a [u8],
}

#[derive(Debug, PartialEq, Facet)]
struct OwnedResponse {
    message: String,
    count: u32,
}

fn make_owned_frame(payload: &[u8]) -> Frame {
    let desc = MsgDesc::new(2, 0, 0);
    Frame::with_owned_payload(desc, payload.to_vec())
}

fn make_inline_frame(payload: &[u8]) -> Frame {
    let desc = MsgDesc::new(2, 0, 0);
    Frame::with_inline_payload(desc, payload).expect("inline payload")
}

#[test]
fn owned_message_can_borrow_from_owned_payload() {
    let original = BorrowingResponse {
        message: Cow::Borrowed("hello world"),
        data: b"binary data",
    };

    let bytes = facet_postcard::to_vec(&original).expect("serialize");
    let frame = make_owned_frame(&bytes);

    let owned: OwnedMessage<BorrowingResponse<'static>> =
        OwnedMessage::try_new(frame, facet_postcard::from_slice_borrowed).expect("deserialize");

    assert_eq!(&*owned.message, "hello world");
    assert_eq!(owned.data, b"binary data");
    assert!(matches!(owned.message, Cow::Borrowed(_)));
}

#[test]
fn owned_message_can_borrow_from_inline_payload() {
    let original = BorrowingResponse {
        message: Cow::Borrowed("hi"),
        data: b"ok",
    };

    let bytes = facet_postcard::to_vec(&original).expect("serialize");
    assert!(bytes.len() <= rapace_frame::INLINE_PAYLOAD_LEN);
    let frame = make_inline_frame(&bytes);
    let owned: OwnedMessage<BorrowingResponse<'static>> =
        OwnedMessage::try_new(frame, facet_postcard::from_slice_borrowed).expect("deserialize");

    assert_eq!(&*owned.message, "hi");
    assert_eq!(owned.data, b"ok");
}

#[test]
fn owned_message_into_frame_preserves_payload() {
    let original = BorrowingResponse {
        message: Cow::Borrowed("test"),
        data: b"data",
    };
    let bytes = facet_postcard::to_vec(&original).expect("serialize");
    let frame = make_owned_frame(&bytes);
    let original_len = frame.payload_bytes().len();
    let owned: OwnedMessage<BorrowingResponse<'static>> =
        OwnedMessage::try_new(frame, facet_postcard::from_slice_borrowed).expect("deserialize");
    let recovered = owned.into_frame();
    assert_eq!(recovered.payload_bytes().len(), original_len);
}

#[test]
fn owned_message_works_with_owned_types_too() {
    let original = OwnedResponse {
        message: "test string".to_string(),
        count: 123,
    };
    let bytes = facet_postcard::to_vec(&original).expect("serialize");
    let frame = make_owned_frame(&bytes);
    let owned: OwnedMessage<OwnedResponse> =
        OwnedMessage::try_new(frame, facet_postcard::from_slice_borrowed).expect("deserialize");

    assert_eq!(owned.message, "test string");
    assert_eq!(owned.count, 123);
}
