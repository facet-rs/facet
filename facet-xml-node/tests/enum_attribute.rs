use facet::Facet;
use facet_testhelpers::test;
use facet_xml as xml;

#[derive(Facet, Debug)]
#[facet(rename_all = "PascalCase")]
#[repr(C)]
pub enum MyValue {
    Foo,
    Bar,
    #[facet(rename = "BAz")]
    Baz,
}

#[derive(Debug, Facet)]
struct Container {
    #[facet(xml::attribute)]
    value: MyValue,
}

#[test]
fn enum_attribute() {
    let c1 = Container {
        value: MyValue::Foo,
    };
    let el = facet_xml_node::to_element(&c1).unwrap();
    assert_eq!(el.attrs["value"], "Foo");
    let c1: Container = facet_xml_node::from_element(&el).unwrap();
    assert!(matches!(c1.value, MyValue::Foo));

    let c2 = Container {
        value: MyValue::Baz,
    };
    let el = facet_xml_node::to_element(&c2).unwrap();
    assert_eq!(el.attrs["value"], "BAz");
    let c2: Container = facet_xml_node::from_element(&el).unwrap();
    assert!(matches!(c2.value, MyValue::Baz));
}
