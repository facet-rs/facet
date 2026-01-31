/// A metadata container that captures both span and doc metadata.
///
/// This is useful for validation errors that need to point back to source locations,
/// while also preserving doc comments.
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
pub struct WithMeta<T> {
    pub value: T,

    #[facet(metadata = "span")]
    pub span: Option<Span>,

    #[facet(metadata = "doc")]
    pub doc: Option<Vec<String>>,

    #[facet(metadata = "tag")]
    pub tag: Option<String>,
}

use super::super::*;
use facet::Facet;
use facet_reflect::Span;
use facet_testhelpers::test;

struct ParseTest<'a> {
    source: &'a str,
}

impl<'a> ParseTest<'a> {
    fn parse<T: Facet<'static>>(source: &'a str, f: impl FnOnce(&Self, T)) {
        let test = Self { source };
        let parsed: T = from_str(source).unwrap();
        f(&test, parsed);
    }

    #[track_caller]
    fn assert_is<T, E>(
        &self,
        meta: &WithMeta<T>,
        expected: E,
        span_text: &str,
        doc: Option<&[&str]>,
        tag: Option<&str>,
    ) where
        T: PartialEq + std::fmt::Debug,
        E: Into<T>,
    {
        assert_eq!(meta.value, expected.into(), "value mismatch");
        let span = meta.span.expect("expected span to be present");
        let actual = &self.source[span.offset as usize..(span.offset + span.len) as usize];
        assert_eq!(actual, span_text, "span mismatch");
        if let Some(expected_lines) = doc {
            let meta_doc_lines = meta.doc.as_ref().expect("expected doc to be present");
            assert_eq!(
                meta_doc_lines.len(),
                expected_lines.len(),
                "doc line count mismatch"
            );
            for (i, (actual, expected)) in
                meta_doc_lines.iter().zip(expected_lines.iter()).enumerate()
            {
                assert_eq!(actual, *expected, "doc line {} mismatch", i);
            }
        }
        if let Some(tag) = tag {
            let meta_tag = meta.tag.as_ref().unwrap();
            assert_eq!(meta_tag, tag, "tag mismatch");
        }
    }
}

impl<T: PartialEq> PartialEq for WithMeta<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for WithMeta<T> {}

impl<T: std::hash::Hash> std::hash::Hash for WithMeta<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

/// Reference test demonstrating the `ParseTest` harness conventions:
///
/// - Always use raw string literals (`r#"..."#`) for source input
/// - Always use actual newlines, never `\n` escapes
/// - Use `ParseTest::parse(source, |t, parsed| { ... })` to parse and test
/// - Use `t.assert_is(&field, value, "span", doc, tag)` to check value, span, doc, and tag
/// - For strings, `value` can be `&str` (converts via `Into`)
/// - For integers, suffix literals to match the type (e.g., `8080u16`)
/// - Pass `None` for doc/tag when not testing those, or `Some("...")` to assert
#[test]
fn test_spanned_doc_as_struct_field() {
    #[derive(Facet, Debug)]
    struct Config {
        name: WithMeta<String>,
        port: WithMeta<u16>,
    }

    ParseTest::parse(
        r#"
name myapp
port 8080
"#,
        |t, c: Config| {
            t.assert_is(&c.name, "myapp", "myapp", None, None);
            t.assert_is(&c.port, 8080u16, "8080", None, None);

            // Roundtrip: serialize and check output (spans are not preserved)
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
name myapp

port 8080"#
                    .trim()
            );
        },
    );
}

#[test]
fn test_doc_comment() {
    #[derive(Facet, Debug)]
    struct Config {
        name: WithMeta<String>,
    }

    ParseTest::parse(
        r#"
/// The application name
name myapp
"#,
        |t, c: Config| {
            t.assert_is(
                &c.name,
                "myapp",
                "myapp",
                Some(&["The application name"]),
                None,
            );

            // Roundtrip: doc comment should be preserved
            let serialized = to_string(&c).unwrap();
            assert_eq!(
                serialized.trim(),
                r#"
/// The application name
name myapp"#
                    .trim()
            );
        },
    );
}

#[test]
fn test_spanned_doc_as_map_value() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<String, WithMeta<String>>,
    }

    ParseTest::parse(
        r#"
foo bar
baz qux
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 2);
            t.assert_is(c.items.get("foo").unwrap(), "bar", "bar", None, None);
            t.assert_is(c.items.get("baz").unwrap(), "qux", "qux", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
foo bar

baz qux"#
                    .trim()
            );
        },
    );
}

#[test]
fn test_spanned_doc_as_map_key() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<WithMeta<String>, String>,
    }

    ParseTest::parse(
        r#"
foo bar
baz qux
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 2);
            let keys: Vec<_> = c.items.keys().collect();
            t.assert_is(keys[0], "foo", "foo", None, None);
            t.assert_is(keys[1], "baz", "baz", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
foo bar

baz qux"#
                    .trim()
            );
        },
    );
}

