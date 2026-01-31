use super::super::*;
use facet::Facet;
use facet_reflect::Span;
use facet_testhelpers::test;

/// A metadata container that captures both span and doc metadata.
///
/// This is useful for validation errors that need to point back to source locations,
/// while also preserving doc comments.
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
pub struct SpannedDoc<T> {
    pub value: T,
    #[facet(metadata = "span")]
    pub span: Option<Span>,
    #[facet(metadata = "doc")]
    pub doc: Option<Vec<String>>,
}

impl<T: PartialEq> PartialEq for SpannedDoc<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for SpannedDoc<T> {}

impl<T: std::hash::Hash> std::hash::Hash for SpannedDoc<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

// =========================================================================
// SpannedDoc<T> tests - metadata container in various positions
// =========================================================================

/// Test SpannedDoc<T> as a struct field.
#[test]
fn test_spanned_doc_as_struct_field() {
    #[derive(Facet, Debug)]
    struct Config {
        name: SpannedDoc<String>,
        port: SpannedDoc<u16>,
    }

    let source = "name myapp\nport 8080";
    let result: Config = from_str(source).unwrap();

    assert_eq!(result.name.value, "myapp");
    assert!(result.name.span.is_some());
    let name_span = result.name.span.unwrap();
    assert_eq!(
        &source[name_span.offset as usize..(name_span.offset + name_span.len) as usize],
        "myapp"
    );

    assert_eq!(result.port.value, 8080);
    assert!(result.port.span.is_some());
    let port_span = result.port.span.unwrap();
    assert_eq!(
        &source[port_span.offset as usize..(port_span.offset + port_span.len) as usize],
        "8080"
    );
}

/// Test SpannedDoc<T> as a struct field with doc comments.
#[test]
fn test_spanned_doc_as_struct_field_with_docs() {
    #[derive(Facet, Debug)]
    struct Config {
        name: SpannedDoc<String>,
    }

    let source = "/// The application name\nname myapp";
    let result: Config = from_str(source).unwrap();

    assert_eq!(result.name.value, "myapp");
    assert!(result.name.span.is_some());

    // TODO: doc comments should be captured in metadata containers
    // but this isn't implemented yet for SpannedDoc
    // assert!(result.name.doc.is_some());
}

/// Test SpannedDoc<T> as a map value.
#[test]
fn test_spanned_doc_as_map_value() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<String, SpannedDoc<String>>,
    }

    let source = "foo bar\nbaz qux";
    let result: Config = from_str(source).unwrap();

    assert_eq!(result.items.len(), 2);

    let foo_val = result.items.get("foo").unwrap();
    assert_eq!(foo_val.value, "bar");
    assert!(foo_val.span.is_some());
    let foo_span = foo_val.span.unwrap();
    assert_eq!(
        &source[foo_span.offset as usize..(foo_span.offset + foo_span.len) as usize],
        "bar"
    );

    let baz_val = result.items.get("baz").unwrap();
    assert_eq!(baz_val.value, "qux");
    assert!(baz_val.span.is_some());
}

/// Test SpannedDoc<T> as a map key.
#[test]
fn test_spanned_doc_as_map_key() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<SpannedDoc<String>, String>,
    }

    let source = "foo bar\nbaz qux";
    let result: Config = from_str(source).unwrap();

    assert_eq!(result.items.len(), 2);

    let keys: Vec<_> = result.items.keys().collect();
    assert_eq!(keys[0].value, "foo");
    assert_eq!(keys[1].value, "baz");

    // TODO: spans should be populated for map keys but currently aren't
    // See: https://github.com/bearcove/styx/issues/45
    // assert!(keys[0].span.is_some());
}

/// Test SpannedDoc<T> as both map key and value.
#[test]
fn test_spanned_doc_as_map_key_and_value() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<SpannedDoc<String>, SpannedDoc<String>>,
    }

    let source = "foo bar\nbaz qux";
    let result: Config = from_str(source).unwrap();

    assert_eq!(result.items.len(), 2);

    let (key, val) = result.items.get_index(0).unwrap();
    assert_eq!(key.value, "foo");
    assert_eq!(val.value, "bar");

    // Values get spans
    assert!(val.span.is_some());

    // TODO: keys should get spans too but currently don't
    // See: https://github.com/bearcove/styx/issues/45
    // assert!(key.span.is_some());
}

