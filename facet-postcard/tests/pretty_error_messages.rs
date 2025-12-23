//! Tests for pretty error message rendering with path tracking.
//!
//! These tests verify the multi-type diagnostic output when pretty-errors is enabled.
//! Uses `to_string()` which renders with pretty formatting when the feature is on.

#![cfg(feature = "pretty-errors")]

use facet::Facet;
use facet_core::ConstTypeId;
use facet_postcard::to_vec;

/// Test pretty error through nested struct shows both types
#[test]
fn test_pretty_nested_struct_error() {
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
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Pretty output should show both Outer and Inner types
    insta::assert_snapshot!(err_msg);
}

/// Test pretty error through Vec shows Container and Item types
#[test]
fn test_pretty_vec_error() {
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
    let err_msg = err.to_string();

    // Should show path through Container -> Item
    insta::assert_snapshot!(err_msg);
}

/// Test pretty error through Option shows the containing type
#[test]
fn test_pretty_option_error() {
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
    let err_msg = err.to_string();

    insta::assert_snapshot!(err_msg);
}

/// Test pretty error through enum variant
#[test]
fn test_pretty_enum_error() {
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
    let err_msg = err.to_string();

    insta::assert_snapshot!(err_msg);
}
