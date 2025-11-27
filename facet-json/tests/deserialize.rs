use std::num::IntErrorKind;

use facet::Facet;
use facet_testhelpers::test;

#[test]
fn test_custom_deserialization_struct() {
    struct OpaqueType(u64);

    #[derive(Facet)]
    #[facet(transparent)]
    struct Wrapper(String);

    #[allow(clippy::ptr_arg)]
    fn u64_from_str(s: &String) -> Result<u64, &'static str> {
        if let Some(hex) = s.strip_prefix("0x") {
            u64::from_str_radix(hex, 16)
        } else {
            s.parse::<u64>()
        }
        .map_err(|e| match e.kind() {
            IntErrorKind::Empty => "cannot parse integer from empty string",
            IntErrorKind::InvalidDigit => "invalid digit found in string",
            IntErrorKind::PosOverflow => "number too large to fit in target type",
            IntErrorKind::NegOverflow => "number too small to fit in target type",
            IntErrorKind::Zero => "number would be zero for non-zero type",
            _ => "unknown error",
        })
    }

    #[allow(clippy::ptr_arg)]
    fn opaque_type_from_str(s: &String) -> Result<OpaqueType, &'static str> {
        Ok(OpaqueType(u64_from_str(s)?))
    }

    fn opaque_type_from_wrapper(w: &Wrapper) -> Result<OpaqueType, &'static str> {
        opaque_type_from_str(&w.0)
    }

    #[derive(Facet)]
    struct OtherStruct {
        val: u64,
    }

    fn opaque_type_from_nested(o: &OtherStruct) -> Result<OpaqueType, &'static str> {
        Ok(OpaqueType(o.val))
    }

    fn arc_from_nested(o: &OtherStruct) -> Result<std::sync::Arc<u64>, &'static str> {
        Ok(std::sync::Arc::new(o.val))
    }

    #[derive(Facet)]
    struct MyType {
        #[facet(opaque, deserialize_with = opaque_type_from_str)]
        str: OpaqueType,
        #[facet(opaque, deserialize_with = opaque_type_from_wrapper)]
        wrap: OpaqueType,
        #[facet(opaque, deserialize_with = opaque_type_from_nested)]
        nest: OpaqueType,
        #[facet(deserialize_with = u64_from_str)]
        cust: u64,
        #[facet(deserialize_with = arc_from_nested)]
        arc: std::sync::Arc<u64>,
    }

    let data =
        r#"{"str":"0xabc","wrap":"0xabc","nest":{"val": 8472},"cust":"0xabc","arc":{"val": 3342}}"#;

    let test: MyType = facet_json::from_str(data).unwrap();
    assert_eq!(test.str.0, 2748);
    assert_eq!(test.wrap.0, 2748);
    assert_eq!(test.nest.0, 8472);
    assert_eq!(test.cust, 2748);
    assert_eq!(*test.arc, 3342);
}

#[test]
fn test_custom_deserialization_enum() {
    struct OpaqueType(u64);

    #[allow(clippy::ptr_arg)]
    fn u64_from_str(s: &String) -> Result<u64, &'static str> {
        if let Some(hex) = s.strip_prefix("0x") {
            u64::from_str_radix(hex, 16)
        } else {
            s.parse::<u64>()
        }
        .map_err(|e| match e.kind() {
            IntErrorKind::Empty => "cannot parse integer from empty string",
            IntErrorKind::InvalidDigit => "invalid digit found in string",
            IntErrorKind::PosOverflow => "number too large to fit in target type",
            IntErrorKind::NegOverflow => "number too small to fit in target type",
            IntErrorKind::Zero => "number would be zero for non-zero type",
            _ => "unknown error",
        })
    }

    #[allow(clippy::ptr_arg)]
    fn opaque_type_from_str(s: &String) -> Result<OpaqueType, &'static str> {
        Ok(OpaqueType(u64_from_str(s)?))
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    enum MyEnum {
        OpStrTuple(#[facet(opaque, deserialize_with = opaque_type_from_str)] OpaqueType),
        OpStrField {
            #[facet(opaque, deserialize_with = opaque_type_from_str)]
            field: OpaqueType,
        },
    }

    let data = r#"{"OpStrTuple": "0xabc"}"#;
    let opstr: MyEnum = facet_json::from_str(data).unwrap();
    match opstr {
        MyEnum::OpStrTuple(OpaqueType(v)) => assert_eq!(v, 2748),
        _ => panic!("expected OpStrTuple variant"),
    }

    let data = r#"{"OpStrField": {"field": "0xabc"}}"#;
    let opstr: MyEnum = facet_json::from_str(data).unwrap();
    match opstr {
        MyEnum::OpStrField {
            field: OpaqueType(v),
        } => assert_eq!(v, 2748),
        _ => panic!("expected OpStrField variant"),
    }
}
