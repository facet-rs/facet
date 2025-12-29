//! Tests for all four combinations of opaque/proxy attributes.
//!
//! The four combinations are:
//! 1. Neither opaque nor proxy: normal Facet deserialization
//! 2. Only opaque: type is treated as opaque, no Facet impl needed, but no conversion
//! 3. Only proxy: type implements Facet, proxy handles conversion (e.g., validation)
//! 4. Both opaque and proxy: type doesn't implement Facet, proxy handles everything

use facet::Facet;
use facet_json_legacy as json;

/// Test for issue #1112: proxy without opaque should work.
/// Previously this failed with "not currently processing a field" because
/// begin_custom_deserialization was called after begin_some(), losing the field context.
#[test]
fn test_proxy_without_opaque() {
    // Target type that DOES implement Facet
    #[derive(Facet, Debug, Clone, Default, PartialEq)]
    pub struct StyleData {
        pub value: String,
    }

    // Proxy type that implements Facet
    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct StyleProxy(pub String);

    // Convert proxy -> Option<StyleData> (deserialization)
    impl From<StyleProxy> for Option<StyleData> {
        fn from(proxy: StyleProxy) -> Self {
            if proxy.0.is_empty() {
                None
            } else {
                Some(StyleData { value: proxy.0 })
            }
        }
    }

    // Convert &Option<StyleData> -> proxy (serialization)
    impl From<&Option<StyleData>> for StyleProxy {
        fn from(v: &Option<StyleData>) -> Self {
            StyleProxy(v.as_ref().map(|d| d.value.clone()).unwrap_or_default())
        }
    }

    #[derive(Facet, Debug, Clone, Default, PartialEq)]
    pub struct Element {
        // Note: NO opaque attribute! Just proxy. This used to fail.
        #[facet(default, proxy = StyleProxy)]
        pub style: Option<StyleData>,
    }

    // Test with field present
    let json = r#"{"style":"color:red"}"#;
    let elem: Element = json::from_str(json).unwrap();
    assert_eq!(
        elem.style,
        Some(StyleData {
            value: "color:red".to_string()
        })
    );

    // Test with null (uses default)
    let json2 = r#"{"style":null}"#;
    let elem2: Element = json::from_str(json2).unwrap();
    assert!(elem2.style.is_none());

    // Test with empty string (proxy converts to None)
    let json3 = r#"{"style":""}"#;
    let elem3: Element = json::from_str(json3).unwrap();
    assert!(elem3.style.is_none());

    // Test with missing field (uses default)
    let json4 = r#"{}"#;
    let elem4: Element = json::from_str(json4).unwrap();
    assert!(elem4.style.is_none());

    // Test serialization roundtrip
    let elem5 = Element {
        style: Some(StyleData {
            value: "font-size:12px".to_string(),
        }),
    };
    let serialized = json::to_string(&elem5);
    assert_eq!(serialized, r#"{"style":"font-size:12px"}"#);

    let deserialized: Element = json::from_str(&serialized).unwrap();
    assert_eq!(elem5, deserialized);
}

/// Test opaque + proxy on Option<T> fields (issue #1075)
#[test]
fn test_opaque_with_proxy_option() {
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

    // Test with field present
    let json = r#"{"d":"M0,0 L10,10"}"#;
    let path: Path = json::from_str(json).unwrap();
    assert!(path.d.is_some());
    assert_eq!(path.d.unwrap().commands, vec!["M0,0 L10,10".to_string()]);
}

/// Test proxy on non-Option field (validation use case)
#[test]
fn test_proxy_for_validation() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct ValidatedEmail {
        pub address: String,
    }

    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct EmailProxy(pub String);

    impl TryFrom<EmailProxy> for ValidatedEmail {
        type Error = String;
        fn try_from(proxy: EmailProxy) -> Result<Self, Self::Error> {
            if proxy.0.contains('@') {
                Ok(ValidatedEmail { address: proxy.0 })
            } else {
                Err("Invalid email: must contain @".to_string())
            }
        }
    }

    impl From<&ValidatedEmail> for EmailProxy {
        fn from(v: &ValidatedEmail) -> Self {
            EmailProxy(v.address.clone())
        }
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct User {
        #[facet(proxy = EmailProxy)]
        pub email: ValidatedEmail,
    }

    // Valid email
    let json = r#"{"email":"test@example.com"}"#;
    let user: User = json::from_str(json).unwrap();
    assert_eq!(user.email.address, "test@example.com");

    // Invalid email should fail
    let json2 = r#"{"email":"invalid"}"#;
    let result: Result<User, _> = json::from_str(json2);
    assert!(result.is_err());
}
