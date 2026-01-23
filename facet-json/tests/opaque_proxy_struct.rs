// Test opaque proxy on struct fields (this should already work)

use facet::Facet;

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

impl TryFrom<&OpaqueType> for OpaqueTypeProxy {
    type Error = std::convert::Infallible;
    fn try_from(opaque: &OpaqueType) -> Result<Self, Self::Error> {
        Ok(OpaqueTypeProxy {
            value: opaque.value.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TestStruct {
    #[facet(opaque, proxy = OpaqueTypeProxy)]
    pub field: OpaqueType,
}

#[test]
fn test_struct_opaque_proxy_serializes() {
    let test = TestStruct {
        field: OpaqueType {
            value: "test".to_string(),
        },
    };
    let json = facet_json::to_string(&test).expect("should serialize");
    assert_eq!(json, r#"{"field":{"value":"test"}}"#);
}
