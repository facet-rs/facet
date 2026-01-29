use facet::Facet;
use facet_testhelpers::test;
use facet_xml as xml;

#[derive(Debug, Facet)]
#[repr(C)]
enum TypeProxy {
    Foo {
        #[facet(xml::attribute)]
        value: String,
    },
    Bar {
        #[facet(xml::attribute)]
        value: String,
    },
}

#[derive(Debug, Facet)]
#[facet(xml::proxy = TypeProxy)]
struct Type {
    // just to show the data is represented differently
    is_foo: bool,
    value: String,
}
// XML proxy conversion: u32 <-> binary string
impl From<&TypeProxy> for Type {
    fn from(proxy: &TypeProxy) -> Self {
        match proxy {
            TypeProxy::Foo { value } => Self {
                is_foo: true,
                value: value.to_owned(),
            },
            TypeProxy::Bar { value } => Self {
                is_foo: false,
                value: value.to_owned(),
            },
        }
    }
}

impl From<TypeProxy> for Type {
    fn from(proxy: TypeProxy) -> Self {
        match proxy {
            TypeProxy::Foo { value } => Self {
                is_foo: true,
                value,
            },
            TypeProxy::Bar { value } => Self {
                is_foo: false,
                value,
            },
        }
    }
}

impl From<Type> for TypeProxy {
    fn from(value: Type) -> Self {
        if value.is_foo {
            Self::Foo { value: value.value }
        } else {
            Self::Bar { value: value.value }
        }
    }
}

impl From<&Type> for TypeProxy {
    fn from(value: &Type) -> Self {
        if value.is_foo {
            Self::Foo {
                value: value.value.to_owned(),
            }
        } else {
            Self::Bar {
                value: value.value.to_owned(),
            }
        }
    }
}

#[derive(Debug, Facet)]
struct Container {
    #[facet(xml::elements)]
    elements: Vec<Type>,
}

#[test]
fn elements_collection_uses_proxy_struct_enum_single() {
    let proxy_used: Type = facet_xml::from_str(
        r#"
<foo value="proxies are annoying" />"#,
    )
    .unwrap();
    // works
    assert!(proxy_used.is_foo);
    assert_eq!(proxy_used.value, "proxies are annoying");
}

#[test]
fn elements_collection_uses_proxy_struct_enum() {
    let proxy_used: Container = facet_xml::from_str(
        r#"
<container>
    <foo value="proxies are annoying" />
    <bar value="i like bars" />
</container>"#,
    )
    .unwrap();
    assert!(
        !proxy_used.elements.is_empty(),
        "container contains two valid elements"
    );
    // works
    let foo = &proxy_used.elements[0];
    let bar = &proxy_used.elements[1];
    assert!(foo.is_foo);
    assert_eq!(foo.value, "proxies are annoying");
    assert!(!bar.is_foo);
    assert_eq!(bar.value, "i like bars");
}
