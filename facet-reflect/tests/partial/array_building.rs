use facet::Facet;
use facet_reflect::{Partial, ReflectError};
use facet_testhelpers::test;

#[test]
fn test_building_array_f32_3_pushback() {
    // Test building a [f32; 3] array using set_nth_element API
    let array = *Partial::alloc::<[f32; 3]>()
        .unwrap()
        .set_nth_element(0, 1.0f32)
        .unwrap()
        .set_nth_element(1, 2.0f32)
        .unwrap()
        .set_nth_element(2, 3.0f32)
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(array, [1.0, 2.0, 3.0]);
    assert_eq!(array.len(), 3);
}

#[test]
fn test_building_array_u8_4_pushback() {
    // Test building a [u8; 4] array using set_nth_element API
    let array = *Partial::alloc::<[u8; 4]>()
        .unwrap()
        .set_nth_element(0, 1u8)
        .unwrap()
        .set_nth_element(1, 2u8)
        .unwrap()
        .set_nth_element(2, 3u8)
        .unwrap()
        .set_nth_element(3, 4u8)
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(array, [1, 2, 3, 4]);
    assert_eq!(array.len(), 4);
}

#[test]
fn test_building_array_in_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct WithArrays {
        name: String,
        values: [f32; 3],
    }

    let mut partial = Partial::alloc::<WithArrays>().unwrap();
    println!("Allocated WithArrays");

    partial.set_field("name", "test array".to_string()).unwrap();
    println!("Set 'name' field");

    partial.begin_field("values").unwrap();
    println!("Selected 'values' field (array)");

    partial.set_nth_element(0, 1.1f32).unwrap();
    println!("Set first array element");

    partial.set_nth_element(1, 2.2f32).unwrap();
    println!("Set second array element");

    partial.set_nth_element(2, 3.3f32).unwrap();
    println!("Set third array element");

    partial.end().unwrap();
    println!("Popped from array level back to struct");

    let with_arrays = *partial.build().unwrap();
    println!("Built and materialized WithArrays struct");

    assert_eq!(
        with_arrays,
        WithArrays {
            name: "test array".to_string(),
            values: [1.1, 2.2, 3.3]
        }
    );
}

#[test]
fn test_too_many_items_in_array() {
    // Try to set more elements than array size
    let mut partial = Partial::alloc::<[u8; 2]>().unwrap();
    partial.set_nth_element(0, 1u8).unwrap();
    partial.set_nth_element(1, 2u8).unwrap();

    let result = partial.begin_nth_element(2); // This is the 3rd element, but the array can only hold 2 items

    match result {
        Err(ReflectError::OperationFailed {
            shape: _,
            operation,
        }) => {
            assert_eq!(operation, "array index out of bounds");
        }
        Ok(_) => panic!(
            "Expected OperationFailed error for array index out of bounds, but operation succeeded"
        ),
        Err(e) => panic!("Expected OperationFailed error, but got: {e:?}"),
    }
}

#[test]
fn test_too_few_items_in_array() {
    let result = Partial::alloc::<[u8; 3]>()
        .unwrap()
        .set_nth_element(0, 1u8)
        .unwrap()
        .set_nth_element(1, 2u8)
        .unwrap()
        // Missing third element
        .build();

    assert!(result.is_err());
}

#[test]
fn test_nested_array_building() {
    #[derive(Facet, Debug, PartialEq)]
    struct NestedArrays {
        name: String,
        matrix: [[i32; 2]; 3], // 3x2 matrix
    }

    let mut partial = Partial::alloc::<NestedArrays>().unwrap();
    println!("Allocated NestedArrays");

    partial
        .set_field("name", "test matrix".to_string())
        .unwrap();
    println!("Set 'name' field");

    partial.begin_field("matrix").unwrap();
    println!("Selected 'matrix' field (outer array)");

    // First row [1, 2]
    partial.begin_nth_element(0).unwrap();
    println!("Started first row");
    partial.set_nth_element(0, 1i32).unwrap();
    partial.set_nth_element(1, 2i32).unwrap();
    partial.end().unwrap();
    println!("Completed first row");

    // Second row [3, 4]
    partial.begin_nth_element(1).unwrap();
    println!("Started second row");
    partial.set_nth_element(0, 3i32).unwrap();
    partial.set_nth_element(1, 4i32).unwrap();
    partial.end().unwrap();
    println!("Completed second row");

    // Third row [5, 6]
    partial.begin_nth_element(2).unwrap();
    println!("Started third row");
    partial.set_nth_element(0, 5i32).unwrap();
    partial.set_nth_element(1, 6i32).unwrap();
    partial.end().unwrap();
    println!("Completed third row");

    partial.end().unwrap();
    println!("Popped from outer array back to struct level");

    let nested_arrays = *partial.build().unwrap();
    println!("Built and materialized NestedArrays struct");

    assert_eq!(
        nested_arrays,
        NestedArrays {
            name: "test matrix".to_string(),
            matrix: [[1, 2], [3, 4], [5, 6]]
        }
    );
}
