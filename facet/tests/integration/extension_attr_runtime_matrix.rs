use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
struct Demo {
    #[facet(testattrs::named)]
    #[facet(testattrs::env_prefix = "APP_")]
    #[facet(testattrs::min = -7)]
    #[facet(testattrs::max_len = 42)]
    #[facet(testattrs::short = 'v')]
    #[facet(testattrs::generic_name = "primary")]
    #[facet(rename = "demo_value")]
    #[facet(sensitive)]
    value: String,
}

fn demo_field() -> &'static facet::Field {
    let Type::User(UserType::Struct(StructType { fields, .. })) = Demo::SHAPE.ty else {
        panic!("expected Demo to be a struct");
    };
    &fields[0]
}

#[test]
fn extension_attr_runtime_decoding_matrix() {
    let field = demo_field();

    let named = field
        .get_attr(Some("testattrs"), "named")
        .expect("named extension attribute must exist");
    assert!(named.get_as::<testattrs::Attr>().is_none());
    assert!(named.get_as::<()>().is_some());

    let env_prefix = field
        .get_attr(Some("testattrs"), "env_prefix")
        .expect("env_prefix extension attribute must exist");
    assert!(env_prefix.get_as::<testattrs::Attr>().is_none());
    assert_eq!(env_prefix.get_as::<&'static str>().copied(), Some("APP_"));

    let min = field
        .get_attr(Some("testattrs"), "min")
        .expect("min extension attribute must exist");
    assert!(min.get_as::<testattrs::Attr>().is_none());
    assert_eq!(min.get_as::<i64>().copied(), Some(-7));

    let max_len = field
        .get_attr(Some("testattrs"), "max_len")
        .expect("max_len extension attribute must exist");
    assert!(max_len.get_as::<testattrs::Attr>().is_none());
    assert_eq!(max_len.get_as::<usize>().copied(), Some(42));

    let short = field
        .get_attr(Some("testattrs"), "short")
        .expect("short extension attribute must exist");
    assert!(short.get_as::<Option<char>>().is_none());
    let short_decoded = short
        .get_as::<testattrs::Attr>()
        .expect("short should decode through testattrs::Attr");
    assert!(matches!(short_decoded, testattrs::Attr::Short(Some('v'))));

    let maybe_name = field
        .get_attr(Some("testattrs"), "generic_name")
        .expect("generic_name extension attribute must exist");
    let maybe_name_decoded = maybe_name
        .get_as::<testattrs::Attr>()
        .expect("generic_name should decode through testattrs::Attr");
    assert!(matches!(
        maybe_name_decoded,
        testattrs::Attr::GenericName(Some("primary"))
    ));
}

#[test]
fn builtin_attrs_use_dedicated_storage() {
    let field = demo_field();

    assert_eq!(field.rename, Some("demo_value"));
    assert!(field.is_sensitive());
}
