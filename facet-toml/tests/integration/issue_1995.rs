//! Tests for issue #1995: #[facet(other)] variants don't support metadata containers
//!
//! When using `#[facet(other)]` as a catch-all variant in an externally-tagged enum,
//! metadata containers like `Meta<T>` should be supported so that span information is preserved.

use facet::Facet;
use facet_reflect::Span;
use facet_toml as toml;
use std::collections::HashMap;

/// A value with source span information.
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
pub struct Meta<T> {
    pub value: T,
    #[facet(metadata = "span")]
    pub span: Option<Span>,
}

/// An externally-tagged enum with a catch-all variant that uses a metadata container.
#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
#[allow(dead_code)]
pub enum FilterValue {
    /// NULL check
    Null,
    /// Greater than
    Gt(Vec<Meta<String>>),
    /// Equality - bare scalar fallback (unknown variant names fall through here)
    #[facet(other)]
    EqBare(Meta<String>),
}

/// WHERE clause - filter conditions.
#[derive(Debug, Facet)]
pub struct Where {
    #[facet(flatten)]
    pub filters: HashMap<String, FilterValue>,
}

#[test]
fn other_variant_with_metadata_container() {
    // Input: {id = "$id"} - "id" is not a known variant, so falls through to EqBare
    // The value "$id" should be wrapped in Meta<String> with span info
    let input = r#"id = "$id""#;
    let result: Where = toml::from_str(input).unwrap();

    assert_eq!(result.filters.len(), 1);
    let value = result.filters.get("id").expect("should have 'id' key");

    match value {
        FilterValue::EqBare(meta) => {
            assert_eq!(meta.value, "$id");
            assert!(
                meta.span.is_some(),
                "span should be populated for #[facet(other)] variant"
            );
        }
        _ => panic!("Expected EqBare variant (other fallback), got {:?}", value),
    }
}

/// A strongly-typed string wrapper (like ColumnName from strid).
/// This is a newtype that wraps String but is not itself a String.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Facet)]
#[facet(transparent)]
#[repr(transparent)]
pub struct ColumnName(String);

impl ColumnName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// WHERE clause using ColumnName as the key type instead of String.
#[derive(Debug, Facet)]
pub struct WhereWithColumnName {
    #[facet(flatten)]
    pub filters: HashMap<ColumnName, FilterValue>,
}

#[test]
fn flatten_hashmap_with_newtype_key() {
    // Same as above but using ColumnName (a transparent newtype around String) as the key
    let input = r#"id = "$id""#;
    let result: WhereWithColumnName = toml::from_str(input).unwrap();

    assert_eq!(result.filters.len(), 1);
    let key = ColumnName::new("id");
    let value = result.filters.get(&key).expect("should have 'id' key");

    match value {
        FilterValue::EqBare(meta) => {
            assert_eq!(meta.value, "$id");
            assert!(
                meta.span.is_some(),
                "span should be populated for #[facet(other)] variant"
            );
        }
        _ => panic!("Expected EqBare variant (other fallback), got {:?}", value),
    }
}
