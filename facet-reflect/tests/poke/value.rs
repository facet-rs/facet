use facet::Facet;
use facet_reflect::{PokeValueUninit, ReflectError};
use std::fmt::Debug;

// Simple test structures
#[derive(Debug, PartialEq, Eq, Facet, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
enum Direction {
    #[allow(dead_code)]
    North,
    #[allow(dead_code)]
    East,
    #[allow(dead_code)]
    South,
    #[allow(dead_code)]
    West,
}

// Tests for basic allocation and shapes
#[test]
fn test_allocate_and_check_shape() {
    facet_testhelpers::setup();

    let poke = PokeValueUninit::alloc::<i32>();
    assert_eq!(poke.shape(), i32::SHAPE);

    let poke = PokeValueUninit::alloc_shape(Point::SHAPE);
    assert_eq!(poke.shape(), Point::SHAPE);
}

// Test initializing with default value
#[test]
fn test_default_initialization() {
    facet_testhelpers::setup();

    // Test with a primitive that has a default
    let poke = PokeValueUninit::alloc::<i32>();
    let poke = poke
        .default_in_place()
        .expect("i32 should have default impl");
    assert_eq!(*poke.get::<i32>(), 0);

    // Test with a struct that has a default
    let poke = PokeValueUninit::alloc::<Point>();
    let poke = poke
        .default_in_place()
        .expect("Point should have default impl");
    assert_eq!(*poke.get::<Point>(), Point { x: 0, y: 0 });
}

// Test direct value initialization
#[test]
fn test_put_value() {
    facet_testhelpers::setup();

    // For a simple type
    let poke = PokeValueUninit::alloc::<i32>();
    let poke = poke.put(42i32).expect("Should accept correct type");
    assert_eq!(*poke.get::<i32>(), 42);

    // For a compound type
    let poke = PokeValueUninit::alloc::<Point>();
    let point = Point { x: 10, y: 20 };
    let poke = poke.put(point).expect("Should accept correct type");
    assert_eq!(*poke.get::<Point>(), Point { x: 10, y: 20 });
}

// Test type mismatch in put
#[test]
fn test_put_wrong_type() {
    facet_testhelpers::setup();

    let poke = PokeValueUninit::alloc::<i32>();
    let result = poke.put(42i64); // Wrong type

    match result {
        Err(ReflectError::WrongShape { expected, actual }) => {
            assert_eq!(expected, i32::SHAPE);
            assert_eq!(actual, i64::SHAPE);
        }
        _ => panic!("Expected WrongShape error"),
    }
}

// Test parsing from string
#[test]
fn test_parse_from_string() {
    facet_testhelpers::setup();

    // Parse a number
    let poke = PokeValueUninit::alloc::<i32>();
    let poke = poke
        .parse("42")
        .expect("i32 should be parseable from string");
    assert_eq!(*poke.get::<i32>(), 42);

    // Try parsing an invalid string
    let poke = PokeValueUninit::alloc::<i32>();
    let result = poke.parse("not a number");
    assert!(result.is_err(), "Parsing invalid string should fail");
}

// Test scalar type identification
#[test]
fn test_scalar_type_identification() {
    facet_testhelpers::setup();

    // For a primitive type
    let poke = PokeValueUninit::alloc::<i32>();
    assert!(poke.scalar_type().is_some());

    // For a non-scalar type
    let poke = PokeValueUninit::alloc::<Point>();
    assert!(poke.scalar_type().is_none());
}

// We can't directly test try_from because we can't access the private OpaqueConst
// directly. Instead, we'll test direct value initialization.
#[test]
fn test_direct_initialization() {
    facet_testhelpers::setup();

    // For a primitive
    let poke = PokeValueUninit::alloc::<u16>();
    let poke = poke.put(42u16).expect("Should accept u16");
    assert_eq!(*poke.get::<u16>(), 42u16);

    // For a different primitive
    let poke = PokeValueUninit::alloc::<f64>();
    let poke = poke.put(3.2f64).expect("Should accept f64");
    assert_eq!(*poke.get::<f64>(), 3.2f64);
}
