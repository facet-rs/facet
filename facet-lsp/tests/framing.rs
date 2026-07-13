use std::io::{BufReader, Cursor};

use facet::Facet;
use facet_lsp::framing::{
    IncomingMessage, NotificationMessage, RequestMessage, RpcId, frame, read_frame, read_message,
};
use facet_lsp::types::InitializedParams;
use facet_testhelpers::test;

#[derive(Debug, Facet)]
struct Small {
    value: u32,
}

#[test]
fn frames_are_byte_exact() {
    let msg = RequestMessage::new(1, "small", Small { value: 7 });
    let got = frame(&msg).expect("frame");
    assert_eq!(
        got,
        b"Content-Length: 62\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"small\",\"params\":{\"value\":7}}"
    );
}

#[test]
fn reads_one_message_body_with_headers() {
    let bytes =
        b"Content-Length: 33\r\nX-Test: ok\r\n\r\n{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}";
    let mut reader = BufReader::new(Cursor::new(bytes));
    let body = read_frame(&mut reader).expect("read").expect("body");
    assert_eq!(body, br#"{"jsonrpc":"2.0","method":"ping"}"#);
}

#[test]
fn decodes_raw_params_without_losing_method() {
    let notification = NotificationMessage::new("initialized", InitializedParams::default());
    let bytes = frame(&notification).expect("frame");
    let mut reader = BufReader::new(Cursor::new(bytes));
    let message = read_message(&mut reader)
        .expect("message")
        .expect("not eof");
    assert_eq!(
        message,
        IncomingMessage {
            jsonrpc: "2.0".to_owned(),
            id: None,
            method: "initialized".to_owned(),
            params: Some(facet_json::RawJson::from_owned("{}".to_owned())),
        }
    );
}

#[test]
fn request_id_can_be_string_or_number() {
    assert_eq!(RpcId::from(9), RpcId::Number(9));
    assert_eq!(RpcId::from("abc"), RpcId::String("abc".to_owned()));
}
