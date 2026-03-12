use facet::Facet;
use facet_testhelpers::test;

use facet_json::to_vec;

#[test]
fn internally_tagged_struct_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "type")]
    enum Message {
        Request { id: String, method: String },
        Response { id: String, result: String },
    }

    let request = Message::Request {
        id: "1".to_string(),
        method: "ping".to_string(),
    };
    let json = String::from_utf8(to_vec(&request).unwrap()).unwrap();
    assert_eq!(json, r#"{"type":"Request","id":"1","method":"ping"}"#);

    let response = Message::Response {
        id: "1".to_string(),
        result: "pong".to_string(),
    };
    let json = String::from_utf8(to_vec(&response).unwrap()).unwrap();
    assert_eq!(json, r#"{"type":"Response","id":"1","result":"pong"}"#);
}

#[test]
fn internally_tagged_unit_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "status")]
    enum Status {
        Active,
        Inactive,
    }

    let active = Status::Active;
    let json = String::from_utf8(to_vec(&active).unwrap()).unwrap();
    assert_eq!(json, r#"{"status":"Active"}"#);
}

#[test]
fn adjacently_tagged_struct_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "t", content = "c")]
    enum Block {
        Para { text: String },
        Header { level: u8, text: String },
    }

    let para = Block::Para {
        text: "Hello".to_string(),
    };
    let json = String::from_utf8(to_vec(&para).unwrap()).unwrap();
    assert_eq!(json, r#"{"t":"Para","c":{"text":"Hello"}}"#);

    let header = Block::Header {
        level: 2,
        text: "Title".to_string(),
    };
    let json = String::from_utf8(to_vec(&header).unwrap()).unwrap();
    assert_eq!(json, r#"{"t":"Header","c":{"level":2,"text":"Title"}}"#);
}

#[test]
fn adjacently_tagged_tuple_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "type", content = "data")]
    enum Value {
        Str(String),
        Pair(i32, i32),
    }

    let s = Value::Str("hello".to_string());
    let json = String::from_utf8(to_vec(&s).unwrap()).unwrap();
    assert_eq!(json, r#"{"type":"Str","data":"hello"}"#);

    let pair = Value::Pair(10, 20);
    let json = String::from_utf8(to_vec(&pair).unwrap()).unwrap();
    assert_eq!(json, r#"{"type":"Pair","data":[10,20]}"#);
}

#[test]
fn adjacently_tagged_unit_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "kind", content = "value")]
    enum Signal {
        Start,
        Stop,
    }

    let start = Signal::Start;
    let json = String::from_utf8(to_vec(&start).unwrap()).unwrap();
    assert_eq!(json, r#"{"kind":"Start"}"#);
}
