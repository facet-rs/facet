use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;
use insta::assert_snapshot;
use std::num::NonZero;

#[test]
fn read_nonzero_one() {
    #[derive(Facet)]
    struct Foo {
        foo: NonZero<u64>,
    }
    let json = r#"{"foo": 1}"#;
    let s: Foo = from_str(json)?;
    assert_eq!(s.foo, { const { NonZero::new(1).unwrap() } });
}

#[test]
fn read_nonzero_zero() {
    #[derive(Facet, Debug)]
    struct Foo {
        foo: NonZero<u64>,
    }
    let json = r#"{"foo": 0}"#;
    let result = from_str::<Foo>(json);
    assert!(result.is_err());
    #[cfg(not(miri))]
    assert_snapshot!(result.unwrap_err().to_string());
}

#[test]
fn write_nonzero() {
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
