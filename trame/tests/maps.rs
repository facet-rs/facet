use facet::Facet;
use std::collections::HashMap;
use trame::{Op, Partial};

// =============================================================================
// Basic HashMap tests
// =============================================================================

#[test]
fn build_empty_map() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    // For root-level maps, no End needed - Stage initializes the map
    partial.apply(&[Op::set().stage()]).unwrap();

    let result: HashMap<String, u32> = partial.build().unwrap();
    assert!(result.is_empty());
}

#[test]
fn build_map_with_imm_values() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut key1 = String::from("one");
    let mut key2 = String::from("two");
    let mut val1 = 1u32;
    let mut val2 = 2u32;

    partial
        .apply(&[
            Op::set().stage_with_capacity(2),
            // Entry 1: key="one", value=1
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key1),
            Op::set().at(1).imm(&mut val1),
            Op::End,
            // Entry 2: key="two", value=2
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key2),
            Op::set().at(1).imm(&mut val2),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(key1);
    std::mem::forget(key2);

    let result: HashMap<String, u32> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("one"), Some(&1));
    assert_eq!(result.get("two"), Some(&2));
}

#[test]
fn build_map_with_default_values() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut key1 = String::from("first");
    let mut key2 = String::from("second");

    partial
        .apply(&[
            Op::set().stage(),
            // Entry 1: key="first", value=default
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key1),
            Op::set().at(1).default(),
            Op::End,
            // Entry 2: key="second", value=default
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key2),
            Op::set().at(1).default(),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(key1);
    std::mem::forget(key2);

    let result: HashMap<String, u32> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("first"), Some(&0)); // u32::default() = 0
    assert_eq!(result.get("second"), Some(&0));
}

// =============================================================================
// HashMap with complex values (using Stage for complex values)
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Server {
    host: String,
    port: u16,
}

#[test]
fn build_map_with_struct_values() {
    let mut partial = Partial::alloc::<HashMap<String, Server>>().unwrap();

    let mut key = String::from("primary");
    let mut host = String::from("localhost");
    let mut port = 8080u16;

    partial
        .apply(&[
            Op::set().stage_with_capacity(1),
            // Entry with key and struct value
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key),  // key
            Op::set().at(1).stage(),        // value (struct) - push frame for struct
            Op::set().at(0).imm(&mut host), // Server.host
            Op::set().at(1).imm(&mut port), // Server.port
            Op::End,                        // End Server
            Op::End,                        // End entry
        ])
        .unwrap();
    std::mem::forget(key);
    std::mem::forget(host);

    let result: HashMap<String, Server> = partial.build().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result.get("primary"),
        Some(&Server {
            host: String::from("localhost"),
            port: 8080
        })
    );
}

#[test]
fn build_map_with_multiple_struct_values() {
    let mut partial = Partial::alloc::<HashMap<String, Server>>().unwrap();

    let mut key1 = String::from("primary");
    let mut key2 = String::from("secondary");
    let mut host1 = String::from("host1.example.com");
    let mut host2 = String::from("host2.example.com");
    let mut port1 = 8080u16;
    let mut port2 = 9090u16;

    partial
        .apply(&[
            Op::set().stage_with_capacity(2),
            // First entry
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key1),
            Op::set().at(1).stage(),
            Op::set().at(0).imm(&mut host1),
            Op::set().at(1).imm(&mut port1),
            Op::End, // End Server
            Op::End, // End entry
            // Second entry
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key2),
            Op::set().at(1).stage(),
            Op::set().at(0).imm(&mut host2),
            Op::set().at(1).imm(&mut port2),
            Op::End, // End Server
            Op::End, // End entry
        ])
        .unwrap();
    std::mem::forget(key1);
    std::mem::forget(key2);
    std::mem::forget(host1);
    std::mem::forget(host2);

    let result: HashMap<String, Server> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result.get("primary"),
        Some(&Server {
            host: String::from("host1.example.com"),
            port: 8080
        })
    );
    assert_eq!(
        result.get("secondary"),
        Some(&Server {
            host: String::from("host2.example.com"),
            port: 9090
        })
    );
}

// =============================================================================
// Struct with HashMap field
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Config {
    name: String,
    env: HashMap<String, String>,
}

#[test]
fn build_struct_with_map_field() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let mut name = String::from("my-app");
    let mut env_key = String::from("PATH");
    let mut env_value = String::from("/usr/bin");

    partial
        .apply(&[
            // Set name field
            Op::set().at(0).imm(&mut name),
            // Build env field
            Op::set().at(1).stage_with_capacity(1),
            // Entry
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut env_key),
            Op::set().at(1).imm(&mut env_value),
            Op::End, // End entry
            Op::End, // End map
        ])
        .unwrap();
    std::mem::forget(name);
    std::mem::forget(env_key);
    std::mem::forget(env_value);

    let result: Config = partial.build().unwrap();
    assert_eq!(result.name, "my-app");
    assert_eq!(result.env.len(), 1);
    assert_eq!(result.env.get("PATH"), Some(&String::from("/usr/bin")));
}

#[test]
fn build_struct_with_empty_map_field() {
    let mut partial = Partial::alloc::<Config>().unwrap();

    let mut name = String::from("empty-config");

    partial
        .apply(&[
            Op::set().at(0).imm(&mut name),
            Op::set().at(1).stage(), // empty map
            Op::End,
        ])
        .unwrap();
    std::mem::forget(name);

    let result: Config = partial.build().unwrap();
    assert_eq!(result.name, "empty-config");
    assert!(result.env.is_empty());
}

