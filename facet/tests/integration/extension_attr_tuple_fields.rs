use facet::{Facet, StructKind, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
#[facet(testattrs::named)]
struct TupleFieldAttrs(
    #[facet(testattrs::short = 't')] u32,
    #[facet(testattrs::positional)] String,
);

#[derive(Facet)]
struct GenericTuplePayload<S>(#[facet(testattrs::generic_size = core::mem::size_of::<S>())] S);

#[derive(Facet)]
struct ConstTuplePayload<const N: usize>(#[facet(testattrs::generic_size = N)] [u8; N]);

fn read_generic_size(attrs: &[facet::Attr]) -> usize {
    let attr = attrs
        .iter()
        .find(|a| a.ns == Some("testattrs") && a.key == "generic_size")
        .expect("generic extension attribute should be present");
    let typed = attr
        .get_as::<testattrs::Attr>()
        .expect("attribute payload should decode as testattrs::Attr");
    match typed {
        testattrs::Attr::GenericSize(Some(n)) => *n,
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn namespaced_extension_attrs_on_tuple_fields() {
    let shape = TupleFieldAttrs::SHAPE;

    assert!(
        shape
            .attributes
            .iter()
            .any(|a| a.ns == Some("testattrs") && a.key == "named"),
        "container-level namespaced attribute should be present"
    );

    let Type::User(UserType::Struct(StructType { kind, fields, .. })) = shape.ty else {
        panic!("expected tuple struct shape");
    };
    assert_eq!(kind, StructKind::TupleStruct);
    assert_eq!(fields.len(), 2);

    let short_attr = fields[0]
        .attributes
        .iter()
        .find(|a| a.ns == Some("testattrs") && a.key == "short")
        .expect("first tuple field should have short attribute");
    let decoded = short_attr
        .get_as::<testattrs::Attr>()
        .expect("short attribute should decode as testattrs::Attr");
    assert!(
        matches!(decoded, testattrs::Attr::Short(Some('t'))),
        "unexpected decoded short attr: {decoded:?}"
    );

    assert!(
        fields[1]
            .attributes
            .iter()
            .any(|a| a.ns == Some("testattrs") && a.key == "positional"),
        "second tuple field should have positional attribute"
    );
}

#[test]
fn generic_payloads_on_tuple_fields() {
    let Type::User(UserType::Struct(StructType { kind, fields, .. })) =
        GenericTuplePayload::<u16>::SHAPE.ty
    else {
        panic!("expected tuple struct");
    };
    assert_eq!(kind, StructKind::TupleStruct);
    assert_eq!(
        read_generic_size(fields[0].attributes),
        core::mem::size_of::<u16>()
    );

    let Type::User(UserType::Struct(StructType { kind, fields, .. })) =
        GenericTuplePayload::<[u8; 10]>::SHAPE.ty
    else {
        panic!("expected tuple struct");
    };
    assert_eq!(kind, StructKind::TupleStruct);
    assert_eq!(
        read_generic_size(fields[0].attributes),
        core::mem::size_of::<[u8; 10]>()
    );
}

#[test]
fn const_generic_payloads_on_tuple_fields() {
    let Type::User(UserType::Struct(StructType { kind, fields, .. })) =
        ConstTuplePayload::<2>::SHAPE.ty
    else {
        panic!("expected tuple struct");
    };
    assert_eq!(kind, StructKind::TupleStruct);
    assert_eq!(read_generic_size(fields[0].attributes), 2);

    let Type::User(UserType::Struct(StructType { kind, fields, .. })) =
        ConstTuplePayload::<21>::SHAPE.ty
    else {
        panic!("expected tuple struct");
    };
    assert_eq!(kind, StructKind::TupleStruct);
    assert_eq!(read_generic_size(fields[0].attributes), 21);
}
