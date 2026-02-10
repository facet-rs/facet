use facet::Facet;
use facet_testattrs as testattrs;

trait HasGenericName {
    const NAME: &'static str;
}

impl HasGenericName for u32 {
    const NAME: &'static str = "u32";
}

#[derive(Facet)]
#[facet(testattrs::generic_name = <S as HasGenericName>::NAME)]
struct GenericNamed<S: HasGenericName> {
    marker: core::marker::PhantomData<S>,
}

#[test]
fn extension_attr_optional_string_payload_on_generic_container() {
    let shape = GenericNamed::<u32>::SHAPE;
    let attr = shape
        .attributes
        .iter()
        .find(|a| a.ns == Some("testattrs") && a.key == "generic_name")
        .expect("generic optional-string extension attribute should be present");

    let typed = attr
        .get_as::<testattrs::Attr>()
        .expect("attribute payload should decode as testattrs::Attr");
    match typed {
        testattrs::Attr::GenericName(Some(name)) => assert_eq!(*name, "u32"),
        other => panic!("unexpected payload: {other:?}"),
    }
}