/// Test SpannedDoc<T> in an array/sequence.
#[test]
fn test_spanned_doc_in_array() {
    #[derive(Facet, Debug)]
    struct Config {
        items: Vec<SpannedDoc<String>>,
    }

    let source = "items (alpha beta gamma)";
    let result: Config = from_str(source).unwrap();

    assert_eq!(result.items.len(), 3);

    assert_eq!(result.items[0].value, "alpha");
    assert!(result.items[0].span.is_some());
    let alpha_span = result.items[0].span.unwrap();
    assert_eq!(
        &source[alpha_span.offset as usize..(alpha_span.offset + alpha_span.len) as usize],
        "alpha"
    );

    assert_eq!(result.items[1].value, "beta");
    assert!(result.items[1].span.is_some());

    assert_eq!(result.items[2].value, "gamma");
    assert!(result.items[2].span.is_some());
}

/// Test SpannedDoc<T> in a nested struct.
#[test]
fn test_spanned_doc_in_nested_struct() {
    #[derive(Facet, Debug)]
    struct Inner {
        value: SpannedDoc<i32>,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        inner: Inner,
    }

    let source = "inner { value 42 }";
    let result: Outer = from_str(source).unwrap();

    assert_eq!(result.inner.value.value, 42);
    assert!(result.inner.value.span.is_some());
    let span = result.inner.value.span.unwrap();
    assert_eq!(
        &source[span.offset as usize..(span.offset + span.len) as usize],
        "42"
    );
}

/// Test SpannedDoc<T> with Option.
#[test]
fn test_spanned_doc_with_option_present() {
    #[derive(Facet, Debug)]
    struct Config {
        name: Option<SpannedDoc<String>>,
    }

    let source = "name hello";
    let result: Config = from_str(source).unwrap();

    assert!(result.name.is_some());
    let name = result.name.unwrap();
    assert_eq!(name.value, "hello");
    assert!(name.span.is_some());
}

/// Test SpannedDoc<T> with Option absent.
#[test]
fn test_spanned_doc_with_option_absent() {
    #[derive(Facet, Debug)]
    struct Config {
        name: Option<SpannedDoc<String>>,
        other: String,
    }

    let source = "other world";
    let result: Config = from_str(source).unwrap();

    assert!(result.name.is_none());
    assert_eq!(result.other, "world");
}

/// Test SpannedDoc<T> with integer values.
#[test]
fn test_spanned_doc_with_integers() {
    #[derive(Facet, Debug)]
    struct Numbers {
        a: SpannedDoc<i32>,
        b: SpannedDoc<u64>,
        c: SpannedDoc<i8>,
    }

    let source = "a -42\nb 999\nc 127";
    let result: Numbers = from_str(source).unwrap();

    assert_eq!(result.a.value, -42);
    assert_eq!(result.b.value, 999);
    assert_eq!(result.c.value, 127);

    assert!(result.a.span.is_some());
    assert!(result.b.span.is_some());
    assert!(result.c.span.is_some());
}

/// Test SpannedDoc<T> with boolean values.
#[test]
fn test_spanned_doc_with_booleans() {
    #[derive(Facet, Debug)]
    struct Flags {
        enabled: SpannedDoc<bool>,
        debug: SpannedDoc<bool>,
    }

    let source = "enabled true\ndebug false";
    let result: Flags = from_str(source).unwrap();

    assert_eq!(result.enabled.value, true);
    assert_eq!(result.debug.value, false);
    assert!(result.enabled.span.is_some());
    assert!(result.debug.span.is_some());
}

/// Test SpannedDoc<T> in a flattened map with inline object syntax.
#[test]
fn test_spanned_doc_in_flattened_map_inline() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct Config {
        #[facet(flatten)]
        items: IndexMap<SpannedDoc<String>, SpannedDoc<String>>,
    }

    let source = "{foo bar, baz qux}";
    let result: Config = from_str(source).unwrap();

    assert_eq!(result.items.len(), 2);

    let keys: Vec<_> = result.items.keys().map(|k| k.value.as_str()).collect();
    assert!(keys.contains(&"foo"));
    assert!(keys.contains(&"baz"));
}
