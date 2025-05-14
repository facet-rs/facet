use eyre::Result;
use facet::Facet;
use facet_json::{from_str, to_string};
use insta::assert_snapshot;
use std::num::NonZero;

#[test]
fn read_nonzero_one() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Foo {
        foo: NonZero<u64>,
    }
    let json = r#"{"foo": 1}"#;
    let s: Foo = from_str(json)?;
    assert_eq!(s.foo, { const { NonZero::new(1).unwrap() } });
    Ok(())
}

#[test]
fn read_nonzero_zero() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Foo {
        foo: NonZero<u64>,
    }
    let json = r#"{"foo": 0}"#;
    let result: Result<Foo> = from_str(json).map_err(Into::into);
    assert!(result.is_err());
    #[cfg(not(miri))]
    assert_snapshot!(result.unwrap_err().to_string());
    Ok(())
}

#[test]
fn write_nonzero() {
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
