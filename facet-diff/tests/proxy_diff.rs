//! Tests for diffing types with proxy attributes.
//!
//! These tests verify that `facet-diff` correctly handles field-level and
//! container-level proxy attributes when comparing values.

use facet::Facet;
use facet_diff::FacetDiff;
use facet_testhelpers::test;

// =============================================================================
// Opaque type with proxy (simulates types like DashMap that can't derive Facet)
// =============================================================================

/// An opaque type that doesn't derive Facet directly.
#[derive(Clone, Debug, PartialEq)]
pub struct OpaqueCounter {
    count: u64,
}

impl OpaqueCounter {
    pub fn new(count: u64) -> Self {
        Self { count }
    }
}

/// Proxy type that derives Facet for serialization/comparison.
#[derive(Facet, Clone, Debug, PartialEq)]
#[facet(auto_traits)]
pub struct OpaqueCounterProxy {
    pub count: u64,
}

impl TryFrom<OpaqueCounterProxy> for OpaqueCounter {
    type Error = &'static str;
    fn try_from(proxy: OpaqueCounterProxy) -> Result<Self, Self::Error> {
        Ok(OpaqueCounter { count: proxy.count })
    }
}

impl TryFrom<&OpaqueCounter> for OpaqueCounterProxy {
    type Error = &'static str;
    fn try_from(val: &OpaqueCounter) -> Result<Self, Self::Error> {
        Ok(OpaqueCounterProxy { count: val.count })
    }
}

// =============================================================================
// Struct with opaque proxy field
// =============================================================================

#[derive(Facet, Clone, Debug)]
#[facet(auto_traits)]
pub struct StructWithOpaqueField {
    pub name: String,
    #[facet(opaque, proxy = OpaqueCounterProxy)]
    pub counter: OpaqueCounter,
}

#[test]
fn diff_struct_with_opaque_proxy_field_equal() {
    let a = StructWithOpaqueField {
        name: "test".to_string(),
        counter: OpaqueCounter::new(42),
    };
    let b = StructWithOpaqueField {
        name: "test".to_string(),
        counter: OpaqueCounter::new(42),
    };

    let diff = a.diff(&b);
    assert!(diff.is_equal(), "Expected equal diff, got: {diff}");
}

