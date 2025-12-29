use std::num::IntErrorKind;

use facet::Facet;
use facet_testhelpers::test;

/// Helper to parse u64 from string (supports hex with 0x prefix)
fn parse_u64(s: &str) -> Result<u64, &'static str> {
    if let Some(hex) = s.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        s.parse::<u64>()
    }
    .map_err(|e| match e.kind() {
        IntErrorKind::Empty => "cannot parse integer from empty string",
        IntErrorKind::InvalidDigit => "invalid digit found in string",
        IntErrorKind::PosOverflow => "number too large to fit in target type",
        IntErrorKind::NegOverflow => "number too small to fit in target type",
        IntErrorKind::Zero => "number would be zero for non-zero type",
        _ => "unknown error",
    })
}

#[test]
fn test_proxy_deserialization_struct() {
    // Target type that doesn't implement Facet
    struct OpaqueType(u64);

    // Proxy type for OpaqueType that deserializes from a string
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct OpaqueTypeProxy(String);

    impl TryFrom<OpaqueTypeProxy> for OpaqueType {
        type Error = &'static str;
        fn try_from(proxy: OpaqueTypeProxy) -> Result<Self, Self::Error> {
            Ok(OpaqueType(parse_u64(&proxy.0)?))
        }
    }

    impl From<&OpaqueType> for OpaqueTypeProxy {
        fn from(v: &OpaqueType) -> Self {
            OpaqueTypeProxy(v.0.to_string())
        }
    }

    // Proxy type for u64 that deserializes from string (with hex support)
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct U64FromStrProxy(String);

    impl TryFrom<U64FromStrProxy> for u64 {
        type Error = &'static str;
        fn try_from(proxy: U64FromStrProxy) -> Result<Self, Self::Error> {
            parse_u64(&proxy.0)
        }
    }

    impl From<&u64> for U64FromStrProxy {
        fn from(v: &u64) -> Self {
            U64FromStrProxy(v.to_string())
        }
    }

    // Proxy type for Arc<u64> from nested struct
    #[derive(Facet, Clone)]
    struct ArcU64Proxy {
        val: u64,
    }

    impl TryFrom<ArcU64Proxy> for std::sync::Arc<u64> {
        type Error = &'static str;
        fn try_from(proxy: ArcU64Proxy) -> Result<Self, Self::Error> {
            Ok(std::sync::Arc::new(proxy.val))
        }
    }

    impl From<&std::sync::Arc<u64>> for ArcU64Proxy {
        fn from(v: &std::sync::Arc<u64>) -> Self {
            ArcU64Proxy { val: **v }
        }
    }

    #[derive(Facet)]
    struct MyType {
        #[facet(opaque, proxy = OpaqueTypeProxy)]
        str: OpaqueType,
        #[facet(proxy = U64FromStrProxy)]
        cust: u64,
        #[facet(opaque, proxy = ArcU64Proxy)]
        arc: std::sync::Arc<u64>,
    }

    let data = r#"{"str":"0xabc","cust":"0xabc","arc":{"val": 3342}}"#;

    let test: MyType = facet_json_legacy::from_str(data).unwrap();
    assert_eq!(test.str.0, 2748);
    assert_eq!(test.cust, 2748);
    assert_eq!(*test.arc, 3342);
}