#[test]
fn test_spanned_doc_as_map_key_and_value() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<WithMeta<String>, WithMeta<String>>,
    }

    ParseTest::parse(
        r#"
foo bar
baz qux
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 2);
            let (key, val) = c.items.get_index(0).unwrap();
            t.assert_is(key, "foo", "foo", None, None);
            t.assert_is(val, "bar", "bar", None, None);
            let (key, val) = c.items.get_index(1).unwrap();
            t.assert_is(key, "baz", "baz", None, None);
            t.assert_is(val, "qux", "qux", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
foo bar

baz qux"#
                    .trim()
            );
        },
    );
}

#[test]
fn test_tag_on_value() {
    #[derive(Facet, Debug)]
    struct Config {
        value: WithMeta<String>,
    }

    ParseTest::parse(
        r#"
value @tag"hello"
"#,
        |t, c: Config| {
            t.assert_is(&c.value, "hello", r#"@tag"hello""#, None, Some("tag"));

            // Roundtrip: tag should be preserved
            let serialized = to_string(&c).unwrap();
            assert_eq!(serialized.trim(), r#"value @tag"hello""#);
        },
    );
}

#[test]
fn test_tag_unit() {
    #[derive(Facet, Debug)]
    struct Config {
        status: WithMeta<()>,
    }

    ParseTest::parse(
        r#"
status @ok
"#,
        |t, c: Config| {
            t.assert_is(&c.status, (), "@ok", None, Some("ok"));

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "status @ok");
        },
    );
}

#[test]
fn test_tag_bare_unit() {
    #[derive(Facet, Debug)]
    struct Config {
        status: WithMeta<()>,
    }

    ParseTest::parse(
        r#"
status @
"#,
        |t, c: Config| {
            t.assert_is(&c.status, (), "@", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "status @");
        },
    );
}

#[test]
fn test_unit_key_in_map() {
    use std::collections::HashMap;

    #[derive(Facet, Debug)]
    struct Config {
        items: HashMap<Option<String>, String>,
    }

    ParseTest::parse(
        r#"
items {
    @ value
}
"#,
        |_t, c: Config| {
            assert_eq!(c.items.len(), 1);
            assert_eq!(c.items.get(&None), Some(&"value".to_string()));

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "items {@ value}");
        },
    );
}

#[test]
fn test_tag_in_map_key_and_value() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        items: IndexMap<WithMeta<String>, WithMeta<String>>,
    }

    ParseTest::parse(
        r#"
items {
    @key"foo" @val"bar"
}
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 1);
            let (key, val) = c.items.get_index(0).unwrap();
            t.assert_is(key, "foo", r#"@key"foo""#, None, Some("key"));
            t.assert_is(val, "bar", r#"@val"bar""#, None, Some("val"));

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), r#"items {@key"foo" @val"bar"}"#);
        },
    );
}

#[test]
fn test_spanned_doc_in_array() {
    #[derive(Facet, Debug)]
    struct Config {
        items: Vec<WithMeta<String>>,
    }

    ParseTest::parse(
        r#"
items (alpha beta gamma)
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 3);
            t.assert_is(&c.items[0], "alpha", "alpha", None, None);
            t.assert_is(&c.items[1], "beta", "beta", None, None);
            t.assert_is(&c.items[2], "gamma", "gamma", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "items (alpha beta gamma)");
        },
    );
}

#[test]
fn test_spanned_doc_in_nested_struct() {
    #[derive(Facet, Debug)]
    struct Inner {
        value: WithMeta<i32>,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        inner: Inner,
    }

    ParseTest::parse(
        r#"
inner { value 42 }
"#,
        |t, c: Outer| {
            t.assert_is(&c.inner.value, 42, "42", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "inner {value 42}");
        },
    );
}

#[test]
fn test_spanned_doc_with_option_present() {
    #[derive(Facet, Debug)]
    struct Config {
        name: Option<WithMeta<String>>,
    }

    ParseTest::parse(
        r#"
name hello
"#,
        |t, c: Config| {
            t.assert_is(c.name.as_ref().unwrap(), "hello", "hello", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "name hello");
        },
    );
}

#[test]
fn test_spanned_doc_with_option_absent() {
    #[derive(Facet, Debug)]
    struct Config {
        #[facet(skip_serializing_if = Option::is_none)]
        name: Option<WithMeta<String>>,
        other: String,
    }

    ParseTest::parse(
        r#"
other world
"#,
        |_t, c: Config| {
            assert!(c.name.is_none());
            assert_eq!(c.other, "world");

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "other world");
        },
    );
}

#[test]
fn test_spanned_doc_with_integers() {
    #[derive(Facet, Debug)]
    struct Numbers {
        a: WithMeta<i32>,
        b: WithMeta<u64>,
        c: WithMeta<i8>,
    }

    ParseTest::parse(
        r#"
a -42
b 999
c 127
"#,
        |t, c: Numbers| {
            t.assert_is(&c.a, -42, "-42", None, None);
            t.assert_is(&c.b, 999u64, "999", None, None);
            t.assert_is(&c.c, 127i8, "127", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
a -42

b 999

c 127"#
                    .trim()
            );
        },
    );
}