#[test]
fn diff_struct_with_opaque_proxy_field_name_different() {
    let a = StructWithOpaqueField {
        name: "alice".to_string(),
        counter: OpaqueCounter::new(42),
    };
    let b = StructWithOpaqueField {
        name: "bob".to_string(),
        counter: OpaqueCounter::new(42),
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff");
    let diff_str = format!("{diff}");
    assert!(
        diff_str.contains("name"),
        "Diff should mention 'name' field: {diff_str}"
    );
}

#[test]
fn diff_struct_with_opaque_proxy_field_counter_different() {
    let a = StructWithOpaqueField {
        name: "test".to_string(),
        counter: OpaqueCounter::new(10),
    };
    let b = StructWithOpaqueField {
        name: "test".to_string(),
        counter: OpaqueCounter::new(20),
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff");
    let diff_str = format!("{diff}");
    assert!(
        diff_str.contains("counter"),
        "Diff should mention 'counter' field: {diff_str}"
    );
}

#[test]
fn diff_struct_with_opaque_proxy_field_both_different() {
    let a = StructWithOpaqueField {
        name: "alice".to_string(),
        counter: OpaqueCounter::new(10),
    };
    let b = StructWithOpaqueField {
        name: "bob".to_string(),
        counter: OpaqueCounter::new(20),
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff");
    let diff_str = format!("{diff}");
    assert!(
        diff_str.contains("name") && diff_str.contains("counter"),
        "Diff should mention both fields: {diff_str}"
    );
}

// =============================================================================
// Enum with opaque proxy field
// =============================================================================

#[derive(Facet, Clone, Debug)]
#[facet(auto_traits)]
#[repr(u8)]
pub enum EnumWithOpaqueField {
    Empty,
    WithCounter {
        label: String,
        #[facet(opaque, proxy = OpaqueCounterProxy)]
        counter: OpaqueCounter,
    },
}

#[test]
fn diff_enum_with_opaque_proxy_field_equal() {
    let a = EnumWithOpaqueField::WithCounter {
        label: "test".to_string(),
        counter: OpaqueCounter::new(42),
    };
    let b = EnumWithOpaqueField::WithCounter {
        label: "test".to_string(),
        counter: OpaqueCounter::new(42),
    };

    let diff = a.diff(&b);
    assert!(diff.is_equal(), "Expected equal diff, got: {diff}");
}

#[test]
fn diff_enum_with_opaque_proxy_field_counter_different() {
    let a = EnumWithOpaqueField::WithCounter {
        label: "test".to_string(),
        counter: OpaqueCounter::new(10),
    };
    let b = EnumWithOpaqueField::WithCounter {
        label: "test".to_string(),
        counter: OpaqueCounter::new(20),
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff");
    let diff_str = format!("{diff}");
    assert!(
        diff_str.contains("counter"),
        "Diff should mention 'counter' field: {diff_str}"
    );
}

#[test]
fn diff_enum_different_variants() {
    let a = EnumWithOpaqueField::Empty;
    let b = EnumWithOpaqueField::WithCounter {
        label: "test".to_string(),
        counter: OpaqueCounter::new(42),
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff for different variants");
}

// =============================================================================
// Multiple opaque fields
// =============================================================================

#[derive(Facet, Clone, Debug)]
#[facet(auto_traits)]
pub struct MultipleOpaqueFields {
    #[facet(opaque, proxy = OpaqueCounterProxy)]
    pub first: OpaqueCounter,
    #[facet(opaque, proxy = OpaqueCounterProxy)]
    pub second: OpaqueCounter,
}

#[test]
fn diff_multiple_opaque_fields_equal() {
    let a = MultipleOpaqueFields {
        first: OpaqueCounter::new(10),
        second: OpaqueCounter::new(20),
    };
    let b = MultipleOpaqueFields {
        first: OpaqueCounter::new(10),
        second: OpaqueCounter::new(20),
    };

    let diff = a.diff(&b);
    assert!(diff.is_equal(), "Expected equal diff, got: {diff}");
}

#[test]
fn diff_multiple_opaque_fields_first_different() {
    let a = MultipleOpaqueFields {
        first: OpaqueCounter::new(10),
        second: OpaqueCounter::new(20),
    };
    let b = MultipleOpaqueFields {
        first: OpaqueCounter::new(99),
        second: OpaqueCounter::new(20),
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff");
    let diff_str = format!("{diff}");
    assert!(
        diff_str.contains("first"),
        "Diff should mention 'first' field: {diff_str}"
    );
}

#[test]
fn diff_multiple_opaque_fields_both_different() {
    let a = MultipleOpaqueFields {
        first: OpaqueCounter::new(10),
        second: OpaqueCounter::new(20),
    };
    let b = MultipleOpaqueFields {
        first: OpaqueCounter::new(99),
        second: OpaqueCounter::new(88),
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff");
    let diff_str = format!("{diff}");
    assert!(
        diff_str.contains("first") && diff_str.contains("second"),
        "Diff should mention both fields: {diff_str}"
    );
}

// =============================================================================
// Nested struct with opaque field
// =============================================================================

#[derive(Facet, Clone, Debug)]
#[facet(auto_traits)]
pub struct OuterStruct {
    pub id: u32,
    pub inner: StructWithOpaqueField,
}

#[test]
fn diff_nested_struct_with_opaque_field_equal() {
    let a = OuterStruct {
        id: 1,
        inner: StructWithOpaqueField {
            name: "test".to_string(),
            counter: OpaqueCounter::new(42),
        },
    };
    let b = OuterStruct {
        id: 1,
        inner: StructWithOpaqueField {
            name: "test".to_string(),
            counter: OpaqueCounter::new(42),
        },
    };

    let diff = a.diff(&b);
    assert!(diff.is_equal(), "Expected equal diff, got: {diff}");
}

#[test]
fn diff_nested_struct_with_opaque_field_inner_counter_different() {
    let a = OuterStruct {
        id: 1,
        inner: StructWithOpaqueField {
            name: "test".to_string(),
            counter: OpaqueCounter::new(10),
        },
    };
    let b = OuterStruct {
        id: 1,
        inner: StructWithOpaqueField {
            name: "test".to_string(),
            counter: OpaqueCounter::new(20),
        },
    };

    let diff = a.diff(&b);
    assert!(!diff.is_equal(), "Expected different diff");
    let diff_str = format!("{diff}");
    assert!(
        diff_str.contains("inner") && diff_str.contains("counter"),
        "Diff should mention nested path: {diff_str}"
    );
}
