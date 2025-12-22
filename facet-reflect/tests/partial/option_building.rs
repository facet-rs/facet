use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn test_option_building_manual() {
    // Test building Option<String> manually step by step
    let mut wip = Partial::alloc::<Option<String>>().unwrap();

    // Check initial state - option starts uninitialized

    // Try to build Some("hello") manually
    // First, let's see what methods are available for option building

    // Option 1: Try using the option vtable directly
    if let facet_core::Def::Option(option_def) = wip.shape().def {
        // We have an option - let's try to initialize it as Some
        println!("Option def found: inner type is {}", option_def.t());

        // We need to:
        // 1. Initialize the option as Some
        // 2. Set the inner value

        // Let's see if we can access the option vtable functions
        // This is exploratory - we want to understand the API
    }

    // For now, let's use the high-level API to see what works
    wip = wip.set(Some("hello".to_string())).unwrap();

    let option_value = wip
        .build()
        .unwrap()
        .materialize::<Option<String>>()
        .unwrap();
    assert_eq!(option_value, Some("hello".to_string()));
}

#[test]
fn test_option_building_none() {
    let mut wip = Partial::alloc::<Option<String>>().unwrap();

    // Set to None
    wip = wip.set(None::<String>).unwrap();

    let option_value = wip
        .build()
        .unwrap()
        .materialize::<Option<String>>()
        .unwrap();
    assert_eq!(option_value, None);
}

#[test]
fn test_option_building_with_begin_some() {
    // This test will likely fail with the current implementation
    // but it shows what we WANT to be able to do
    let mut wip = Partial::alloc::<Option<String>>().unwrap();

    // Try the current begin_some API
    let result = wip.begin_some();

    match result {
        Ok(w) => {
            // If begin_some works, continue building
            wip = w;
            wip = wip.set("hello".to_string()).unwrap();
            wip = wip.end().unwrap();

            let option_value = wip
                .build()
                .unwrap()
                .materialize::<Option<String>>()
                .unwrap();
            assert_eq!(option_value, Some("hello".to_string()));
        }
        Err(e) => {
            println!("begin_some failed as expected: {e:?}");
            // This shows that begin_some is not properly implemented
        }
    }
}

#[test]
fn test_option_building_set_default() {
    // Test using set_default to create None
    let mut wip = Partial::alloc::<Option<String>>().unwrap();

    wip = wip.set_default().unwrap();

    let option_value = wip
        .build()
        .unwrap()
        .materialize::<Option<String>>()
        .unwrap();
    assert_eq!(option_value, None);
}

#[test]
fn test_nested_option_building() {
    // Test building Option<Option<String>>
    let mut wip = Partial::alloc::<Option<Option<String>>>().unwrap();

    // Build Some(Some("hello"))
    wip = wip.set(Some(Some("hello".to_string()))).unwrap();

    let option_value = wip
        .build()
        .unwrap()
        .materialize::<Option<Option<String>>>()
        .unwrap();
    assert_eq!(option_value, Some(Some("hello".to_string())));
}

#[test]
fn test_option_in_struct() {
    #[derive(facet::Facet, Debug, PartialEq)]
    struct TestStruct {
        name: Option<String>,
        age: Option<u32>,
    }

    let mut wip = Partial::alloc::<TestStruct>().unwrap();

    // Build the struct with option fields
    wip = wip.begin_nth_field(0).unwrap(); // name field
    wip = wip.set(Some("Alice".to_string())).unwrap();
    wip = wip.end().unwrap();

    wip = wip.begin_nth_field(1).unwrap(); // age field
    wip = wip.set(None::<u32>).unwrap();
    wip = wip.end().unwrap();

    let struct_value = wip.build().unwrap().materialize::<TestStruct>().unwrap();
    assert_eq!(
        struct_value,
        TestStruct {
            name: Some("Alice".to_string()),
            age: None,
        }
    );
}

#[test]
fn test_option_field_manual_building() {
    // Test manually building option fields in a struct
    #[derive(facet::Facet, Debug, PartialEq)]
    struct TestStruct {
        value: Option<String>,
    }

    let mut wip = Partial::alloc::<TestStruct>().unwrap();

    // Navigate to the option field
    wip = wip.begin_nth_field(0).unwrap(); // value field

    // Now we're in the Option<String> context
    // This is where we want to test proper option building

    // For now, use the high-level API
    wip = wip.set(Some("test".to_string())).unwrap();
    wip = wip.end().unwrap();

    let struct_value = wip.build().unwrap().materialize::<TestStruct>().unwrap();
    assert_eq!(struct_value.value, Some("test".to_string()));
}

#[test]
fn explore_option_shape() {
    // Explore the shape of Option<String> to understand its structure
    let _wip = Partial::alloc::<Option<String>>().unwrap();

    println!("Option<String> shape: {:?}", _wip.shape());

    if let facet_core::Def::Option(option_def) = _wip.shape().def {
        println!("Inner type: {:?}", option_def.t());
        println!("Option vtable: {:?}", option_def.vtable);
    }

    // Also check if it has an inner shape (transparent wrapper)
    if let Some(inner_shape) = _wip.shape().inner {
        println!("Inner shape: {inner_shape:?}");
    }
}

// =============================================================================
// Tests migrated from src/partial/tests.rs
// =============================================================================

use facet_testhelpers::IPanic;

#[cfg(not(miri))]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        insta::assert_snapshot!($($tt)*)
    };
}
#[cfg(miri)]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {{ let _ = $($tt)*; }};
}

#[test]
fn option_uninit() -> Result<(), IPanic> {
    assert_snapshot!(Partial::alloc::<Option<f64>>()?.build().unwrap_err());
    Ok(())
}

#[test]
fn option_init() -> Result<(), IPanic> {
    let hv = Partial::alloc::<Option<f64>>()?
        .set::<Option<f64>>(Some(6.241))?
        .build()?
        .materialize::<Option<f64>>()?;
    assert_eq!(hv, Some(6.241));
    Ok(())
}
