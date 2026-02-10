use facet::Facet;
use facet_testattrs as testattrs;

#[derive(Facet)]
#[facet(testattrs::generic_size = core::mem::size_of::<S>())]
struct Predict<S> {
    marker: core::marker::PhantomData<S>,
}

#[test]
fn extension_attr_payload_on_generic_container() {
    let shape = Predict::<u64>::SHAPE;
    let attr = shape
        .attributes
        .iter()
        .find(|a| a.ns == Some("testattrs") && a.key == "generic_size")
        .expect("generic extension attribute should be present on the container");
    let typed = attr
        .get_as::<testattrs::Attr>()
        .expect("attribute payload should decode as testattrs::Attr");
    match typed {
        testattrs::Attr::GenericSize(Some(n)) => assert_eq!(*n, core::mem::size_of::<u64>()),
        other => panic!("unexpected payload: {other:?}"),
    }
}
