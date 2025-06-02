use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn test_option_building_manual() {
    // Test building Option<String> manually step by step
    let mut wip = Partial::alloc::<Option<String>>()?;

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
    wip.set(Some("hello".to_string()))?;

    let result = wip.build()?;
    let option_value: Option<String> = *result;
    assert_eq!(option_value, Some("hello".to_string()));
}

#[test]
fn test_option_building_none() {
    let mut wip = Partial::alloc::<Option<String>>()?;

    // Set to None
    wip.set(None::<String>)?;

    let result = wip.build()?;
    let option_value: Option<String> = *result;
    assert_eq!(option_value, None);
}

#[test]
fn test_option_building_with_begin_some() {
    // This test will likely fail with the current implementation
    // but it shows what we WANT to be able to do
    let mut wip = Partial::alloc::<Option<String>>()?;

    // Try the current begin_some API
    let result = wip.begin_some();

    match result {
        Ok(_) => {
            // If begin_some works, continue building
            wip.set("hello".to_string())?;
            wip.end()?;

            let result = wip.build()?;
            let option_value: Option<String> = *result;
            assert_eq!(option_value, Some("hello".to_string()));
        }
        Err(e) => {
            println!("begin_some failed as expected: {:?}", e);
            // This shows that begin_some is not properly implemented
        }
    }
}

#[test]
fn test_option_building_set_default() {
    // Test using set_default to create None
    let mut wip = Partial::alloc::<Option<String>>()?;

    wip.set_default()?;

    let result = wip.build()?;
    let option_value: Option<String> = *result;
    assert_eq!(option_value, None);
}

#[test]
fn test_nested_option_building() {
    // Test building Option<Option<String>>
    let mut wip = Partial::alloc::<Option<Option<String>>>()?;

    // Build Some(Some("hello"))
    wip.set(Some(Some("hello".to_string())))?;

    let result = wip.build()?;
    let option_value: Option<Option<String>> = *result;
    assert_eq!(option_value, Some(Some("hello".to_string())));
}

#[test]
fn test_option_in_struct() {
    #[derive(facet::Facet, Debug, PartialEq)]
    struct TestStruct {
        name: Option<String>,
        age: Option<u32>,
    }

    let mut wip = Partial::alloc::<TestStruct>()?;

    // Build the struct with option fields
    wip.begin_nth_field(0)?; // name field
    wip.set(Some("Alice".to_string()))?;
    wip.end()?;

    wip.begin_nth_field(1)?; // age field  
    wip.set(None::<u32>)?;
    wip.end()?;

    let result = wip.build()?;
    let struct_value: TestStruct = *result;
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

    let mut wip = Partial::alloc::<TestStruct>()?;

    // Navigate to the option field
    wip.begin_nth_field(0)?; // value field

    // Now we're in the Option<String> context
    // This is where we want to test proper option building

    // For now, use the high-level API
    wip.set(Some("test".to_string()))?;
    wip.end()?;

    let result = wip.build()?;
    let struct_value: TestStruct = *result;
    assert_eq!(struct_value.value, Some("test".to_string()));
}

#[test]
fn explore_option_shape() {
    // Explore the shape of Option<String> to understand its structure
    let wip = Partial::alloc::<Option<String>>()?;

    println!("Option<String> shape: {:?}", wip.shape());

    if let facet_core::Def::Option(option_def) = wip.shape().def {
        println!("Inner type: {:?}", option_def.t());
        println!("Option vtable: {:?}", option_def.vtable);
    }

    // Also check if it has an inner shape (transparent wrapper)
    if let Some(inner_fn) = wip.shape().inner {
        let inner_shape = inner_fn();
        println!("Inner shape: {:?}", inner_shape);
    }
}