#[test]
fn test_proxy_deserialization_enum() {
    // Target type that doesn't implement Facet
    struct OpaqueType(u64);

    // Proxy type for OpaqueType that deserializes from a string
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct OpaqueTypeProxy(String);

    impl TryFrom<OpaqueTypeProxy> for OpaqueType {
        type Error = &'static str;
        fn try_from(proxy: OpaqueTypeProxy) -> Result<Self, Self::Error> {
            Ok(OpaqueType(parse_u64(&proxy.0)?))
        }
    }

    impl From<&OpaqueType> for OpaqueTypeProxy {
        fn from(v: &OpaqueType) -> Self {
            OpaqueTypeProxy(v.0.to_string())
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

    let data = r#"{"OpStrTuple": "0xabc"}"#;
    let opstr: MyEnum = facet_json_legacy::from_str(data).unwrap();
    match opstr {
        MyEnum::OpStrTuple(OpaqueType(v)) => assert_eq!(v, 2748),
        _ => panic!("expected OpStrTuple variant"),
    }

    let data = r#"{"OpStrField": {"field": "0xabc"}}"#;
    let opstr: MyEnum = facet_json_legacy::from_str(data).unwrap();
    match opstr {
        MyEnum::OpStrField {
            field: OpaqueType(v),
        } => assert_eq!(v, 2748),
        _ => panic!("expected OpStrField variant"),
    }
}

#[test]
#[ignore = "TODO: transparent wrapper with opaque+proxy field not yet supported"]
fn test_proxy_transparent_struct() {
    // Target type that doesn't implement Facet
    #[derive(Clone)]
    struct MyUrl(String);

    // Proxy type that implements Facet
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct MyUrlProxy(String);

    impl TryFrom<MyUrlProxy> for MyUrl {
        type Error = &'static str;
        fn try_from(proxy: MyUrlProxy) -> Result<Self, Self::Error> {
            Ok(MyUrl(proxy.0))
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

    let test: UrlWrapper = facet_json_legacy::from_str(data).unwrap();

    assert_eq!(&test.0.0, "http://thing");
}

/// Test for the new `proxy` attribute that uses TryFrom for conversion
#[test]
fn test_proxy_attribute_deserialization() {
    // A third-party type that doesn't implement Facet
    #[derive(Debug, PartialEq)]
    struct ExternalConfig {
        api_key: String,
        endpoint: String,
    }

    // Proxy type that implements Facet and mirrors the structure
    #[derive(Facet, Clone)]
    struct ExternalConfigProxy {
        api_key: String,
        endpoint: String,
    }

    // Implement TryFrom<Proxy> for ExternalConfig (deserialization)
    impl TryFrom<ExternalConfigProxy> for ExternalConfig {
        type Error = &'static str;

        fn try_from(proxy: ExternalConfigProxy) -> Result<Self, Self::Error> {
            if proxy.api_key.is_empty() {
                return Err("api_key cannot be empty");
            }
            Ok(ExternalConfig {
                api_key: proxy.api_key,
                endpoint: proxy.endpoint,
            })
        }
    }

    // Implement From<&ExternalConfig> for Proxy (serialization)
    impl From<&ExternalConfig> for ExternalConfigProxy {
        fn from(cfg: &ExternalConfig) -> Self {
            ExternalConfigProxy {
                api_key: cfg.api_key.clone(),
                endpoint: cfg.endpoint.clone(),
            }
        }
    }

    // Container struct using the proxy attribute
    #[derive(Facet)]
    struct AppConfig {
        #[facet(opaque, proxy = ExternalConfigProxy)]
        external: ExternalConfig,
    }

    let data = r#"{"external":{"api_key":"secret123","endpoint":"https://api.example.com"}}"#;

    let config: AppConfig = facet_json_legacy::from_str(data).unwrap();

    assert_eq!(config.external.api_key, "secret123");
    assert_eq!(config.external.endpoint, "https://api.example.com");
}

/// Test proxy attribute with validation error
#[test]
fn test_proxy_attribute_validation_error() {
    #[derive(Debug, PartialEq)]
    struct ValidatedString(String);

    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct StringProxy(String);

    impl TryFrom<StringProxy> for ValidatedString {
        type Error = &'static str;

        fn try_from(proxy: StringProxy) -> Result<Self, Self::Error> {
            if proxy.0.is_empty() {
                return Err("string cannot be empty");
            }
            Ok(ValidatedString(proxy.0))
        }
    }

    impl From<&ValidatedString> for StringProxy {
        fn from(v: &ValidatedString) -> Self {
            StringProxy(v.0.clone())
        }
    }

    #[derive(Facet)]
    struct Container {
        #[facet(opaque, proxy = StringProxy)]
        value: ValidatedString,
    }

    // Valid case
    let data = r#"{"value":"hello"}"#;
    let result: Container = facet_json_legacy::from_str(data).unwrap();
    assert_eq!(result.value.0, "hello");

    // Invalid case - empty string should fail
    let data = r#"{"value":""}"#;
    let result: Result<Container, _> = facet_json_legacy::from_str(data);
    assert!(result.is_err());
}

/// Test for issue #1075: UB/SIGABRT when using opaque + proxy on Option<T> fields
#[test]
fn test_opaque_proxy_option_field() {
    // Target type that doesn't implement Facet
    #[derive(Debug, Clone, Default)]
    pub struct PathData {
        pub commands: Vec<String>,
    }

    // Proxy type that implements Facet
    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct PathDataProxy(pub String);

    // Convert proxy -> Option<PathData> (deserialization)
    impl From<PathDataProxy> for Option<PathData> {
        fn from(proxy: PathDataProxy) -> Self {
            Some(PathData {
                commands: vec![proxy.0],
            })
        }
    }

    // Convert &Option<PathData> -> proxy (serialization)
    impl From<&Option<PathData>> for PathDataProxy {
        fn from(v: &Option<PathData>) -> Self {
            PathDataProxy(v.as_ref().map(|d| d.commands.join(",")).unwrap_or_default())
        }
    }

    #[derive(Facet, Debug, Clone, Default)]
    pub struct Path {
        #[facet(opaque, proxy = PathDataProxy)]
        pub d: Option<PathData>,
    }

    // This should work without SIGABRT
    let data = r#"{"d":"M0,0 L10,10"}"#;
    let path: Path = facet_json_legacy::from_str(data).unwrap();
    assert!(path.d.is_some());
    assert_eq!(path.d.unwrap().commands, vec!["M0,0 L10,10".to_string()]);
}
