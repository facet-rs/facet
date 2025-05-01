use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[test]
fn test_from_msgpack_with_option() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Options {
        name: Option<String>,
        age: Option<u32>,
        inner: Option<Inner>,
    }

    #[derive(Facet)]
    struct Inner {
        foo: i32,
    }

    // MessagePack data for:
    // { name: "Alice", age: null, inner: { foo: 42 } }
    let data = [
        0x83, // Map with 3 elements
        // name: "Alice"
        0xa4, 0x6e, 0x61, 0x6d, 0x65, // "name"
        0xa5, 0x41, 0x6c, 0x69, 0x63, 0x65, // "Alice"
        // age: null
        0xa3, 0x61, 0x67, 0x65, // "age"
        0xc0, // null
        // inner: { foo: 42 }
        0xa5, 0x69, 0x6e, 0x6e, 0x65, 0x72, // "inner"
        0x81, // Map with 1 element
        0xa3, 0x66, 0x6f, 0x6f, // "foo"
        0x2a, // 42 (positive fixint)
    ];

    let test_struct: Options = from_slice(&data)?;
    assert_eq!(test_struct.name.as_deref(), Some("Alice"));
    assert_eq!(test_struct.age, None);
    assert_eq!(test_struct.inner.as_ref().map(|i| i.foo), Some(42));

    Ok(())
}