#[test]
fn test_spanned_doc_with_booleans() {
    #[derive(Facet, Debug)]
    struct Flags {
        enabled: WithMeta<bool>,
        debug: WithMeta<bool>,
    }

    ParseTest::parse(
        r#"
enabled true
debug false
"#,
        |t, c: Flags| {
            t.assert_is(&c.enabled, true, "true", None, None);
            t.assert_is(&c.debug, false, "false", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
enabled true

debug false"#
                    .trim()
            );
        },
    );
}

#[test]
fn test_spanned_doc_in_flattened_map_inline() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<WithMeta<String>, WithMeta<String>>,
    }

    ParseTest::parse(
        r#"
{foo bar, baz qux}
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 2);
            let (key, val) = c.items.get_index(0).unwrap();
            t.assert_is(key, "foo", "foo", None, None);
            t.assert_is(val, "bar", "bar", None, None);
            let (key, val) = c.items.get_index(1).unwrap();
            t.assert_is(key, "baz", "baz", None, None);
            t.assert_is(val, "qux", "qux", None, None);

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
foo bar

baz qux"#
                    .trim()
            );
        },
    );
}

// =============================================================================
// Edge case tests
// =============================================================================

/// Test that multi-line doc comments are captured as multiple lines.
#[test]
fn test_multiline_doc_comment() {
    #[derive(Facet, Debug)]
    struct Config {
        name: WithMeta<String>,
    }

    ParseTest::parse(
        r#"
/// First line of documentation.
/// Second line of documentation.
/// Third line.
name myapp
"#,
        |t, c: Config| {
            t.assert_is(
                &c.name,
                "myapp",
                "myapp",
                Some(&[
                    "First line of documentation.",
                    "Second line of documentation.",
                    "Third line.",
                ]),
                None,
            );

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
/// First line of documentation.
/// Second line of documentation.
/// Third line.
name myapp"#
                    .trim()
            );
        },
    );
}

/// Test that tags are captured on sequence elements.
#[test]
fn test_tag_on_sequence_element() {
    #[derive(Facet, Debug)]
    struct Config {
        items: Vec<WithMeta<()>>,
    }

    ParseTest::parse(
        r#"
items (@ok @err @ok)
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 3);
            t.assert_is(&c.items[0], (), "@ok", None, Some("ok"));
            t.assert_is(&c.items[1], (), "@err", None, Some("err"));
            t.assert_is(&c.items[2], (), "@ok", None, Some("ok"));

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), "items (@ok @err @ok)");
        },
    );
}

/// Test that a tag on a nested struct value is captured.
#[test]
fn test_tag_on_nested_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        field: String,
    }

    #[derive(Facet, Debug)]
    struct Config {
        inner: WithMeta<Inner>,
    }

    ParseTest::parse(
        r#"
inner @tagged{field value}
"#,
        |t, c: Config| {
            assert_eq!(c.inner.value.field, "value");
            t.assert_is(
                &c.inner,
                Inner {
                    field: "value".into(),
                },
                "@tagged{field value}",
                None,
                Some("tagged"),
            );

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), r#"inner @tagged{field "value"}"#);
        },
    );
}

/// Test mixed tagged and untagged map entries.
#[test]
fn test_mixed_tagged_untagged_map_entries() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        items: IndexMap<WithMeta<String>, String>,
    }

    ParseTest::parse(
        r#"
items {
    foo bar
    @key"baz" qux
}
"#,
        |t, c: Config| {
            assert_eq!(c.items.len(), 2);
            let keys: Vec<_> = c.items.keys().collect();
            t.assert_is(keys[0], "foo", "foo", None, None);
            t.assert_is(keys[1], "baz", r#"@key"baz""#, None, Some("key"));

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), r#"items {foo bar, @key"baz" qux}"#);
        },
    );
}

/// Test that span for quoted strings includes the quotes.
#[test]
fn test_span_quoted_string_with_escapes() {
    #[derive(Facet, Debug)]
    struct Config {
        name: WithMeta<String>,
    }

    ParseTest::parse(
        r#"
name "hello\nworld"
"#,
        |t, c: Config| {
            // The value is the unescaped string
            assert_eq!(c.name.value, "hello\nworld");
            // The span should cover the quoted string in the source (including quotes)
            t.assert_is(
                &c.name,
                "hello\nworld".to_string(),
                r#""hello\nworld""#,
                None,
                None,
            );

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(s.trim(), r#"name "hello\nworld""#);
        },
    );
}

/// Test tag on Option value - @some and @none style.
#[test]
fn test_tag_on_option_value() {
    #[derive(Facet, Debug)]
    struct Config {
        present: WithMeta<Option<String>>,
        absent: WithMeta<Option<String>>,
    }

    ParseTest::parse(
        r#"
present @some"hello"
absent @none
"#,
        |t, c: Config| {
            t.assert_is(
                &c.present,
                Some("hello".to_string()),
                r#"@some"hello""#,
                None,
                Some("some"),
            );
            t.assert_is(&c.absent, None, "@none", None, Some("none"));

            // Roundtrip
            let s = to_string(&c).unwrap();
            assert_eq!(
                s.trim(),
                r#"
present @some"hello"

absent @none"#
                    .trim()
            );
        },
    );
}
