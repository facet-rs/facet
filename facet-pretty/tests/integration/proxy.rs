use facet::Facet;
use facet_pretty::PrettyPrinter;
use facet_testhelpers::test;
use insta::assert_snapshot;

/// Proxy type that serializes an integer as a string.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct IntAsString(pub String);

/// Target type that uses the proxy for serialization.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(proxy = IntAsString)]
pub struct ProxyInt {
    pub value: i32,
}

/// Struct that aliases the same proxied value twice.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ProxyIntAliased {
    pub a: std::rc::Rc<ProxyInt>,
    pub b: std::rc::Rc<ProxyInt>,
}

/// Convert from proxy (deserialization): string -> ProxyInt
impl TryFrom<IntAsString> for ProxyInt {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
        Ok(ProxyInt {
            value: proxy.0.parse()?,
        })
    }
}

/// Convert to proxy (serialization): ProxyInt -> string
impl From<&ProxyInt> for IntAsString {
    fn from(v: &ProxyInt) -> Self {
        IntAsString(v.value.to_string())
    }
}

#[test]
fn test_proxy_container_pretty_print() {
    let proxy_int = ProxyInt { value: 42 };
    let output = PrettyPrinter::new()
        .with_colors(false.into())
        .format(&proxy_int);

    assert_snapshot!(output, @r#""42""#);
}

/// Struct with field-level proxy.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ProxyFieldLevel {
    pub name: String,
    #[facet(proxy = IntAsString)]
    pub count: i32,
}

/// Convert from proxy for field-level proxy.
impl TryFrom<IntAsString> for i32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
        proxy.0.parse()
    }
}

/// Convert to proxy for field-level proxy.
impl From<&i32> for IntAsString {
    fn from(v: &i32) -> Self {
        IntAsString(v.to_string())
    }
}

#[test]
fn test_proxy_field_level_pretty_print() {
    let data = ProxyFieldLevel {
        name: "test".to_string(),
        count: 100,
    };
    let output = PrettyPrinter::new().with_colors(false.into()).format(&data);

    assert_snapshot!(output, @r#"
    ProxyFieldLevel {
      name: "test",
      count: "100",
    }
    "#);
}

// Tests for format_peek_with_spans

#[test]
fn test_proxy_container_format_peek_with_spans() {
    use facet_reflect::Peek;

    let proxy_int = ProxyInt { value: 42 };
    let formatted = PrettyPrinter::new().format_peek_with_spans(Peek::new(&proxy_int));

    // The proxy should convert the integer to a string representation
    assert_snapshot!(formatted.text, @r#""42""#);

    // Check that we have spans recorded
    assert!(!formatted.spans.is_empty());
}

#[test]
fn test_proxy_container_aliasing_format_peek_with_spans() {
    use facet_reflect::Peek;
    use std::rc::Rc;

    let shared = Rc::new(ProxyInt { value: 42 });
    let value = ProxyIntAliased {
        a: shared.clone(),
        b: shared,
    };

    let formatted = PrettyPrinter::new().format_peek_with_spans(Peek::new(&value));
    assert!(
        !formatted.text.contains("cycle detected"),
        "aliasing a proxied value should not be treated as a cycle"
    );

    assert_snapshot!(formatted.text, @r#"
    ProxyIntAliased {
      a: "42",
      b: "42",
    }
    "#);
}

/// Test that proxy types inside Arc-aliased structures don't trigger false cycle detection.
/// This reproduces the issue where multiple Arc pointers to the same data containing proxy
/// fields would incorrectly trigger cycle detection.
#[test]
fn test_proxy_inside_arc_aliased_struct() {
    use facet_reflect::Peek;
    use std::sync::Arc;

    /// Inner struct that contains a proxy field
    #[derive(Facet, Debug, Clone)]
    pub struct InnerWithProxy {
        pub name: String,
        #[facet(proxy = IntAsString)]
        pub count: i32,
    }

    /// Outer struct that references the same Arc from two different paths
    #[derive(Facet, Debug, Clone)]
    pub struct OuterWithArcAliasing {
        pub direct: Arc<InnerWithProxy>,
        pub nested: NestedArcRef,
    }

    #[derive(Facet, Debug, Clone)]
    pub struct NestedArcRef {
        pub inner: Arc<InnerWithProxy>,
    }

    let shared = Arc::new(InnerWithProxy {
        name: "test".to_string(),
        count: 42,
    });

    let value = OuterWithArcAliasing {
        direct: shared.clone(),
        nested: NestedArcRef { inner: shared },
    };

    let formatted = PrettyPrinter::new()
        .with_colors(false.into())
        .format_peek_with_spans(Peek::new(&value));

    // The proxy field should NOT trigger cycle detection
    assert!(
        !formatted.text.contains("cycle detected"),
        "proxy fields inside Arc-aliased structs should not trigger false cycle detection. Got:\n{}",
        formatted.text
    );

    // Both occurrences of the proxy field should show the actual value
    assert!(
        formatted.text.matches("\"42\"").count() == 2,
        "expected 2 occurrences of the proxied value \"42\", got:\n{}",
        formatted.text
    );
}

#[test]
fn test_proxy_field_level_format_peek_with_spans() {
    use facet_pretty::PathSegment;
    use facet_reflect::Peek;
    use std::borrow::Cow;

    let data = ProxyFieldLevel {
        name: "test".to_string(),
        count: 100,
    };
    let formatted = PrettyPrinter::new().format_peek_with_spans(Peek::new(&data));

    // The output should show count as a string
    assert_snapshot!(formatted.text, @r#"
    ProxyFieldLevel {
      name: "test",
      count: "100",
    }
    "#);

    // Check that we have spans for the fields
    let count_path = vec![PathSegment::Field(Cow::Borrowed("count"))];
    assert!(
        formatted.spans.contains_key(&count_path),
        "count field span not found"
    );

    // Verify the count field value is "100" (as a string via proxy)
    if let Some(span) = formatted.spans.get(&count_path) {
        let value_text = &formatted.text[span.value.0..span.value.1];
        assert_eq!(value_text, "\"100\"");
    }
}
