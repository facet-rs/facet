use facet::Facet;
use facet_testhelpers::test;

#[test]
fn test_proxy_serialization_struct() {
    // Target type that doesn't implement Facet
    struct OpaqueType(u64);

    // Proxy type that serializes OpaqueType to a hex string
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct OpaqueTypeStrProxy(String);

    impl TryFrom<OpaqueTypeStrProxy> for OpaqueType {
        type Error = &'static str;
        fn try_from(_proxy: OpaqueTypeStrProxy) -> Result<Self, Self::Error> {
            // Not needed for serialization tests
            Err("not implemented for this test")
        }
    }

    impl From<&OpaqueType> for OpaqueTypeStrProxy {
        fn from(v: &OpaqueType) -> Self {
            OpaqueTypeStrProxy(format!("0x{:x}", v.0))
        }
    }

    // Proxy type that serializes OpaqueType to a nested struct
    #[derive(Facet, Clone)]
    struct OpaqueTypeNestedProxy {
        val: u64,
    }

    impl TryFrom<OpaqueTypeNestedProxy> for OpaqueType {
        type Error = &'static str;
        fn try_from(_proxy: OpaqueTypeNestedProxy) -> Result<Self, Self::Error> {
            Err("not implemented for this test")
        }
    }

    impl From<&OpaqueType> for OpaqueTypeNestedProxy {
        fn from(v: &OpaqueType) -> Self {
            OpaqueTypeNestedProxy { val: v.0 }
        }
    }

    // Proxy for u64 -> hex string
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct U64ToStrProxy(String);

    impl TryFrom<U64ToStrProxy> for u64 {
        type Error = &'static str;
        fn try_from(_proxy: U64ToStrProxy) -> Result<Self, Self::Error> {
            Err("not implemented for this test")
        }
    }

    impl From<&u64> for U64ToStrProxy {
        fn from(v: &u64) -> Self {
            U64ToStrProxy(format!("0x{v:x}"))
        }
    }

    // Proxy for Arc<u64> -> nested struct
    #[derive(Facet, Clone)]
    struct ArcU64Proxy {
        val: u64,
    }

    impl TryFrom<ArcU64Proxy> for std::sync::Arc<u64> {
        type Error = &'static str;
        fn try_from(_proxy: ArcU64Proxy) -> Result<Self, Self::Error> {
            Err("not implemented for this test")
        }
    }

    impl From<&std::sync::Arc<u64>> for ArcU64Proxy {
        fn from(v: &std::sync::Arc<u64>) -> Self {
            ArcU64Proxy { val: **v }
        }
    }

    #[derive(Facet)]
    struct MyType {
        #[facet(opaque, proxy = OpaqueTypeStrProxy)]
        str: OpaqueType,
        #[facet(opaque, proxy = OpaqueTypeNestedProxy)]
        nest: OpaqueType,
        #[facet(proxy = U64ToStrProxy)]
        cust: u64,
        #[facet(opaque, proxy = ArcU64Proxy)]
        arc: std::sync::Arc<u64>,
    }

    let data = MyType {
        str: OpaqueType(2748),
        nest: OpaqueType(8472),
        cust: 2748,
        arc: std::sync::Arc::new(3342),
    };

    let ser = facet_json_legacy::to_string(&data);

    let expected = r#"{"str":"0xabc","nest":{"val":8472},"cust":"0xabc","arc":{"val":3342}}"#;

    assert_eq!(ser, expected);
}

#[test]
fn test_proxy_serialization_enum() {
    // Target type that doesn't implement Facet
    struct OpaqueType(u64);

    // Proxy type for serializing to hex string
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct OpaqueTypeProxy(String);

    impl TryFrom<OpaqueTypeProxy> for OpaqueType {
        type Error = &'static str;
        fn try_from(_proxy: OpaqueTypeProxy) -> Result<Self, Self::Error> {
            Err("not implemented for this test")
        }
    }

    impl From<&OpaqueType> for OpaqueTypeProxy {
        fn from(v: &OpaqueType) -> Self {
            OpaqueTypeProxy(format!("0x{:x}", v.0))
        }
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    enum MyEnum {
        OpStrTuple(#[facet(opaque, proxy = OpaqueTypeProxy)] OpaqueType),
        OpStrField {
            #[facet(opaque, proxy = OpaqueTypeProxy)]
            field: OpaqueType,
        },
    }

    let data = MyEnum::OpStrTuple(OpaqueType(2748));
    let expected = r#"{"OpStrTuple":"0xabc"}"#;
    let ser = facet_json_legacy::to_string(&data);
    assert_eq!(ser, expected);

    let data = MyEnum::OpStrField {
        field: OpaqueType(2748),
    };
    let expected = r#"{"OpStrField":{"field":"0xabc"}}"#;
    let ser = facet_json_legacy::to_string(&data);
    assert_eq!(ser, expected);
}

#[test]
fn test_proxy_serialize_transparent_struct() {
    // Target type that doesn't implement Facet
    #[derive(Clone)]
    struct MyUrl(String);

    // Proxy type
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct MyUrlProxy(String);

    impl TryFrom<MyUrlProxy> for MyUrl {
        type Error = &'static str;
        fn try_from(_proxy: MyUrlProxy) -> Result<Self, Self::Error> {
            Err("not implemented for this test")
        }
    }

    impl From<&MyUrl> for MyUrlProxy {
        fn from(v: &MyUrl) -> Self {
            MyUrlProxy(v.0.clone())
        }
    }

    #[derive(Facet)]
    #[facet(transparent)]
    struct UrlWrapper(#[facet(opaque, proxy = MyUrlProxy)] MyUrl);

    let data = r#""http://thing""#;

    let test = facet_json_legacy::to_string(&UrlWrapper(MyUrl("http://thing".to_owned())));

    assert_eq!(data, test);
}
