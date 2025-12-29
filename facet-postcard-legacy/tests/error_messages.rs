//! Tests for error message quality with path tracking
//!
//! Uses `format_simple()` for stable snapshots that don't change based on features.
//! See `pretty_error_messages.rs` for tests of pretty-formatted output.

use facet::Facet;
use facet_core::ConstTypeId;
use facet_postcard_legacy::to_vec;

/// Test that unsupported scalars show the path where they occurred
#[test]
fn test_unknown_scalar_error_shows_path() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Inner {
        name: String,
        type_id: ConstTypeId,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        label: String,
        inner: Inner,
    }

    let value = Outer {
        label: "test".to_string(),
        inner: Inner {
            name: "example".to_string(),
            type_id: ConstTypeId::of::<String>(),
        },
    };

    let result = to_vec(&value);
    assert!(
        result.is_err(),
        "Expected error for unsupported ConstTypeId"
    );

    let err = result.unwrap_err();
    // Use format_simple() for stable output regardless of feature flags
    let err_msg = err.format_simple();

    // The error message should show the path to the problematic field
    insta::assert_snapshot!(err_msg);
}

/// Test error path through a Vec
#[test]
fn test_error_path_through_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Item {
        id: u32,
        type_info: ConstTypeId,
    }

    #[derive(Facet, Debug)]
    struct Container {
        items: Vec<Item>,
    }

    let value = Container {
        items: vec![
            Item {
                id: 1,
                type_info: ConstTypeId::of::<u32>(),
            },
            Item {
                id: 2,
                type_info: ConstTypeId::of::<String>(),
            },
        ],
    };

    let result = to_vec(&value);
    assert!(result.is_err());

    let err = result.unwrap_err();
    // Use format_simple() for stable output regardless of feature flags
    let err_msg = err.format_simple();

    // Should show path like items[0].type_info
    insta::assert_snapshot!(err_msg);
}

/// Test error path through Option
#[test]
fn test_error_path_through_option() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Config {
        name: String,
        debug_type: Option<ConstTypeId>,
    }

    let value = Config {
        name: "test".to_string(),
        debug_type: Some(ConstTypeId::of::<i32>()),
    };

    let result = to_vec(&value);
    assert!(result.is_err());

    let err = result.unwrap_err();
    // Use format_simple() for stable output regardless of feature flags
    let err_msg = err.format_simple();

    // Should show path through the Option
    insta::assert_snapshot!(err_msg);
}

/// Test error path through enum variant
#[test]
fn test_error_path_through_enum() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    #[repr(C)]
    #[allow(dead_code)]
    enum TypedValue {
        Simple(u32),
        WithType { value: u32, type_id: ConstTypeId },
    }

    let value = TypedValue::WithType {
        value: 42,
        type_id: ConstTypeId::of::<bool>(),
    };

    let result = to_vec(&value);
    assert!(result.is_err());

    let err = result.unwrap_err();
    // Use format_simple() for stable output regardless of feature flags
    let err_msg = err.format_simple();

    // Should show path through the enum variant
    insta::assert_snapshot!(err_msg);
}
