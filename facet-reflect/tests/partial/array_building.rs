// In ownership-based APIs, the last assignment to `partial` is often unused
// because the value is consumed by `.build()` - this is expected behavior
#![allow(unused_assignments)]

use facet::Facet;
use facet_reflect::{Partial, ReflectErrorKind};
use facet_testhelpers::{IPanic, test};

#[test]
fn test_building_array_f32_3_pushback() -> Result<(), IPanic> {
    // Test building a [f32; 3] array using set_nth_field API
    let array = Partial::alloc::<[f32; 3]>()?
        .set_nth_field(0, 1.0f32)?
        .set_nth_field(1, 2.0f32)?
        .set_nth_field(2, 3.0f32)?
        .build()?
        .materialize::<[f32; 3]>()?;

    assert_eq!(array, [1.0, 2.0, 3.0]);
    assert_eq!(array.len(), 3);
    Ok(())
}

#[test]
fn test_building_array_u8_4_pushback() -> Result<(), IPanic> {
    // Test building a [u8; 4] array using set_nth_field API
    let array = Partial::alloc::<[u8; 4]>()?
        .set_nth_field(0, 1u8)?
        .set_nth_field(1, 2u8)?
        .set_nth_field(2, 3u8)?
        .set_nth_field(3, 4u8)?
        .build()?
        .materialize::<[u8; 4]>()?;

    assert_eq!(array, [1, 2, 3, 4]);
    assert_eq!(array.len(), 4);
    Ok(())
}

#[test]
fn test_building_array_in_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithArrays {
        name: String,
        values: [f32; 3],
    }

    let mut partial: Partial<'_> = Partial::alloc::<WithArrays>()?;
    println!("Allocated WithArrays");

    partial = partial.set_field("name", "test array".to_string())?;
    println!("Set 'name' field");

    partial = partial.begin_field("values")?;
    println!("Selected 'values' field (array)");

    partial = partial.set_nth_field(0, 1.1f32)?;
    println!("Set first array element");

    partial = partial.set_nth_field(1, 2.2f32)?;
    println!("Set second array element");

    partial = partial.set_nth_field(2, 3.3f32)?;
    println!("Set third array element");

    partial = partial.end()?;
    println!("Popped from array level back to struct");

    let with_arrays = partial.build()?.materialize::<WithArrays>()?;
    println!("Built and materialized WithArrays struct");

    assert_eq!(
        with_arrays,
        WithArrays {
            name: "test array".to_string(),
            values: [1.1, 2.2, 3.3]
        }
    );
    Ok(())
}

#[test]
fn test_too_many_items_in_array() -> Result<(), IPanic> {
    // Try to set more elements than array size
    let mut partial: Partial<'_> = Partial::alloc::<[u8; 2]>()?;
    partial = partial.set_nth_field(0, 1u8)?;
    partial = partial.set_nth_field(1, 2u8)?;

    let result = partial.begin_nth_field(2); // This is the 3rd element, but the array can only hold 2 items

    match result {
        Err(err) => match err.kind {
            ReflectErrorKind::OperationFailed {
                shape: _,
                operation,
            } => {
                assert_eq!(operation, "array index out of bounds");
            }
            _ => panic!("Expected OperationFailed error, but got: {err:?}"),
        },
        Ok(_) => panic!(
            "Expected OperationFailed error for array index out of bounds, but operation succeeded"
        ),
    }
    Ok(())
}

