use facet::Facet;
use facet_reflect::{PokeValueUninit, ReflectError};
use std::{fmt::Debug, string::String};

// Simple test structures
#[derive(Debug, PartialEq, Eq, Facet, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug, PartialEq, Eq, Facet)]
struct NamedPoint {
    name: String,
    point: Point,
}

#[derive(Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
enum Direction {
    North,
    East,
    South,
    West,
}

// Tests for basic allocation and shapes
#[test]
fn test_allocate_and_check_shape() {
    facet_testhelpers::setup();

    let (poke, guard) = PokeValueUninit::alloc::<i32>();
    assert_eq!(poke.shape(), i32::SHAPE);
    drop((poke, guard)); // Clean up

    let (poke, guard) = PokeValueUninit::alloc_shape(Point::SHAPE);
    assert_eq!(poke.shape(), Point::SHAPE);
    drop((poke, guard)); // Clean up
}

// Test initializing with default value
#[test]
fn test_default_initialization() {
    facet_testhelpers::setup();

    // Test with a primitive that has a default
    let (poke, guard) = PokeValueUninit::alloc::<i32>();
    let poke = poke
        .default_in_place()
        .expect("i32 should have default impl");
    assert_eq!(*poke.as_ref::<i32>(), 0);
    drop((poke, guard)); // Clean up

    // Test with a struct that has a default
    let (poke, guard) = PokeValueUninit::alloc::<Point>();
    let poke = poke
        .default_in_place()
        .expect("Point should have default impl");
    assert_eq!(*poke.as_ref::<Point>(), Point { x: 0, y: 0 });
    drop((poke, guard)); // Clean up
}

// Test direct value initialization
#[test]
fn test_put_value() {
    facet_testhelpers::setup();

    // For a simple type
    let (poke, guard) = PokeValueUninit::alloc::<i32>();
    let poke = poke.put(42i32).expect("Should accept correct type");
    assert_eq!(*poke.as_ref::<i32>(), 42);
    drop((poke, guard)); // Clean up

    // For a compound type
    let (poke, guard) = PokeValueUninit::alloc::<Point>();
    let point = Point { x: 10, y: 20 };
    let poke = poke.put(point).expect("Should accept correct type");
    assert_eq!(*poke.as_ref::<Point>(), Point { x: 10, y: 20 });
    drop((poke, guard)); // Clean up
}

// Test type mismatch in put
#[test]
fn test_put_wrong_type() {
    facet_testhelpers::setup();

    let (poke, guard) = PokeValueUninit::alloc::<i32>();
    let result = poke.put(42i64); // Wrong type

    match result {
        Err(ReflectError::WrongShape { expected, actual }) => {
            assert_eq!(expected, i32::SHAPE);
            assert_eq!(actual, i64::SHAPE);
        }
        _ => panic!("Expected WrongShape error"),
    }

    drop(guard); // Clean up
}

// Test parsing from string
#[test]
fn test_parse_from_string() {
    facet_testhelpers::setup();

    // Parse a number
    let (poke, guard) = PokeValueUninit::alloc::<i32>();
    let poke = poke
        .parse("42")
        .expect("i32 should be parseable from string");
    assert_eq!(*poke.as_ref::<i32>(), 42);
    drop((poke, guard)); // Clean up

    // Try parsing an invalid string
    let (poke, guard) = PokeValueUninit::alloc::<i32>();
    let result = poke.parse("not a number");
    assert!(result.is_err(), "Parsing invalid string should fail");
    drop(guard); // Clean up
}

// Test scalar type identification
#[test]
fn test_scalar_type_identification() {
    facet_testhelpers::setup();

    // For a primitive type
    let (poke, _guard) = PokeValueUninit::alloc::<i32>();
    assert!(poke.scalar_type().is_some());

    // For a non-scalar type
    let (poke, _guard) = PokeValueUninit::alloc::<Point>();
    assert!(poke.scalar_type().is_none());
}

// We can't directly test try_from because we can't access the private OpaqueConst
// directly. Instead, we'll test direct value initialization.
#[test]
fn test_direct_initialization() {
    facet_testhelpers::setup();

    // For a primitive
    let (poke, guard) = PokeValueUninit::alloc::<u16>();
    let poke = poke.put(42u16).expect("Should accept u16");
    assert_eq!(*poke.as_ref::<u16>(), 42u16);
    drop((poke, guard)); // Clean up

    // For a different primitive
    let (poke, guard) = PokeValueUninit::alloc::<f64>();
    let poke = poke.put(3.2f64).expect("Should accept f64");
    assert_eq!(*poke.as_ref::<f64>(), 3.2f64);
    drop((poke, guard)); // Clean up
}

// Test for different struct shape types
#[test]
fn test_into_struct() {
    facet_testhelpers::setup();

    // Should succeed for struct type
    let (poke, guard) = PokeValueUninit::alloc::<Point>();
    assert!(poke.into_struct().is_ok());
    drop(guard); // Clean up

    // Should fail for non-struct type
    let (poke, guard) = PokeValueUninit::alloc::<i32>();
    assert!(poke.into_struct().is_err());
    drop(guard); // Clean up
}

// Test for enum shape types
#[test]
fn test_into_enum() {
    facet_testhelpers::setup();

    // Should succeed for enum type
    let (poke, guard) = PokeValueUninit::alloc::<Direction>();
    assert!(poke.into_enum().is_ok());
    drop(guard); // Clean up

    // Should fail for non-enum type
    let (poke, guard) = PokeValueUninit::alloc::<Point>();
    assert!(poke.into_enum().is_err());
    drop(guard); // Clean up
}

// Test that allocate and build work together
#[test]
fn test_alloc_and_build() -> eyre::Result<()> {
    facet_testhelpers::setup();

    // Allocate and initialize a struct value
    let (poke, guard) = PokeValueUninit::alloc::<Point>();
    let poke = poke.into_struct()?;
    let poke = poke.field_by_name("x")?.set(42i32)?.into_struct_uninit();
    let poke = poke.field_by_name("y")?.set(24i32)?.into_struct_uninit();

    // Build the final value
    let point: Point = poke.build(Some(guard))?;

    assert_eq!(point, Point { x: 42, y: 24 });
    Ok(())
}

// Test complex nested initialization
#[test]
fn test_nested_initialization() -> eyre::Result<()> {
    facet_testhelpers::setup();

    // Allocate a NamedPoint which contains a Point
    let (poke, guard) = PokeValueUninit::alloc::<NamedPoint>();
    let poke = poke.into_struct()?;

    // Set the name field
    let poke = poke.field_by_name("name")?;
    let poke = poke.set(String::from("Origin"))?.into_struct_uninit();

    // Set the point field by going into the nested struct
    let poke = poke.field_by_name("point")?;
    let poke = poke.into_struct()?;
    let poke = poke.field_by_name("x")?.set(0i32)?.into_struct_slot();
    let poke = poke.field_by_name("y")?.set(0i32)?.into_struct_slot();
    let poke = poke.finish()?.into_struct_uninit();

    // Build the final value
    let named_point: NamedPoint = poke.build(Some(guard))?;

    assert_eq!(named_point.name, "Origin");
    assert_eq!(named_point.point, Point { x: 0, y: 0 });

    Ok(())
}
