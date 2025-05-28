use facet::Facet;
use facet_reflect::{ReflectError, Wip};
use facet_testhelpers::test;

#[test]
fn test_building_array_f32_3_pushback() {
    // Test building a [f32; 3] array using the begin_pushback/push API
    let array = Wip::alloc::<[f32; 3]>()?
        .begin_pushback()?
        .push()?
        .put(1.0f32)?
        .pop()?
        .push()?
        .put(2.0f32)?
        .pop()?
        .push()?
        .put(3.0f32)?
        .pop()?
        .build()?
        .materialize::<[f32; 3]>()?;

    assert_eq!(array, [1.0, 2.0, 3.0]);
    assert_eq!(array.len(), 3);
}

#[test]
fn test_building_array_u8_4_pushback() {
    // Test building a [u8; 4] array using the begin_pushback/push API
    let array = Wip::alloc::<[u8; 4]>()?
        .begin_pushback()?
        .push()?
        .put(1u8)?
        .pop()?
        .push()?
        .put(2u8)?
        .pop()?
        .push()?
        .put(3u8)?
        .pop()?
        .push()?
        .put(4u8)?
        .pop()?
        .build()?
        .materialize::<[u8; 4]>()?;

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

    let mut wip = Wip::alloc::<WithArrays>()?;
    println!("Allocated WithArrays");

    wip = wip.field_named("name")?;
    println!("Selected 'name' field");

    wip = wip.put("test array".to_string())?;
    println!("Put string value to 'name' field");

    wip = wip.pop()?;
    println!("Popped back to struct level from 'name'");

    wip = wip.field_named("values")?;
    println!("Selected 'values' field (array)");

    wip = wip.begin_pushback()?;
    println!("Started array pushback");

    wip = wip.push()?;
    println!("Pushed first array element frame");

    wip = wip.put(1.1f32)?;
    println!("Put first array element value");

    wip = wip.pop()?;
    println!("Popped first array element");

    wip = wip.push()?;
    println!("Pushed second array element frame");

    wip = wip.put(2.2f32)?;
    println!("Put second array element value");

    wip = wip.pop()?;
    println!("Popped second array element");

    wip = wip.push()?;
    println!("Pushed third array element frame");

    wip = wip.put(3.3f32)?;
    println!("Put third array element value");

    wip = wip.pop()?;
    println!("Popped third array element");

    wip = wip.pop()?;
    println!("Popped from array level back to struct");

    let hv = wip.build()?;
    println!("Built heap value");

    let with_arrays = hv.materialize::<WithArrays>()?;
    println!("Materialized WithArrays struct");

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
    // Push more elements than array size
    let result = Wip::alloc::<[u8; 2]>()?
        .begin_pushback()?
        .push()?
        .put(1u8)?
        .pop()?
        .push()?
        .put(2u8)?
        .pop()?
        .push(); // This is the 3rd push, but the array can only hold 2 items

    match result {
        Err(ReflectError::ArrayIndexOutOfBounds {
            shape: _,
            index,
            size,
        }) => {
            assert_eq!(index, 2);
            assert_eq!(size, 2);
        }
        Ok(_) => panic!("Expected ArrayIndexOutOfBounds error, but push succeeded"),
        Err(e) => panic!("Expected ArrayIndexOutOfBounds error, but got: {:?}", e),
    }
}

#[test]
fn test_too_few_items_in_array() {
    let result = Wip::alloc::<[u8; 3]>()?
        .begin_pushback()?
        .push()?
        .put(1u8)?
        .pop()?
        .push()?
        .put(2u8)?
        .pop()?
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

    let mut wip = Wip::alloc::<NestedArrays>()?;
    println!("Allocated NestedArrays");

    wip = wip.field_named("name")?;
    println!("Selected 'name' field");

    wip = wip.put("test matrix".to_string())?;
    println!("Put string value to 'name' field");

    wip = wip.pop()?;
    println!("Popped back to struct level from 'name'");

    wip = wip.field_named("matrix")?;
    println!("Selected 'matrix' field (outer array)");

    wip = wip.begin_pushback()?;
    println!("Started outer array pushback");

    wip = wip.push()?;
    println!("Pushed first row frame");

    wip = wip.begin_pushback()?;
    println!("Started first inner array pushback");

    wip = wip.push()?;
    wip = wip.put(1i32)?;
    wip = wip.pop()?;
    println!("Set first row, first element (1)");

    wip = wip.push()?;
    wip = wip.put(2i32)?;
    wip = wip.pop()?;
    println!("Set first row, second element (2)");

    wip = wip.pop()?;
    println!("Popped from first inner array back to outer array");

    wip = wip.push()?;
    println!("Pushed second row frame");

    wip = wip.begin_pushback()?;
    println!("Started second inner array pushback");

    wip = wip.push()?;
    wip = wip.put(3i32)?;
    wip = wip.pop()?;
    println!("Set second row, first element (3)");

    wip = wip.push()?;
    wip = wip.put(4i32)?;
    wip = wip.pop()?;
    println!("Set second row, second element (4)");

    wip = wip.pop()?;
    println!("Popped from second inner array back to outer array");

    wip = wip.push()?;
    println!("Pushed third row frame");

    wip = wip.begin_pushback()?;
    println!("Started third inner array pushback");

    wip = wip.push()?;
    wip = wip.put(5i32)?;
    wip = wip.pop()?;
    println!("Set third row, first element (5)");

    wip = wip.push()?;
    wip = wip.put(6i32)?;
    wip = wip.pop()?;
    println!("Set third row, second element (6)");

    wip = wip.pop()?;
    println!("Popped from third inner array back to outer array");

    wip = wip.pop()?;
    println!("Popped from outer array back to struct level");

    let hv = wip.build()?;
    println!("Built heap value");

    let nested_arrays = hv.materialize::<NestedArrays>()?;
    println!("Materialized NestedArrays struct");

    assert_eq!(
        nested_arrays,
        NestedArrays {
            name: "test matrix".to_string(),
            matrix: [[1, 2], [3, 4], [5, 6]]
        }
    );
}
