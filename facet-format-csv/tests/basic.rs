//! Basic tests for CSV parsing and serialization.

use facet::Facet;
use facet_format_csv::{from_str, to_string};

#[derive(Facet, Debug, PartialEq)]
struct Person {
    name: String,
    age: u32,
}

#[test]
fn test_simple_struct() {
    let csv = "Alice,30";
    let person: Person = from_str(csv).unwrap();
    assert_eq!(person.name, "Alice");
    assert_eq!(person.age, 30);
}

#[test]
fn test_quoted_field() {
    let csv = "\"Bob, Jr.\",25";
    let person: Person = from_str(csv).unwrap();
    assert_eq!(person.name, "Bob, Jr.");
    assert_eq!(person.age, 25);
}

#[derive(Facet, Debug, PartialEq)]
struct Numbers {
    a: i32,
    b: f64,
    c: bool,
}

#[test]
fn test_multiple_types() {
    let csv = "-42,3.125,true";
    let nums: Numbers = from_str(csv).unwrap();
    assert_eq!(nums.a, -42);
    assert!((nums.b - 3.125).abs() < 0.001);
    assert!(nums.c);
}

#[test]
fn test_false_bool() {
    let csv = "0,0.0,false";
    let nums: Numbers = from_str(csv).unwrap();
    assert_eq!(nums.a, 0);
    assert!((nums.b - 0.0).abs() < 0.001);
    assert!(!nums.c);
}

// Serialization tests

#[test]
fn test_serialize_simple_struct() {
    let person = Person {
        name: "Alice".to_string(),
        age: 30,
    };
    let csv = to_string(&person).unwrap();
    assert_eq!(csv, "Alice,30\n");
}

#[test]
fn test_serialize_quoted_field() {
    let person = Person {
        name: "Bob, Jr.".to_string(),
        age: 25,
    };
    let csv = to_string(&person).unwrap();
    assert_eq!(csv, "\"Bob, Jr.\",25\n");
}

#[test]
fn test_serialize_numbers() {
    let nums = Numbers {
        a: -42,
        b: 3.125,
        c: true,
    };
    let csv = to_string(&nums).unwrap();
    assert_eq!(csv, "-42,3.125,true\n");
}

// Round-trip tests

#[test]
fn test_roundtrip_person() {
    let original = Person {
        name: "Charlie".to_string(),
        age: 35,
    };
    let csv = to_string(&original).unwrap();
    let parsed: Person = from_str(csv.trim()).unwrap();
    assert_eq!(original, parsed);
}
