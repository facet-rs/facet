use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;

#[derive(Debug, Facet)]
struct ElectricityData {
    time: String,

    kwh_import_total_tarif_low: f32,
    kwh_import_total_tarif_high: f32,
    kwh_export_total_tarif_low: f32,
    kwh_export_total_tarif_high: f32,

    voltages: [f32; 3],
    active_powers_import: [f32; 3],
    active_powers_export: [f32; 3],
}

#[test]
fn test_array_field_parsing() {
    // Simplified test data based on the issue
    let json = r#"{
        "time": "2024-08-27 11:55:42.428526+00:00",
        "kwh_import_total_tarif_low": 1709.78,
        "kwh_import_total_tarif_high": 2033.879,
        "kwh_export_total_tarif_low": 540.95,
        "kwh_export_total_tarif_high": 1201.854,
        "voltages": [237.1, 232.2, 231.4],
        "active_powers_import": [0, 0.121, 0.215],
        "active_powers_export": [2.94, 0, 0]
    }"#;

    let result = from_str::<ElectricityData>(json);
    match result {
        Ok(data) => {
            // Test succeeded
            assert_eq!(data.voltages.len(), 3);
            assert_eq!(data.active_powers_import.len(), 3);
            assert_eq!(data.active_powers_export.len(), 3);
        }
        Err(e) => {
            // This will fail and show the error
            panic!("Failed to parse JSON: {}", e);
        }
    }
}

#[derive(Debug, Facet, PartialEq)]
struct NestedArrays {
    name: String,
    matrix: [[i32; 2]; 3], // 3x2 matrix
}

#[test]
fn test_nested_array_parsing() {
    let json = r#"{
        "name": "test matrix",
        "matrix": [
            [1, 2],
            [3, 4],
            [5, 6]
        ]
    }"#;

    let result = from_str::<NestedArrays>(json);
    match result {
        Ok(data) => {
            assert_eq!(
                data,
                NestedArrays {
                    name: "test matrix".to_string(),
                    matrix: [[1, 2], [3, 4], [5, 6]]
                }
            );
        }
        Err(e) => {
            panic!("Failed to parse nested arrays: {}", e);
        }
    }
}

#[derive(Debug, Facet, PartialEq)]
struct MixedArrayTypes {
    strings: [String; 2],
    numbers: [i32; 3],
    booleans: [bool; 2],
}

#[test]
fn test_mixed_array_types() {
    let json = r#"{
        "strings": ["hello", "world"],
        "numbers": [42, 123, 999],
        "booleans": [true, false]
    }"#;

    let result = from_str::<MixedArrayTypes>(json);
    match result {
        Ok(data) => {
            assert_eq!(
                data,
                MixedArrayTypes {
                    strings: ["hello".to_string(), "world".to_string()],
                    numbers: [42, 123, 999],
                    booleans: [true, false],
                }
            );
        }
        Err(e) => {
            panic!("Failed to parse mixed array types: {}", e);
        }
    }
}
