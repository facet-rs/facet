use std::collections::BTreeSet;

use facet::Facet;
use facet_xml as xml;

// proxy for elements that are actually a btree set
#[derive(Debug, Facet)]
#[facet(transparent)]
pub(crate) struct VecSet<T>(Vec<T>);

impl<T> From<BTreeSet<T>> for VecSet<T> {
    fn from(value: BTreeSet<T>) -> Self {
        Self(value.into_iter().collect())
    }
}

impl<T> From<VecSet<T>> for BTreeSet<T>
where
    T: Ord,
{
    fn from(value: VecSet<T>) -> Self {
        value.0.into_iter().collect()
    }
}

impl<T> From<&BTreeSet<T>> for VecSet<T>
where
    T: Ord + Clone,
{
    fn from(value: &BTreeSet<T>) -> Self {
        Self(value.iter().cloned().collect())
    }
}

#[derive(Debug, Facet, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct Property {
    #[facet(xml::attribute)]
    name: String,
    #[facet(xml::element)]
    value: String,
}

#[derive(Debug, Facet)]
struct Object {
    #[facet(xml::elements, xml::proxy = VecSet<Property>)]
    elements: BTreeSet<Property>,
}

#[test]
fn parse_elements_btree_set() {
    let xml = r#"
<object>
    <property name="foo">test123</property>
    <property name="foo">test123</property>
    <property name="bar">321test</property>
</object>
    "#;

    let object: Object = facet_xml::from_str(xml).unwrap();
    assert_eq!(object.elements.len(), 2);
    assert!(object.elements.contains(&Property {
        name: "foo".to_string(),
        value: "test123".to_string()
    }));
    assert!(object.elements.contains(&Property {
        name: "bar".to_string(),
        value: "321test".to_string()
    }));
}
