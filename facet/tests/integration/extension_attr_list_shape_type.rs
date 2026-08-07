use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
struct Findable {
    #[facet(testattrs::lenient_width(f32, i32))]
    findable: i32,
}

fn findable_field() -> &'static facet::Field {
    let Type::User(UserType::Struct(StructType { fields, .. })) = Findable::SHAPE.ty else {
        panic!("expected Findable to be a struct");
    };
    &fields[0]
}

#[test]
fn list_shape_type_decodes_to_shape_slice() {
    let field = findable_field();

    let lenient_width = field
        .get_attr(Some("testattrs"), "lenient_width")
        .expect("lenient_width extension attribute must exist");

    let decoded = lenient_width
        .get_as::<testattrs::Attr>()
        .expect("lenient_width should decode through testattrs::Attr");

    let testattrs::Attr::LenientWidth(shapes) = decoded else {
        panic!("expected Attr::LenientWidth, got {decoded:?}");
    };

    assert_eq!(shapes.len(), 2);
    assert_eq!(
        shapes[0].type_identifier,
        <f32 as Facet>::SHAPE.type_identifier
    );
    assert_eq!(
        shapes[1].type_identifier,
        <i32 as Facet>::SHAPE.type_identifier
    );
    assert!(core::ptr::eq(shapes[0], <f32 as Facet>::SHAPE));
    assert!(core::ptr::eq(shapes[1], <i32 as Facet>::SHAPE));
}