#[test]
fn test_too_few_items_in_array() -> Result<(), IPanic> {
    let result = Partial::alloc::<[u8; 3]>()?
        .set_nth_field(0, 1u8)?
        .set_nth_field(1, 2u8)?
        // Missing third element
        .build();

    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_nested_array_building() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct NestedArrays {
        name: String,
        matrix: [[i32; 2]; 3], // 3x2 matrix
    }

    let mut partial: Partial<'_> = Partial::alloc::<NestedArrays>()?;
    println!("Allocated NestedArrays");

    partial = partial.set_field("name", "test matrix".to_string())?;
    println!("Set 'name' field");

    partial = partial.begin_field("matrix")?;
    println!("Selected 'matrix' field (outer array)");

    // First row [1, 2]
    partial = partial.begin_nth_field(0)?;
    println!("Started first row");
    partial = partial.set_nth_field(0, 1i32)?;
    partial = partial.set_nth_field(1, 2i32)?;
    partial = partial.end()?;
    println!("Completed first row");

    // Second row [3, 4]
    partial = partial.begin_nth_field(1)?;
    println!("Started second row");
    partial = partial.set_nth_field(0, 3i32)?;
    partial = partial.set_nth_field(1, 4i32)?;
    partial = partial.end()?;
    println!("Completed second row");

    // Third row [5, 6]
    partial = partial.begin_nth_field(2)?;
    println!("Started third row");
    partial = partial.set_nth_field(0, 5i32)?;
    partial = partial.set_nth_field(1, 6i32)?;
    partial = partial.end()?;
    println!("Completed third row");

    partial = partial.end()?;
    println!("Popped from outer array back to struct level");

    let nested_arrays = partial.build()?.materialize::<NestedArrays>()?;
    println!("Built and materialized NestedArrays struct");

    assert_eq!(
        nested_arrays,
        NestedArrays {
            name: "test matrix".to_string(),
            matrix: [[1, 2], [3, 4], [5, 6]]
        }
    );
    Ok(())
}

// =============================================================================
// Tests migrated from src/partial/tests.rs
// =============================================================================

#[cfg(not(miri))]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        insta::assert_snapshot!($($tt)*)
    };
}
#[cfg(miri)]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => { let _ = $($tt)*; };
}

#[test]
fn array_init() -> Result<(), IPanic> {
    let hv = Partial::alloc::<[u32; 3]>()?
        // Initialize in order
        .set_nth_field(0, 42u32)?
        .set_nth_field(1, 43u32)?
        .set_nth_field(2, 44u32)?
        .build()?
        .materialize::<[u32; 3]>()?;
    assert_eq!(hv, [42, 43, 44]);
    Ok(())
}

#[test]
fn array_init_out_of_order() -> Result<(), IPanic> {
    let hv = Partial::alloc::<[u32; 3]>()?
        // Initialize out of order
        .set_nth_field(2, 44u32)?
        .set_nth_field(0, 42u32)?
        .set_nth_field(1, 43u32)?
        .build()?
        .materialize::<[u32; 3]>()?;
    assert_eq!(hv, [42, 43, 44]);
    Ok(())
}

#[test]
fn array_partial_init() -> Result<(), IPanic> {
    // Should fail to build
    assert_snapshot!(
        Partial::alloc::<[u32; 3]>()?
            // Initialize only two elements
            .set_nth_field(0, 42u32)?
            .set_nth_field(2, 44u32)?
            .build()
            .unwrap_err()
    );
    Ok(())
}

#[test]
fn drop_array_partially_initialized() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct NoisyDrop {
        value: u64,
    }

    impl Drop for NoisyDrop {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            println!("Dropping NoisyDrop with value: {}", self.value);
        }
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    {
        let mut partial: Partial<'_> = Partial::alloc::<[NoisyDrop; 4]>()?;

        // Initialize elements 0 and 2
        partial = partial.set_nth_field(0, NoisyDrop { value: 10 })?;
        partial = partial.set_nth_field(2, NoisyDrop { value: 30 })?;

        // Drop without initializing elements 1 and 3
    }

    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        2,
        "Should drop only the two initialized array elements"
    );
    Ok(())
}

#[test]
fn array_element_set_twice() -> Result<(), IPanic> {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Facet, Debug)]
    struct DropTracker {
        id: u64,
    }

    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            println!("Dropping DropTracker with id: {}", self.id);
        }
    }

    DROP_COUNT.store(0, Ordering::SeqCst);

    let array = Partial::alloc::<[DropTracker; 3]>()?
        // Set element 0
        .set_nth_field(0, DropTracker { id: 1 })?
        // Set element 0 again - drops old value
        .set_nth_field(0, DropTracker { id: 2 })?
        // Set element 1
        .set_nth_field(1, DropTracker { id: 3 })?
        // Set element 2
        .set_nth_field(2, DropTracker { id: 4 })?
        .build()?
        .materialize::<[DropTracker; 3]>()?;

    // Verify the final array has the expected values
    assert_eq!(array[0].id, 2); // Re-initialized value
    assert_eq!(array[1].id, 3);
    assert_eq!(array[2].id, 4);

    // The first value (id: 1) should have been dropped when we re-initialized
    assert_eq!(
        DROP_COUNT.load(Ordering::SeqCst),
        1,
        "First array element should have been dropped during re-initialization"
    );
    Ok(())
}
