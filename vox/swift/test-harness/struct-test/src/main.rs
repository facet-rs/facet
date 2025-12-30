//! Struct serialization test harness for Swift ↔ Rust validation.
//!
//! Commands:
//!   encode - Print test structs as hex
//!   decode - Read hex from stdin and decode
//!   validate - Read bytes from stdin and validate against expected

use facet::Facet;
use facet_postcard::{from_slice, to_vec};
use std::io::{self, Read, Write};

/// Simple struct with basic types
#[derive(Debug, Clone, PartialEq, Facet)]
struct Point {
    x: i32,
    y: i32,
}

/// Struct with various field types
#[derive(Debug, Clone, PartialEq, Facet)]
struct Person {
    name: String,
    age: u32,
    score: f64,
    active: bool,
}

/// Struct with optional and vector fields
#[derive(Debug, Clone, PartialEq, Facet)]
struct ComplexStruct {
    id: u64,
    tags: Vec<String>,
    metadata: Option<String>,
}

/// Nested struct
#[derive(Debug, Clone, PartialEq, Facet)]
struct Nested {
    point: Point,
    label: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("encode");

    match cmd {
        "encode" => encode_test_structs(),
        "decode" => decode_from_stdin(),
        "validate" => validate_from_stdin(),
        "interactive" => interactive_mode(),
        _ => {
            eprintln!("Usage: struct-test [encode|decode|validate|interactive]");
            std::process::exit(1);
        }
    }
}

fn encode_test_structs() {
    println!("=== Struct Serialization Test Vectors ===\n");

    // Test 1: Simple Point
    let point = Point { x: 10, y: -5 };
    print_test_case("Point { x: 10, y: -5 }", &point);

    // Test 2: Point with larger values
    let point2 = Point { x: 1000, y: -1000 };
    print_test_case("Point { x: 1000, y: -1000 }", &point2);

    // Test 3: Person struct
    let person = Person {
        name: "Alice".to_string(),
        age: 30,
        score: 95.5,
        active: true,
    };
    print_test_case(
        "Person { name: \"Alice\", age: 30, score: 95.5, active: true }",
        &person,
    );

    // Test 4: ComplexStruct with Some
    let complex = ComplexStruct {
        id: 12345,
        tags: vec!["rust".to_string(), "swift".to_string()],
        metadata: Some("test data".to_string()),
    };
    print_test_case("ComplexStruct with Some metadata", &complex);

    // Test 5: ComplexStruct with None
    let complex_none = ComplexStruct {
        id: 999,
        tags: vec![],
        metadata: None,
    };
    print_test_case("ComplexStruct with None metadata", &complex_none);

    // Test 6: Nested struct
    let nested = Nested {
        point: Point { x: 42, y: -42 },
        label: "origin".to_string(),
    };
    print_test_case(
        "Nested { point: Point { x: 42, y: -42 }, label: \"origin\" }",
        &nested,
    );

    // Print raw bytes for easy Swift comparison
    println!("\n=== Raw Test Vectors (for Swift) ===\n");

    println!("// Point {{ x: 10, y: -5 }}");
    println!("let pointBytes: [UInt8] = {:?}", to_vec(&point).unwrap());

    println!("\n// Person {{ name: \"Alice\", age: 30, score: 95.5, active: true }}");
    println!("let personBytes: [UInt8] = {:?}", to_vec(&person).unwrap());

    println!("\n// ComplexStruct with tags and Some metadata");
    println!(
        "let complexBytes: [UInt8] = {:?}",
        to_vec(&complex).unwrap()
    );

    println!("\n// Nested struct");
    println!("let nestedBytes: [UInt8] = {:?}", to_vec(&nested).unwrap());
}

fn print_test_case<T: Facet<'static> + std::fmt::Debug>(desc: &str, value: &T) {
    let bytes = to_vec(value).expect("serialization failed");
    println!("{}:", desc);
    println!("  Value: {:?}", value);
    println!("  Bytes: {}", hex(&bytes));
    println!("  Length: {} bytes", bytes.len());
    println!();
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_from_stdin() {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .expect("failed to read stdin");

    // Parse hex string
    let bytes: Vec<u8> = input
        .trim()
        .split_whitespace()
        .filter_map(|s| u8::from_str_radix(s, 16).ok())
        .collect();

    println!("Read {} bytes: {}", bytes.len(), hex(&bytes));

    // Try to decode as Point
    if let Ok(point) = from_slice::<Point>(&bytes) {
        println!("Decoded as Point: {:?}", point);
    }

    // Try to decode as Person
    if let Ok(person) = from_slice::<Person>(&bytes) {
        println!("Decoded as Person: {:?}", person);
    }
}

fn validate_from_stdin() {
    let mut bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut bytes)
        .expect("failed to read stdin");

    eprintln!("Read {} bytes", bytes.len());

    // Expected test cases (same as encode)
    let point = Point { x: 10, y: -5 };
    let expected_point = to_vec(&point).unwrap();

    if bytes == expected_point {
        println!("✓ Matches Point {{ x: 10, y: -5 }}");
        std::process::exit(0);
    }

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
        score: 95.5,
        active: true,
    };
    let expected_person = to_vec(&person).unwrap();

    if bytes == expected_person {
        println!("✓ Matches Person");
        std::process::exit(0);
    }

    // Try to decode and show what we got
    eprintln!("Received: {}", hex(&bytes));
    eprintln!("Expected Point: {}", hex(&expected_point));
    eprintln!("Expected Person: {}", hex(&expected_person));

    if let Ok(point) = from_slice::<Point>(&bytes) {
        println!("Decoded as Point: {:?}", point);
    } else {
        eprintln!("Failed to decode as Point");
    }

    std::process::exit(1);
}

fn interactive_mode() {
    eprintln!("Interactive mode: enter type and hex bytes");
    eprintln!("Format: <type> <hex bytes>");
    eprintln!("Types: point, person, complex, nested");
    eprintln!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        eprint!("> ");
        stdout.flush().unwrap();

        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap() == 0 {
            break;
        }

        let parts: Vec<&str> = line.trim().splitn(2, ' ').collect();
        if parts.len() < 2 {
            eprintln!("Usage: <type> <hex bytes>");
            continue;
        }

        let type_name = parts[0];
        let hex_str = parts[1];

        let bytes: Vec<u8> = hex_str
            .split_whitespace()
            .filter_map(|s| u8::from_str_radix(s, 16).ok())
            .collect();

        match type_name {
            "point" => match from_slice::<Point>(&bytes) {
                Ok(v) => println!("OK: {:?}", v),
                Err(e) => eprintln!("Error: {:?}", e),
            },
            "person" => match from_slice::<Person>(&bytes) {
                Ok(v) => println!("OK: {:?}", v),
                Err(e) => eprintln!("Error: {:?}", e),
            },
            "complex" => match from_slice::<ComplexStruct>(&bytes) {
                Ok(v) => println!("OK: {:?}", v),
                Err(e) => eprintln!("Error: {:?}", e),
            },
            "nested" => match from_slice::<Nested>(&bytes) {
                Ok(v) => println!("OK: {:?}", v),
                Err(e) => eprintln!("Error: {:?}", e),
            },
            _ => eprintln!("Unknown type: {}", type_name),
        }
    }
}
