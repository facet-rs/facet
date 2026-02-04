#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::FormatDeserializer;
use facet_json::JsonParser;
use facet_testhelpers::test;

#[derive(Facet, Debug, PartialEq)]
struct FlattenInner {
    foo: u8,
    #[facet(default)]
    color_code: u8,
}

#[derive(Facet, Debug, PartialEq)]
struct FlattenOuter {
    #[facet(flatten)]
    inner: FlattenInner,
}

#[test]
fn flatten_default_field_missing_format_deserializer() {
    let input = br#"{"foo":1}"#;

    let mut parser = JsonParser::<false>::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);
    let value: FlattenOuter = de
        .deserialize_root()
        .expect("format deserializer should fill defaults inside flatten");

    assert_eq!(
        value,
        FlattenOuter {
            inner: FlattenInner {
                foo: 1,
                color_code: 0
            }
        }
    );
}
