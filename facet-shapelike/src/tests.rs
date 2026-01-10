use crate::shape_like::ShapeLike;
use facet::Facet;
use facet_args as args;
use facet_kdl as kdl;
use facet_xml as xml;

#[derive(Facet)]
#[repr(C)]
struct TestStruct {
    #[facet(kdl::property)]
    a: u32,
    b: bool,
}

#[derive(Facet)]
struct KdlAttributes {
    #[facet(kdl::child)]
    child: u32,
    #[facet(kdl::children)]
    children: Vec<String>,
    #[facet(kdl::property)]
    prop: String,
    #[facet(kdl::argument)]
    arg: bool,
    #[facet(kdl::arguments)]
    args: Vec<i32>,
    #[facet(kdl::node_name)]
    name: String,
}

#[derive(Facet)]
struct XmlAttributes {
    #[facet(xml::attribute)]
    attr: u32,
    #[facet(xml::element)]
    elem: String,
    #[facet(xml::text)]
    text: String,
}

#[derive(Facet)]
struct ArgsAttributes {
    #[facet(args::positional)]
    pos: String,
    #[facet(args::named)]
    named: bool,
    // FIXME: https://github.com/facet-rs/facet/issues/1732
    // #[facet(args::short = 'f')]
    // flag: bool,
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
fn test_kdl_attributes_roundtrip() {
    let shape = KdlAttributes::SHAPE;
    let shape_like: ShapeLike = shape.into();
    let json = facet_json::to_string(&shape_like).expect("Failed to serialize ShapeLike");
    let deserialized: ShapeLike =
        facet_json::from_str(&json).expect("Failed to deserialize ShapeLike");
    facet_assert::assert_same!(shape_like, deserialized)
}

#[test]
fn test_xml_attributes_roundtrip() {
    let shape = XmlAttributes::SHAPE;
    let shape_like: ShapeLike = shape.into();
    let json = facet_json::to_string(&shape_like).expect("Failed to serialize ShapeLike");
    let deserialized: ShapeLike =
        facet_json::from_str(&json).expect("Failed to deserialize ShapeLike");
    facet_assert::assert_same!(shape_like, deserialized)
}

#[test]
fn test_args_attributes_roundtrip() {
    facet_testhelpers::setup();
    let shape = ArgsAttributes::SHAPE;
    let shape_like: ShapeLike = shape.into();
    let json = facet_json::to_string(&shape_like).expect("Failed to serialize ShapeLike");
    let deserialized: ShapeLike =
        facet_json::from_str(&json).expect("Failed to deserialize ShapeLike");
    facet_assert::assert_same!(shape_like, deserialized)
}
