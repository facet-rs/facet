use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[test]
fn msgpack_deserialize_field_level_default_no_function() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefault {
        foo: i32,
        #[facet(default)]
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    // {"foo": 789}
    let data = [
        0x81, // Map with 1 element
        0xa3, 0x66, 0x6f, 0x6f, // "foo"
        0xcd, 0x03, 0x15, // 789 (uint16)
    ];

    let s: FieldDefault = from_slice(&data)?;
    assert_eq!(s.foo, 789, "Expected foo to be 789, got {}", s.foo);
    assert_eq!(
        s.bar, "",
        "Expected bar to be empty string, got {:?}",
        s.bar
    );
    Ok(())
}

#[test]
fn msgpack_deserialize_field_level_default_function() -> Result<()> {
    facet_testhelpers::setup();

    fn default_number() -> i32 {
        12345
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefaultFn {
        #[facet(default = default_number())]
        foo: i32,
        bar: String,
    }

    // Only set bar, leave foo missing - should use default_number()
    // {"bar": "hello"}
    let data = [
        0x81, // Map with 1 element
        0xa3, 0x62, 0x61, 0x72, // "bar"
        0xa5, 0x68, 0x65, 0x6c, 0x6c, 0x6f, // "hello"
    ];

    let s: FieldDefaultFn = from_slice(&data)?;
    assert_eq!(s.foo, 12345, "Expected foo to be 12345, got {}", s.foo);
    assert_eq!(s.bar, "hello", "Expected bar to be 'hello', got {}", s.bar);
    Ok(())
}
