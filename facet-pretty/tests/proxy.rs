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
    let output = PrettyPrinter::new().with_colors(false).format(&proxy_int);

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
    let output = PrettyPrinter::new().with_colors(false).format(&data);

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
