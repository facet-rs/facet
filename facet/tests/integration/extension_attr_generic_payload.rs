use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
#[facet(testattrs::generic_size = core::mem::size_of::<S>())]
struct Predict<S> {
    marker: core::marker::PhantomData<S>,
}

#[derive(Facet)]
#[facet(testattrs::generic_size = N)]
struct ConstPredict<const N: usize> {
    marker: core::marker::PhantomData<[u8; N]>,
}

#[derive(Facet)]
struct FieldPredict<S> {
    #[facet(testattrs::generic_size = core::mem::size_of::<S>())]
    value: S,
}

#[derive(Facet)]
struct ConstFieldPredict<const N: usize> {
    #[facet(testattrs::generic_size = N)]
    value: [u8; N],
}

#[derive(Facet)]
struct GenericBuiltinRename<S> {
    #[facet(rename = "renamed_value")]
    value: S,
}

#[derive(Facet)]
#[repr(u8)]
enum VariantPredict<S> {
    #[facet(testattrs::generic_size = core::mem::size_of::<S>())]
    Wrapped(core::marker::PhantomData<S>),
}

#[derive(Facet)]
#[repr(u8)]
enum ConstVariantPredict<const N: usize> {
    #[facet(testattrs::generic_size = N)]
    Wrapped([u8; N]),
}

#[derive(Facet)]
#[repr(u8)]
enum VariantBuiltinRename<S> {
    #[facet(rename = "wrapped_renamed")]
    Wrapped(core::marker::PhantomData<S>),
}

#[derive(Facet)]
#[facet(opaque)]
struct GenericOpaque<S> {
    marker: core::marker::PhantomData<S>,
}

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

fn struct_fields(shape: &'static facet::Shape) -> &'static [facet::Field] {
    let Type::User(UserType::Struct(StructType { fields, .. })) = shape.ty else {
        panic!("expected struct");
    };
    fields
}

fn enum_variants(shape: &'static facet::Shape) -> &'static [facet::Variant] {
    let Type::User(UserType::Enum(enum_def)) = shape.ty else {
        panic!("expected enum");
    };
    enum_def.variants
}

#[test]
fn extension_attr_payload_on_generic_container() {
    assert_eq!(
        read_generic_size(Predict::<u8>::SHAPE.attributes),
        core::mem::size_of::<u8>()
    );
    assert_eq!(
        read_generic_size(Predict::<[u8; 16]>::SHAPE.attributes),
        core::mem::size_of::<[u8; 16]>()
    );
}

#[test]
fn extension_attr_payload_on_const_generic_container() {
    assert_eq!(read_generic_size(ConstPredict::<1>::SHAPE.attributes), 1);
    assert_eq!(read_generic_size(ConstPredict::<32>::SHAPE.attributes), 32);
}

#[test]
fn extension_attr_payload_on_generic_field() {
    let fields = struct_fields(FieldPredict::<u8>::SHAPE);
    assert_eq!(
        read_generic_size(fields[0].attributes),
        core::mem::size_of::<u8>()
    );

    let fields = struct_fields(FieldPredict::<[u8; 8]>::SHAPE);
    assert_eq!(
        read_generic_size(fields[0].attributes),
        core::mem::size_of::<[u8; 8]>()
    );
}

#[test]
fn extension_attr_payload_on_const_generic_field() {
    let fields = struct_fields(ConstFieldPredict::<1>::SHAPE);
    assert_eq!(read_generic_size(fields[0].attributes), 1);

    let fields = struct_fields(ConstFieldPredict::<24>::SHAPE);
    assert_eq!(read_generic_size(fields[0].attributes), 24);
}

#[test]
fn builtin_attr_dispatch_on_generic_field() {
    let fields = struct_fields(GenericBuiltinRename::<u32>::SHAPE);
    assert_eq!(fields[0].name, "value");
    assert_eq!(fields[0].rename, Some("renamed_value"));
}

#[test]
fn builtin_marker_attr_on_generic_container() {
    assert!(GenericOpaque::<u8>::SHAPE.has_builtin_attr("opaque"));
    assert!(GenericOpaque::<[u8; 8]>::SHAPE.has_builtin_attr("opaque"));
}

#[test]
fn extension_attr_payload_on_generic_variant() {
    let variants = enum_variants(VariantPredict::<u16>::SHAPE);
    assert_eq!(
        read_generic_size(variants[0].attributes),
        core::mem::size_of::<u16>()
    );

    let variants = enum_variants(VariantPredict::<[u8; 12]>::SHAPE);
    assert_eq!(
        read_generic_size(variants[0].attributes),
        core::mem::size_of::<[u8; 12]>()
    );
}

#[test]
fn extension_attr_payload_on_const_generic_variant() {
    let variants = enum_variants(ConstVariantPredict::<1>::SHAPE);
    assert_eq!(read_generic_size(variants[0].attributes), 1);

    let variants = enum_variants(ConstVariantPredict::<19>::SHAPE);
    assert_eq!(read_generic_size(variants[0].attributes), 19);
}

#[test]
fn builtin_attr_dispatch_on_generic_variant() {
    let variants = enum_variants(VariantBuiltinRename::<u32>::SHAPE);
    assert_eq!(variants[0].name, "Wrapped");
    assert_eq!(variants[0].rename, Some("wrapped_renamed"));
}
