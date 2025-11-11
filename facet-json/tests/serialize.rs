use facet::Facet;
use facet_testhelpers::test;

#[test]
fn test_custom_serialization_struct() {
    struct OpaqueType(u64);

    #[derive(Facet)]
    #[facet(transparent)]
    struct Wrapper(String);

    #[derive(Facet)]
    struct OtherStruct {
        val: u64,
    }

    fn u64_into_str(val: &u64) -> Result<String, &'static str> {
        Ok(format!("0x{val:x}"))
    }

    fn opaque_type_into_str(o: &OpaqueType) -> Result<String, &'static str> {
        u64_into_str(&o.0)
    }

    fn opaque_type_into_wrapper(o: &OpaqueType) -> Result<Wrapper, &'static str> {
        Ok(Wrapper(opaque_type_into_str(&o)?))
    }

    fn opaque_type_into_nested(o: &OpaqueType) -> Result<OtherStruct, &'static str> {
        Ok(OtherStruct { val: o.0 })
    }

    fn arc_u64_into_nested(val: &std::sync::Arc<u64>) -> Result<OtherStruct, &'static str> {
        Ok(OtherStruct { val: **val })
    }

    #[derive(Facet)]
    struct MyType {
        #[facet(opaque, serialize_with = opaque_type_into_str)]
        str: OpaqueType,
        #[facet(opaque, serialize_with = opaque_type_into_wrapper)]
        wrap: OpaqueType,
        #[facet(opaque, serialize_with = opaque_type_into_nested)]
        nest: OpaqueType,
        #[facet(serialize_with = u64_into_str)]
        cust: u64,
        #[facet(serialize_with = arc_u64_into_nested)]
        arc: std::sync::Arc<u64>,
    }

    let data = MyType {
        str: OpaqueType(2748),
        wrap: OpaqueType(2748),
        nest: OpaqueType(8472),
        cust: 2748,
        arc: std::sync::Arc::new(3342),
    };

    let ser = facet_json::to_string(&data);

    let expected =
        r#"{"str":"0xabc","wrap":"0xabc","nest":{"val":8472},"cust":"0xabc","arc":{"val":3342}}"#;

    assert_eq!(ser, expected);
}

#[test]
fn test_custom_serialization_enum() {
    struct OpaqueType(u64);

    fn u64_into_str(val: &u64) -> Result<String, &'static str> {
        Ok(format!("0x{val:x}"))
    }

    fn opaque_type_into_str(o: &OpaqueType) -> Result<String, &'static str> {
        u64_into_str(&o.0)
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    enum MyEnum {
        OpStrTuple(#[facet(opaque, serialize_with = opaque_type_into_str)] OpaqueType),
        OpStrField {
            #[facet(opaque, serialize_with = opaque_type_into_str)]
            field: OpaqueType,
        },
    }

    let data = MyEnum::OpStrTuple(OpaqueType(2748));
    let expected = r#"{"OpStrTuple":"0xabc"}"#;
    let ser = facet_json::to_string(&data);
    assert_eq!(ser, expected);

    let data = MyEnum::OpStrField {
        field: OpaqueType(2748),
    };
    let expected = r#"{"OpStrField":{"field":"0xabc"}}"#;
    let ser = facet_json::to_string(&data);
    assert_eq!(ser, expected);
}
