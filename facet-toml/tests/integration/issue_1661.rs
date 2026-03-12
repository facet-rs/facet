//! Regression tests for issue #1661: proxies inside enum variants.

use facet::Facet;

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct IntAsObject {
    inner: String,
}

impl TryFrom<IntAsObject> for i32 {
    type Error = std::num::ParseIntError;

    fn try_from(proxy: IntAsObject) -> Result<Self, Self::Error> {
        proxy.inner.parse()
    }
}

impl From<&i32> for IntAsObject {
    fn from(value: &i32) -> Self {
        Self {
            inner: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum ProxyEnumField {
    Value(#[facet(proxy = IntAsObject)] i32),
}

#[test]
fn enum_variant_field_proxy_deserializes() {
    let input = r#"
[Value]
inner = "99"
"#;

    let got = facet_toml::from_str::<ProxyEnumField>(input).expect("deserialize");
    assert_eq!(got, ProxyEnumField::Value(99));
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[facet(proxy = IntAsObject)]
struct ProxyI32(i32);

impl TryFrom<IntAsObject> for ProxyI32 {
    type Error = std::num::ParseIntError;

    fn try_from(proxy: IntAsObject) -> Result<Self, Self::Error> {
        Ok(Self(proxy.inner.parse()?))
    }
}

impl From<&ProxyI32> for IntAsObject {
    fn from(value: &ProxyI32) -> Self {
        Self {
            inner: value.0.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum ProxyEnumContainer {
    Value(ProxyI32),
}

#[test]
fn enum_variant_container_proxy_deserializes() {
    let input = r#"
[Value]
inner = "77"
"#;

    let got = facet_toml::from_str::<ProxyEnumContainer>(input).expect("deserialize");
    assert_eq!(got, ProxyEnumContainer::Value(ProxyI32(77)));
}
