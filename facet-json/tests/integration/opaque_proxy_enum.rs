// Test for issue #1873: Enum variant with #[facet(opaque, proxy = ...)] fails to serialize

use facet::Facet;
use facet_testhelpers::test;
use tendril::StrTendril;

// Simple opaque type that doesn't implement Facet
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueType {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct OpaqueTypeProxy {
    pub value: String,
}

impl TryFrom<OpaqueTypeProxy> for OpaqueType {
    type Error = &'static str;
    fn try_from(proxy: OpaqueTypeProxy) -> Result<Self, Self::Error> {
        Ok(OpaqueType { value: proxy.value })
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<&OpaqueType> for OpaqueTypeProxy {
    type Error = std::convert::Infallible;
    fn try_from(opaque: &OpaqueType) -> Result<Self, Self::Error> {
        Ok(OpaqueTypeProxy {
            value: opaque.value.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PropKey {
    Text,
    Attr(#[facet(opaque, proxy = OpaqueTypeProxy)] OpaqueType),
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PropKeyStruct {
    Text,
    Attr {
        #[facet(opaque, proxy = OpaqueTypeProxy)]
        opaque: OpaqueType,
    },
}

// Also test with StrTendril from the original issue
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TendrilProxy {
    pub value: String,
}

impl TryFrom<TendrilProxy> for StrTendril {
    type Error = &'static str;
    fn try_from(proxy: TendrilProxy) -> Result<Self, Self::Error> {
        Ok(StrTendril::from(proxy.value))
    }
}

#[allow(clippy::infallible_try_from)]
impl TryFrom<&StrTendril> for TendrilProxy {
    type Error = std::convert::Infallible;
    fn try_from(tendril: &StrTendril) -> Result<Self, Self::Error> {
        Ok(TendrilProxy {
            value: tendril.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TendrilKey {
    Text,
    Attr(#[facet(opaque, proxy = TendrilProxy)] StrTendril),
}

#[test]
fn test_simple_opaque_proxy_serializes_alone() {
    let opaque = OpaqueType {
        value: "test".to_string(),
    };
    let proxy = OpaqueTypeProxy::try_from(&opaque).expect("works");
    let json = facet_json::to_string(&proxy).expect("proxy serializes fine");
    assert_eq!(json, r#"{"value":"test"}"#);
}

#[test]
fn test_simple_opaque_proxy_in_enum_tuple_variant() {
    let opaque = OpaqueType {
        value: "class".to_string(),
    };
    let prop_key = PropKey::Attr(opaque);
    let json = facet_json::to_string(&prop_key)
        .expect("enum tuple variant with opaque proxy field should serialize");
    // Should serialize as: {"Attr": {"value":"class"}}
    assert_eq!(json, r#"{"Attr":{"value":"class"}}"#);
}

#[test]
fn test_simple_opaque_proxy_in_enum_struct_variant() {
    let opaque = OpaqueType {
        value: "class".to_string(),
    };
    let prop_key = PropKeyStruct::Attr { opaque };
    let json = facet_json::to_string(&prop_key)
        .expect("enum struct variant with opaque proxy field should serialize");
    // Should serialize as: {"Attr": {"opaque": {"value":"class"}}}
    assert_eq!(json, r#"{"Attr":{"opaque":{"value":"class"}}}"#);
}

#[test]
fn test_enum_unit_variant() {
    let prop_key = PropKey::Text;
    let json = facet_json::to_string(&prop_key).expect("unit variant should serialize");
    assert_eq!(json, r#""Text""#);
}

#[test]
fn test_tendril_proxy_serializes_alone() {
    let tendril = StrTendril::from("class");
    let proxy = TendrilProxy::try_from(&tendril).expect("works");
    let json = facet_json::to_string(&proxy).expect("proxy serializes fine");
    assert_eq!(json, r#"{"value":"class"}"#);
}

#[test]
fn test_tendril_opaque_proxy_in_enum_variant() {
    let tendril = StrTendril::from("class");
    let prop_key = TendrilKey::Attr(tendril);
    let json =
        facet_json::to_string(&prop_key).expect("enum with opaque proxy field should serialize");
    // Should serialize as: {"Attr": {"value":"class"}}
    assert_eq!(json, r#"{"Attr":{"value":"class"}}"#);
}
