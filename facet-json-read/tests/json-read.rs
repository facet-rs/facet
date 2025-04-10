use facet_derive::Facet;
use facet_json_read::from_str;

use facet_core as facet;

#[ctor::ctor]
fn init() {
    // Initialize color backtrace for pretty stack traces
    color_backtrace::install();

    // Initialize logger to print all log levels
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
        .format_timestamp(None)
        .init();

    log::info!("Logging and color backtrace initialized");
}

#[test]
fn json_read_simple_struct() {
    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u64,
    }
    let json = r#"{"name": "Alice", "age": 30}"#;

    let s: TestStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
}

#[test]
fn json_read_empty_struct() {
    #[derive(Facet)]
    struct TestStruct {}
    let json = r#"{}"#;

    let _: TestStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
}

#[test]
fn json_read_vec() {
    let json = r#"[1, 2, 3, 4, 5]"#;

    let v: Vec<i32> = match from_str(json) {
        Ok(v) => v,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(v, vec![1, 2, 3, 4, 5]);
}

#[test]
fn json_read_empty_vec() {
    let json = r#"[]"#;

    let v: Vec<i32> = match from_str(json) {
        Ok(v) => v,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(v, vec![]);
}

#[test]
fn json_read_hashmap() {
    let json = r#"{"key1": "value1", "key2": "value2", "key3": "value3"}"#;

    let m: std::collections::HashMap<String, String> = match from_str(json) {
        Ok(m) => m,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(m.get("key1").unwrap(), "value1");
    assert_eq!(m.get("key2").unwrap(), "value2");
    assert_eq!(m.get("key3").unwrap(), "value3");
}

#[test]
fn json_read_more_types() {
    #[derive(Facet)]
    struct TestStructWithMoreTypes {
        u8_val: u8,
        u16_val: u16,
        i8_val: i8,
        i16_val: i16,
        u32_val: u32,
        i32_val: i32,
        u64_val: u64,
        i64_val: i64,
        f32_val: f32,
        f64_val: f64,
    }

    let json = r#"{
        "u8_val": 255,
        "u16_val": 65535,
        "i8_val": -128,
        "i16_val": -32768,
        "u32_val": 4294967295,
        "i32_val": -2147483648,
        "u64_val": 18446744073709551615,
        "i64_val": -9223372036854775808,
        "f32_val": 3.141592653589793,
        "f64_val": 3.141592653589793
    }"#;

    let test_struct: TestStructWithMoreTypes = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(test_struct.u8_val, 255);
    assert_eq!(test_struct.u16_val, 65535);
    assert_eq!(test_struct.i8_val, -128);
    assert_eq!(test_struct.i16_val, -32768);
    assert_eq!(test_struct.u32_val, 4294967295);
    assert_eq!(test_struct.i32_val, -2147483648);
    assert_eq!(test_struct.u64_val, 18446744073709551615);
    assert_eq!(test_struct.i64_val, -9223372036854775808);
    assert!((test_struct.f32_val - std::f32::consts::PI).abs() < f32::EPSILON);
    assert!((test_struct.f64_val - std::f64::consts::PI).abs() < f64::EPSILON);
}

#[test]
fn test_from_json_with_nested_structs() {
    #[derive(Facet)]
    struct InnerStruct {
        value: i32,
    }

    #[derive(Facet)]
    struct OuterStruct {
        name: String,
        inner: InnerStruct,
    }

    let json = r#"{
        "name": "Outer",
        "inner": {
            "value": 42
        }
    }"#;

    let test_struct: OuterStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(test_struct.name, "Outer");
    assert_eq!(test_struct.inner.value, 42);
}

// #[test]
// fn test_from_json_with_simple_tuples() {
//     type Tuple = (i32, String);

//     let json = r#"[123, "Hello"]"#;

//     let test_struct: Tuple = match from_str(json) {
//         Ok(s) => s,
//         Err(e) => panic!("Error deserializing JSON: {}", e),
//     };

//     assert_eq!(test_struct.0, 123);
//     assert_eq!(test_struct.1, "Hello");
// }

// #[test]
// fn test_from_json_with_tuples() {
//     type Tuple = (i32, String, (f64, bool));

//     let json = r#"[123, "Hello", [3.69, true]]"#;

//     let test_struct: Tuple = match from_str(json) {
//         Ok(s) => s,
//         Err(e) => panic!("Error deserializing JSON: {}", e),
//     };

//     assert_eq!(test_struct.0, 123);
//     assert_eq!(test_struct.1, "Hello");
//     assert!((test_struct.2.0 - 3.69).abs() < f64::EPSILON);
//     assert!(test_struct.2.1);
// }

// #[test]
// fn test_from_json_with_tuples() {
//     #[derive(Facet)]
//     struct TupleStruct(i32, String, (f64, bool));

//     let json = r#"[123, "Hello", [3.69, true]]"#;

//     let test_struct: TupleStruct = match from_str(json) {
//         Ok(s) => s,
//         Err(e) => panic!("Error deserializing JSON: {}", e),
//     };

//     assert_eq!(test_struct.0, 123);
//     assert_eq!(test_struct.1, "Hello");
//     assert!((test_struct.2.0 - 3.69).abs() < f64::EPSILON);
//     assert!(test_struct.2.1);
// }

// #[test]
// fn test_from_json_with_vec() {
//     #[derive(Facet, Debug, PartialEq)]
//     struct VecStruct {
//         numbers: Vec<i32>,
//         names: Vec<String>,
//     }

//     let json = r#"{
//         "numbers": [1, 2, 3, 4, 5],
//         "names": ["Alice", "Bob", "Charlie"]
//     }"#;

//     // Deserialize
//     let mut test_struct = VecStruct::partial();
//     from_json(&mut test_struct, json).unwrap();
//     let built_struct = test_struct.build::<VecStruct>();

//     // Verify deserialization
//     assert_eq!(built_struct.numbers, vec![1, 2, 3, 4, 5]);
//     assert_eq!(built_struct.names, vec!["Alice", "Bob", "Charlie"]);

//     // Serialize
//     let mut buffer = Vec::new();
//     to_json(
//         &built_struct as *const _ as *mut u8,
//         VecStruct::SHAPE_FN,
//         &mut buffer,
//         true,
//     )
//     .unwrap();
//     let serialized_json = String::from_utf8(buffer).unwrap();

//     // Print the serialized JSON
//     eprintln!("Serialized JSON:\n{}", serialized_json);

//     // Round-trip: deserialize the serialized JSON
//     let mut round_trip_struct = VecStruct::partial();
//     from_json(&mut round_trip_struct, &serialized_json).unwrap();
//     let round_trip_built = round_trip_struct.build::<VecStruct>();

//     // Verify round-trip
//     assert_eq!(round_trip_built, built_struct);
// }

// #[test]
// fn json_read_with_hashmap() {
//     #[derive(Facet, Debug, PartialEq)]
//     struct OtherStruct {
//         value: i32,
//         name: String,
//     }

//     #[derive(Facet, Debug, PartialEq)]
//     struct HashmapStruct {
//         data: std::collections::HashMap<String, OtherStruct>,
//     }

//     let json = r#"{
//         "data": {
//             "first": {
//                 "value": 42,
//                 "name": "First Item"
//             },
//             "second": {
//                 "value": 84,
//                 "name": "Second Item"
//             },
//             "third": {
//                 "value": 126,
//                 "name": "Third Item"
//             }
//         }
//     }"#;

//     // Deserialize
//     let mut test_struct = HashmapStruct::partial();
//     from_json(&mut test_struct, json).unwrap();
//     let built_struct = test_struct.build::<HashmapStruct>();

//     // Verify deserialization
//     assert_eq!(built_struct.data.len(), 3);
//     assert_eq!(built_struct.data.get("first").unwrap().value, 42);
//     assert_eq!(built_struct.data.get("first").unwrap().name, "First Item");
//     assert_eq!(built_struct.data.get("second").unwrap().value, 84);
//     assert_eq!(built_struct.data.get("second").unwrap().name, "Second Item");
//     assert_eq!(built_struct.data.get("third").unwrap().value, 126);
//     assert_eq!(built_struct.data.get("third").unwrap().name, "Third Item");

//     // Serialize
//     let mut buffer = Vec::new();
//     to_json(
//         &built_struct as *const _ as *mut u8,
//         HashmapStruct::SHAPE_FN,
//         &mut buffer,
//         true,
//     )
//     .unwrap();
//     let serialized_json = String::from_utf8(buffer).unwrap();

//     // Print the serialized JSON
//     eprintln!("Serialized JSON:\n{}", serialized_json);

//     // Round-trip: deserialize the serialized JSON
//     let mut round_trip_struct = HashmapStruct::partial();
//     from_json(&mut round_trip_struct, &serialized_json).unwrap();
//     let round_trip_built = round_trip_struct.build::<HashmapStruct>();

//     // Verify round-trip
//     assert_eq!(round_trip_built, built_struct);
// }
