#![cfg(feature = "std")]

use std::num::NonZero;

use facet::Facet;
use facet_json2::to_string;

#[test]
fn test_nonzero() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Foo {
        foo: NonZero<u8>,
    }

    let test_struct = Foo {
        foo: const { NonZero::new(1).unwrap() },
    };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"foo":1}"#);
}
