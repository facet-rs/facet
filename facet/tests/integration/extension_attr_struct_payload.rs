use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
struct IndexedUser {
    #[facet(testattrs::column(rename = "user_name", indexed))]
    username: String,
}

fn first_field(shape: &'static facet::Shape) -> &'static facet::Field {
    let Type::User(UserType::Struct(StructType { fields, .. })) = shape.ty else {
        panic!("expected struct");
    };
    &fields[0]
}

#[test]
fn struct_payload_decodes_through_attr_enum() {
    let field = first_field(IndexedUser::SHAPE);
    let attr = field
        .get_attr(Some("testattrs"), "column")
        .expect("column attribute should be present");

    assert!(
        attr.get_as::<testattrs::Column>().is_none(),
        "struct payloads are wrapped in the generated attr enum"
    );

    let decoded = attr
        .get_as::<testattrs::Attr>()
        .expect("column should decode as testattrs::Attr");

    match decoded {
        testattrs::Attr::Column(column) => {
            assert_eq!(column.rename, Some("user_name"));
            assert!(column.indexed);
        }
        other => panic!("unexpected decoded payload: {other:?}"),
    }
}
