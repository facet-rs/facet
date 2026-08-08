use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;
use std::collections::HashMap;

#[derive(Facet)]
struct Findable {
    #[facet(testattrs::lenient_width(f32, i32))]
    findable: i32,
}

#[derive(Facet)]
struct GenericFindable {
    // `HashMap<u8, u8>` has a top-level-looking comma inside its own `<...>` -
    // this proves the list splitter tracks angle-bracket nesting instead of
    // mis-splitting at the comma between `u8` and `u8`.
    #[facet(testattrs::lenient_width(f32, HashMap<u8, u8>))]
    findable: i32,
}

fn findable_field() -> &'static facet::Field {
    let Type::User(UserType::Struct(StructType { fields, .. })) = Findable::SHAPE.ty else {
        panic!("expected Findable to be a struct");
    };
    &fields[0]
}

fn generic_findable_field() -> &'static facet::Field {
    let Type::User(UserType::Struct(StructType { fields, .. })) = GenericFindable::SHAPE.ty else {
        panic!("expected GenericFindable to be a struct");
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

#[test]
fn list_shape_type_splits_generic_types_correctly() {
    let field = generic_findable_field();

    let lenient_width = field
        .get_attr(Some("testattrs"), "lenient_width")
        .expect("lenient_width extension attribute must exist");

    let decoded = lenient_width
        .get_as::<testattrs::Attr>()
        .expect("lenient_width should decode through testattrs::Attr");

    let testattrs::Attr::LenientWidth(shapes) = decoded else {
        panic!("expected Attr::LenientWidth, got {decoded:?}");
    };

    // If the comma inside `HashMap<u8, u8>` were mistaken for a top-level
    // separator, this would decode to three shapes instead of two.
    assert_eq!(shapes.len(), 2);
    assert_eq!(
        shapes[0].type_identifier,
        <f32 as Facet>::SHAPE.type_identifier
    );
    assert_eq!(
        shapes[1].type_identifier,
        <HashMap<u8, u8> as Facet>::SHAPE.type_identifier
    );
    assert!(core::ptr::eq(shapes[0], <f32 as Facet>::SHAPE));
    assert!(core::ptr::eq(shapes[1], <HashMap<u8, u8> as Facet>::SHAPE));
}
