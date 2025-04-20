use std::num::NonZero;

use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_nonzero() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Foo {
        foo: NonZero<u64>,
    }
    let json = r#"{"foo": 1}"#;
    let s: Foo = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.foo, { const { NonZero::new(1).unwrap() } });
}
