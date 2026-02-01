use facet::Facet;
use trame::{Op, Partial};

// =============================================================================
// Option with Imm (moving complete values)
// =============================================================================

#[test]
fn build_option_some_with_imm() {
    let mut partial = Partial::alloc::<Option<u32>>().unwrap();

    let mut val = Some(42u32);
    partial.apply(&[Op::set().imm(&mut val)]).unwrap();

    let result: Option<u32> = partial.build().unwrap();
    assert_eq!(result, Some(42));
}

#[test]
fn build_option_none_with_imm() {
    let mut partial = Partial::alloc::<Option<u32>>().unwrap();

    let mut val: Option<u32> = None;
    partial.apply(&[Op::set().imm(&mut val)]).unwrap();

    let result: Option<u32> = partial.build().unwrap();
    assert_eq!(result, None);
}

#[test]
fn build_option_none_with_default() {
    let mut partial = Partial::alloc::<Option<u32>>().unwrap();

    // Option's default is None
    partial.apply(&[Op::set().default()]).unwrap();

    let result: Option<u32> = partial.build().unwrap();
    assert_eq!(result, None);
}

// =============================================================================
// Option with Build (building Some with complex inner)
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn build_option_some_struct_with_build() {
    let mut partial = Partial::alloc::<Option<Point>>().unwrap();

    // Enter the Option (selecting Some variant), then set fields of Point
    let mut x = 10i32;
    let mut y = 20i32;

    partial
        .apply(&[
            Op::set().at(1).stage(),     // variant 1 = Some, push frame for Point
            Op::set().at(0).imm(&mut x), // Point.x
            Op::set().at(1).imm(&mut y), // Point.y
            Op::end(),
        ])
        .unwrap();

    let result: Option<Point> = partial.build().unwrap();
    assert_eq!(result, Some(Point { x: 10, y: 20 }));
}

#[test]
fn build_option_some_string_with_build() {
    let mut partial = Partial::alloc::<Option<String>>().unwrap();

    let mut s = String::from("hello");

    partial
        .apply(&[
            Op::set().at(1).stage(), // variant 1 = Some
            Op::set().imm(&mut s),   // the String itself
            Op::end(),
        ])
        .unwrap();

    // Imm moves the value - prevent double-free
    std::mem::forget(s);

    let result: Option<String> = partial.build().unwrap();
    assert_eq!(result, Some(String::from("hello")));
}

#[derive(Debug, PartialEq, Facet)]
struct Server {
    host: String,
    port: u16,
}

#[test]
fn build_option_some_server_with_build() {
    let mut partial = Partial::alloc::<Option<Server>>().unwrap();

    let mut host = String::from("localhost");
    let mut port = 8080u16;

    partial
        .apply(&[
            Op::set().at(1).stage(),        // Some variant
            Op::set().at(0).imm(&mut host), // Server.host
            Op::set().at(1).imm(&mut port), // Server.port
            Op::end(),
        ])
        .unwrap();

    // Imm moves the value - prevent double-free
    std::mem::forget(host);

    let result: Option<Server> = partial.build().unwrap();
    assert_eq!(
        result,
        Some(Server {
            host: String::from("localhost"),
            port: 8080
        })
    );
}

// =============================================================================
// Option as struct field with Build
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Config {
    name: String,
    server: Option<Server>,
}

#[test]
fn build_struct_with_option_field_some_via_build() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let mut name = String::from("my-config");
    let mut host = String::from("example.com");
    let mut port = 443u16;

    partial
        .apply(&[
            Op::set().at(0).imm(&mut name),
            Op::set().at(1).stage(),        // enter Option<Server> field
            Op::set().at(1).stage(),        // Some variant, enter Server
            Op::set().at(0).imm(&mut host), // Server.host
            Op::set().at(1).imm(&mut port), // Server.port
            Op::end(),                      // end Server
            Op::end(),                      // end Option
        ])
        .unwrap();

    // Imm moves the values - prevent double-free
    std::mem::forget(name);
    std::mem::forget(host);

    let result: Config = partial.build().unwrap();
    assert_eq!(
        result,
        Config {
            name: String::from("my-config"),
            server: Some(Server {
                host: String::from("example.com"),
                port: 443
            })
        }
    );
}

#[test]
fn build_struct_with_option_field_none_via_default() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let mut name = String::from("minimal-config");

    partial
        .apply(&[
            Op::set().at(0).imm(&mut name),
            Op::set().at(1).default(), // Option's default is None
        ])
        .unwrap();

    // Imm moves the value - prevent double-free
    std::mem::forget(name);

    let result: Config = partial.build().unwrap();
    assert_eq!(
        result,
        Config {
            name: String::from("minimal-config"),
            server: None
        }
    );
}

// =============================================================================
// Nested Options
// =============================================================================

#[test]
fn build_nested_option_some_some() {
    let mut partial = Partial::alloc::<Option<Option<u32>>>().unwrap();

    let mut val = 42u32;

    partial
        .apply(&[
            Op::set().at(1).stage(), // outer Some
            Op::set().at(1).stage(), // inner Some
            Op::set().imm(&mut val), // the u32
            Op::end(),               // end inner Option
            Op::end(),               // end outer Option
        ])
        .unwrap();

    let result: Option<Option<u32>> = partial.build().unwrap();
    assert_eq!(result, Some(Some(42)));
}

#[test]
fn build_nested_option_some_none() {
    let mut partial = Partial::alloc::<Option<Option<u32>>>().unwrap();

    partial
        .apply(&[
            Op::set().at(1).stage(), // outer Some
            Op::set().default(),     // inner None (Option's default)
            Op::end(),               // end outer Option
        ])
        .unwrap();

    let result: Option<Option<u32>> = partial.build().unwrap();
    assert_eq!(result, Some(None));
}

// =============================================================================
// Option in collections
// =============================================================================

#[test]
fn build_vec_of_option_structs_with_build() {
    let mut partial = Partial::alloc::<Vec<Option<Point>>>().unwrap();

    let mut x1 = 1i32;
    let mut y1 = 2i32;
    let mut x2 = 3i32;
    let mut y2 = 4i32;

    partial
        .apply(&[
            Op::set().stage(), // initialize Vec
            // First element: Some(Point)
            Op::set().append().stage(), // push and enter Option
            Op::set().at(1).stage(),    // Some variant, enter Point
            Op::set().at(0).imm(&mut x1),
            Op::set().at(1).imm(&mut y1),
            Op::end(), // end Point
            Op::end(), // end Option
            // Second element: None
            Op::set().append().default(), // None
            // Third element: Some(Point)
            Op::set().append().stage(), // push and enter Option
            Op::set().at(1).stage(),    // Some variant, enter Point
            Op::set().at(0).imm(&mut x2),
            Op::set().at(1).imm(&mut y2),
            Op::end(), // end Point
            Op::end(), // end Option
        ])
        .unwrap();

    let result: Vec<Option<Point>> = partial.build().unwrap();
    assert_eq!(
        result,
        vec![Some(Point { x: 1, y: 2 }), None, Some(Point { x: 3, y: 4 })]
    );
}
