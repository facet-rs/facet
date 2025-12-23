//! Showcase of facet-postcard error messages
//!
//! This example demonstrates the pretty error output when serialization
//! fails due to unsupported types, showing the full path through nested types.
//!
//! Run with: cargo run --example postcard_showcase

use facet::Facet;
use facet_core::ConstTypeId;
use facet_postcard::to_vec;
use facet_showcase::ShowcaseRunner;

fn main() {
    let mut runner = ShowcaseRunner::new("Postcard Errors");
    runner.header();
    runner.intro("[`facet-postcard`](https://docs.rs/facet-postcard) provides binary serialization using the postcard format, optimized for embedded and no_std environments. When serialization fails due to unsupported types, the error messages show the full path through nested type hierarchies.");

    // =========================================================================
    // Serialization Error Examples
    // =========================================================================

    scenario_nested_struct_error(&mut runner);
    scenario_vec_error(&mut runner);
    scenario_option_error(&mut runner);
    scenario_enum_error(&mut runner);
    scenario_deeply_nested_error(&mut runner);

    runner.footer();
}

// ============================================================================
// Type Definitions
// ============================================================================

// --- Nested struct with unsupported type ---

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

// --- Vec containing unsupported type ---

#[derive(Facet, Debug)]
struct Item {
    id: u32,
    type_info: ConstTypeId,
}

#[derive(Facet, Debug)]
struct Container {
    items: Vec<Item>,
}

// --- Option containing unsupported type ---

#[derive(Facet, Debug)]
struct Config {
    name: String,
    debug_type: Option<ConstTypeId>,
}

// --- Enum with unsupported type in variant ---

#[derive(Facet, Debug)]
#[repr(C)]
enum TypedValue {
    Simple(u32),
    WithType { value: u32, type_id: ConstTypeId },
}

// --- Deeply nested structure ---

#[derive(Facet, Debug)]
struct Level3 {
    data: String,
    marker: ConstTypeId,
}

#[derive(Facet, Debug)]
struct Level2 {
    name: String,
    level3: Level3,
}

#[derive(Facet, Debug)]
struct Level1 {
    id: u32,
    level2: Level2,
}

#[derive(Facet, Debug)]
struct Root {
    label: String,
    level1: Level1,
}

// ============================================================================
// Scenarios
// ============================================================================

fn scenario_nested_struct_error(runner: &mut ShowcaseRunner) {
    let value = Outer {
        label: "test".to_string(),
        inner: Inner {
            name: "example".to_string(),
            type_id: ConstTypeId::of::<String>(),
        },
    };

    let result: Result<Vec<u8>, _> = to_vec(&value);
    let err = result.unwrap_err();
    let diagnostic = err.to_diagnostic().expect("should have diagnostic");

    runner
        .scenario("Nested Struct Error")
        .description(
            "When serialization fails inside a nested struct, the error shows:\n\
             1. The exact field where the error occurred (leaf type)\n\
             2. The path back to the root type ('via this field')",
        )
        .target_type::<Outer>()
        .error(&diagnostic)
        .finish();
}

fn scenario_vec_error(runner: &mut ShowcaseRunner) {
    let value = Container {
        items: vec![Item {
            id: 1,
            type_info: ConstTypeId::of::<u32>(),
        }],
    };

    let result: Result<Vec<u8>, _> = to_vec(&value);
    let err = result.unwrap_err();
    let diagnostic = err.to_diagnostic().expect("should have diagnostic");

    runner
        .scenario("Error Through Vec")
        .description(
            "Errors inside Vec elements show the container field and the element type.\n\
             The path navigates through the Vec to show exactly where the problem is.",
        )
        .target_type::<Container>()
        .error(&diagnostic)
        .finish();
}

fn scenario_option_error(runner: &mut ShowcaseRunner) {
    let value = Config {
        name: "test".to_string(),
        debug_type: Some(ConstTypeId::of::<i32>()),
    };

    let result: Result<Vec<u8>, _> = to_vec(&value);
    let err = result.unwrap_err();
    let diagnostic = err.to_diagnostic().expect("should have diagnostic");

    runner
        .scenario("Error Through Option")
        .description(
            "When an unsupported type is wrapped in Option<T>, the error\n\
             shows the containing struct's field that holds the Option.",
        )
        .target_type::<Config>()
        .error(&diagnostic)
        .finish();
}

fn scenario_enum_error(runner: &mut ShowcaseRunner) {
    let value = TypedValue::WithType {
        value: 42,
        type_id: ConstTypeId::of::<bool>(),
    };

    let result: Result<Vec<u8>, _> = to_vec(&value);
    let err = result.unwrap_err();
    let diagnostic = err.to_diagnostic().expect("should have diagnostic");

    runner
        .scenario("Error in Enum Variant")
        .description(
            "Errors inside enum variants show the variant name and the problematic field.\n\
             The diagnostic points directly to the field within the variant.",
        )
        .target_type::<TypedValue>()
        .error(&diagnostic)
        .finish();
}

fn scenario_deeply_nested_error(runner: &mut ShowcaseRunner) {
    let value = Root {
        label: "root".to_string(),
        level1: Level1 {
            id: 1,
            level2: Level2 {
                name: "level2".to_string(),
                level3: Level3 {
                    data: "level3".to_string(),
                    marker: ConstTypeId::of::<()>(),
                },
            },
        },
    };

    let result: Result<Vec<u8>, _> = to_vec(&value);
    let err = result.unwrap_err();
    let diagnostic = err.to_diagnostic().expect("should have diagnostic");

    runner
        .scenario("Deeply Nested Error")
        .description(
            "For deeply nested types (4+ levels), the error traces the full path:\n\
             Root → Level1 → Level2 → Level3 → marker field.\n\
             Each level shows 'via this field' to help you navigate.",
        )
        .target_type::<Root>()
        .error(&diagnostic)
        .finish();
}