// =============================================================================
// HashMap with Vec values
// =============================================================================

#[test]
fn build_map_with_vec_values() {
    let mut partial = Partial::alloc::<HashMap<String, Vec<u32>>>().unwrap();

    let mut key = String::from("numbers");
    let mut a = 1u32;
    let mut b = 2u32;
    let mut c = 3u32;

    partial
        .apply(&[
            Op::set().stage(),
            // Entry
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key), // key
            Op::set().at(1).stage(),       // value (Vec)
            Op::set().append().imm(&mut a),
            Op::set().append().imm(&mut b),
            Op::set().append().imm(&mut c),
            Op::End, // End Vec
            Op::End, // End entry
        ])
        .unwrap();
    std::mem::forget(key);

    let result: HashMap<String, Vec<u32>> = partial.build().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("numbers"), Some(&vec![1, 2, 3]));
}

// =============================================================================
// Integer keys
// =============================================================================

#[test]
fn build_map_with_integer_keys() {
    let mut partial = Partial::alloc::<HashMap<u32, String>>().unwrap();

    let mut key1 = 1u32;
    let mut key2 = 2u32;
    let mut val1 = String::from("one");
    let mut val2 = String::from("two");

    partial
        .apply(&[
            Op::set().stage(),
            // Entry 1
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key1),
            Op::set().at(1).imm(&mut val1),
            Op::End,
            // Entry 2
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key2),
            Op::set().at(1).imm(&mut val2),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(val1);
    std::mem::forget(val2);

    let result: HashMap<u32, String> = partial.build().unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(&1), Some(&String::from("one")));
    assert_eq!(result.get(&2), Some(&String::from("two")));
}

// =============================================================================
// Setting entire map entry as a tuple
// =============================================================================

#[test]
fn build_map_with_tuple_entry() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    // Create an entry as a (String, u32) tuple and set it directly
    let mut entry: (String, u32) = (String::from("answer"), 42);

    partial
        .apply(&[
            Op::set().stage(),
            // Set entire entry at once using Imm with a tuple
            Op::set().append().imm(&mut entry),
        ])
        .unwrap();
    std::mem::forget(entry);

    let result: HashMap<String, u32> = partial.build().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.get("answer"), Some(&42));
}

// =============================================================================
// Error cases
// =============================================================================

#[test]
fn append_on_non_collection_errors() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let err = partial.apply(&[Op::set().append().stage()]).unwrap_err();
    // Should error because u32 is not a collection
    assert!(matches!(err.kind, trame::ReflectErrorKind::NotAList));
}

#[test]
fn map_entry_wrong_key_type_errors() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut wrong_key = 42u32; // Should be String
    let _val = 1u32; // unused after API change

    partial.apply(&[Op::set().stage()]).unwrap();
    partial.apply(&[Op::set().append().stage()]).unwrap();
    let err = partial
        .apply(&[Op::set().at(0).imm(&mut wrong_key)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        trame::ReflectErrorKind::ShapeMismatch { .. }
    ));
}

#[test]
fn map_entry_wrong_value_type_errors() {
    let mut partial = Partial::alloc::<HashMap<String, u32>>().unwrap();

    let mut key = String::from("key");
    let mut wrong_val = String::from("not a u32");

    partial.apply(&[Op::set().stage()]).unwrap();
    partial.apply(&[Op::set().append().stage()]).unwrap();
    partial.apply(&[Op::set().at(0).imm(&mut key)]).unwrap();
    std::mem::forget(key);

    let err = partial
        .apply(&[Op::set().at(1).imm(&mut wrong_val)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        trame::ReflectErrorKind::ShapeMismatch { .. }
    ));
}

// =============================================================================
// Drop/cleanup tests
// =============================================================================

#[test]
fn drop_partial_map_mid_construction() {
    // Start building a HashMap, insert some entries, then drop without finishing
    let mut partial = Partial::alloc::<HashMap<String, String>>().unwrap();

    let mut key = String::from("key");
    let mut val = String::from("value");

    partial
        .apply(&[
            Op::set().stage(),
            Op::set().append().stage(),
            Op::set().at(0).imm(&mut key),
            Op::set().at(1).imm(&mut val),
            Op::End,
        ])
        .unwrap();
    std::mem::forget(key);
    std::mem::forget(val);

    // Drop the partial - should clean up the map and its entries
    drop(partial);
    // If we get here without Miri complaining, we're good
}

#[test]
fn drop_partial_map_mid_value_build() {
    // Start building a HashMap, start building a value, then drop
    let mut partial = Partial::alloc::<HashMap<String, Server>>().unwrap();

    let mut key = String::from("server");
    let mut host = String::from("localhost");

    let result = partial.apply(&[
        Op::set().stage(),
        Op::set().append().stage(),
        Op::set().at(0).imm(&mut key),
        Op::set().at(1).stage(),
        Op::set().at(0).imm(&mut host),
        // Don't set port, don't End - just drop
    ]);
    std::mem::forget(key);
    std::mem::forget(host);

    // This might error (incomplete value) or succeed depending on implementation
    // Either way, dropping should be safe
    let _ = result;
    drop(partial);
}
