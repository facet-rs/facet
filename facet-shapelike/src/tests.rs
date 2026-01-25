use crate::shape_like::ShapeLike;
use facet::Facet;
use facet_testattrs as testattrs;

#[derive(Facet)]
#[repr(C)]
struct TestStruct {
    a: u32,
    b: bool,
}

#[derive(Facet)]
struct TestAttrsStruct {
    #[facet(testattrs::positional)]
    pos: String,
    #[facet(testattrs::named)]
    named: bool,
}

#[test]
fn test_shape_serialization_roundtrip() {
    let shape = TestStruct::SHAPE;
    let shape_like: ShapeLike = shape.into();
    let json = facet_json::to_string(&shape_like).expect("Failed to serialize ShapeLike");
    let deserialized: ShapeLike =
        facet_json::from_str(&json).expect("Failed to deserialize ShapeLike");
    facet_assert::assert_same!(shape_like, deserialized)
}

#[test]
fn test_testattrs_attributes_roundtrip() {
    facet_testhelpers::setup();
    let shape = TestAttrsStruct::SHAPE;
    let shape_like: ShapeLike = shape.into();
    let json = facet_json::to_string(&shape_like).expect("Failed to serialize ShapeLike");
    let deserialized: ShapeLike =
        facet_json::from_str(&json).expect("Failed to deserialize ShapeLike");
    facet_assert::assert_same!(shape_like, deserialized)
}
