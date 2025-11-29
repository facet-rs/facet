//! Compliance tests using toml-test-data fixtures
//!
//! This runs the official TOML test suite by:
//! 1. Parsing the TOML fixture into facet_value::Value
//! 2. Parsing the expected JSON (tagged format) and converting to plain Value
//! 3. Comparing with assert_same!

use facet_assert::assert_same;
use facet_value::Value;

/// Convert a tagged JSON Value (from toml-test format) to a plain Value.
/// Tagged format: {"type": "string", "value": "hello"} → "hello"
fn untagged(v: &Value) -> Value {
    if let Some(arr) = v.as_array() {
        Value::from_iter(arr.iter().map(untagged))
    } else if let Some(obj) = v.as_object() {
        // Check if it's a tagged scalar
        let has_type = obj.get("type").is_some();
        let has_value = obj.get("value").is_some();

        if has_type && has_value && obj.len() == 2 {
            // Tagged scalar - extract the actual value
            let type_ = obj.get("type").unwrap().as_string().unwrap().as_str();
            let value_str = obj.get("value").unwrap().as_string().unwrap().as_str();

            match type_ {
                "string" => Value::from(value_str),
                "integer" => Value::from(value_str.parse::<i64>().unwrap()),
                "float" => {
                    let f = match value_str {
                        "inf" | "+inf" => f64::INFINITY,
                        "-inf" => f64::NEG_INFINITY,
                        "nan" | "+nan" => f64::NAN,
                        "-nan" => f64::NAN, // NaN sign not preserved
                        _ => value_str.parse::<f64>().unwrap(),
                    };
                    Value::from(f)
                }
                "bool" => Value::from(value_str == "true"),
                "datetime" | "datetime-local" | "date-local" | "date" | "time-local" | "time" => {
                    // Skip datetime for now - return as string
                    Value::from(value_str)
                }
                _ => panic!("Unknown type: {}", type_),
            }
        } else {
            // Regular object - recurse
            Value::from_iter(obj.iter().map(|(k, v)| (k.as_str(), untagged(v))))
        }
    } else {
        panic!("Expected object or array in tagged JSON, got {:?}", v);
    }
}

#[test]
fn test_valid_fixtures() {
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for valid in toml_test_data::valid() {
        let name = valid.name().display().to_string();

        // Skip datetime tests for now
        if name.contains("datetime") || name.contains("local-date") || name.contains("local-time") {
            skipped += 1;
            continue;
        }

        let fixture = match std::str::from_utf8(valid.fixture()) {
            Ok(s) => s,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        // Parse TOML into Value
        let actual: Result<Value, _> = facet_toml::from_str(fixture);

        // Parse expected JSON and convert from tagged format
        let expected_str = std::str::from_utf8(valid.expected()).unwrap();
        let expected_tagged: Result<Value, _> = facet_json::from_str(expected_str);

        match (actual, expected_tagged) {
            (Ok(actual), Ok(expected_tagged)) => {
                let expected = untagged(&expected_tagged);

                // Use check_same to get result without panicking
                match facet_assert::check_same(&actual, &expected) {
                    facet_assert::Sameness::Same => {
                        passed += 1;
                    }
                    facet_assert::Sameness::Different(diff) => {
                        failed += 1;
                        println!("\n--- {} ---", name);
                        println!("{}", diff);
                    }
                    facet_assert::Sameness::Opaque { type_name } => {
                        failed += 1;
                        println!("\n--- {} ---", name);
                        println!("Cannot compare opaque type: {}", type_name);
                    }
                }
            }
            (Err(e), _) => {
                failed += 1;
                println!("\n--- {} ---", name);
                println!("Failed to parse TOML: {}", e);
            }
            (_, Err(e)) => {
                failed += 1;
                println!("\n--- {} ---", name);
                println!("Failed to parse expected JSON: {}", e);
            }
        }
    }

    println!("\n=== Compliance Test Results ===");
    println!("Passed:  {}", passed);
    println!("Failed:  {}", failed);
    println!("Skipped: {} (datetime)", skipped);

    if failed > 0 {
        panic!("{} tests failed", failed);
    }
}
