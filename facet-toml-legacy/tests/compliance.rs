//! Compliance tests using toml-test-data fixtures
//!
//! This runs the official TOML test suite by:
//! 1. Parsing the TOML fixture into facet_value::Value
//! 2. Parsing the expected JSON (tagged format) and converting to plain Value
//! 3. Comparing with assert_same!

use facet_value::{VDateTime, Value};

/// Parse a datetime string into a VDateTime Value.
fn parse_datetime_value(s: &str) -> Value {
    // Check if this is a local time (starts with HH:MM, no date)
    if s.len() >= 5 && s.as_bytes()[2] == b':' && !s.contains('-') {
        // Local time: HH:MM:SS[.fractional]
        let (hour, minute, second, nanos) = parse_time_part(s);
        return VDateTime::new_local_time(hour, minute, second, nanos).into();
    }

    // Must start with a date (YYYY-MM-DD)
    let year: i32 = s[0..4].parse().unwrap();
    let month: u8 = s[5..7].parse().unwrap();
    let day: u8 = s[8..10].parse().unwrap();

    // If that's all, it's a local date
    if s.len() == 10 {
        return VDateTime::new_local_date(year, month, day).into();
    }

    // Has time component
    let time_part = &s[11..];
    let (hour, minute, second, nanos, offset) = parse_time_with_offset(time_part);

    match offset {
        Some(offset_minutes) => VDateTime::new_offset(
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanos,
            offset_minutes,
        )
        .into(),
        None => VDateTime::new_local_datetime(year, month, day, hour, minute, second, nanos).into(),
    }
}

fn parse_time_part(s: &str) -> (u8, u8, u8, u32) {
    let hour: u8 = s[0..2].parse().unwrap();
    let minute: u8 = s[3..5].parse().unwrap();
    let second: u8 = s[6..8].parse().unwrap();

    let nanos = if s.len() > 8 && s.as_bytes()[8] == b'.' {
        parse_nanos(&s[9..])
    } else {
        0
    };

    (hour, minute, second, nanos)
}

fn parse_time_with_offset(s: &str) -> (u8, u8, u8, u32, Option<i16>) {
    let hour: u8 = s[0..2].parse().unwrap();
    let minute: u8 = s[3..5].parse().unwrap();
    let second: u8 = s[6..8].parse().unwrap();

    let rest = &s[8..];
    let (nanos, offset_rest) = if let Some(stripped) = rest.strip_prefix('.') {
        let frac_end = stripped
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(stripped.len());
        let nanos = parse_nanos(&stripped[..frac_end]);
        (nanos, &stripped[frac_end..])
    } else {
        (0, rest)
    };

    let offset = if offset_rest.is_empty() {
        None
    } else if offset_rest == "Z" || offset_rest == "z" {
        Some(0i16)
    } else if offset_rest.starts_with('+') || offset_rest.starts_with('-') {
        Some(parse_offset(offset_rest))
    } else {
        None
    };

    (hour, minute, second, nanos, offset)
}

fn parse_nanos(s: &str) -> u32 {
    let digits: String = s.chars().take(9).filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return 0;
    }
    let padded = format!("{digits:0<9}");
    padded.parse().unwrap_or(0)
}

fn parse_offset(s: &str) -> i16 {
    let sign: i16 = if s.starts_with('+') { 1 } else { -1 };
    let rest = &s[1..];

    let (hours, minutes) = if rest.len() >= 5 && rest.as_bytes()[2] == b':' {
        let h: i16 = rest[0..2].parse().unwrap();
        let m: i16 = rest[3..5].parse().unwrap();
        (h, m)
    } else {
        let h: i16 = rest[0..2].parse().unwrap();
        (h, 0)
    };

    sign * (hours * 60 + minutes)
}

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
                "datetime" => {
                    // Offset datetime: 1979-05-27T07:32:00Z or 1979-05-27T07:32:00+05:30
                    parse_datetime_value(value_str)
                }
                "datetime-local" => {
                    // Local datetime: 1979-05-27T07:32:00
                    parse_datetime_value(value_str)
                }
                "date-local" | "date" => {
                    // Local date: 1979-05-27
                    parse_datetime_value(value_str)
                }
                "time-local" | "time" => {
                    // Local time: 07:32:00
                    parse_datetime_value(value_str)
                }
                _ => panic!("Unknown type: {type_}"),
            }
        } else {
            // Regular object - recurse
            Value::from_iter(obj.iter().map(|(k, v)| (k.as_str(), untagged(v))))
        }
    } else {
        panic!("Expected object or array in tagged JSON, got {v:?}");
    }
}

#[test]
#[ignore = "TOML compliance suite has known failures - run with --ignored to check progress"]
fn test_valid_fixtures() {
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for valid in toml_test_data::valid() {
        let name = valid.name().display().to_string();

        let fixture = match std::str::from_utf8(valid.fixture()) {
            Ok(s) => s,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        // Parse TOML into Value
        eprintln!("Testing: {name}");
        let actual: Result<Value, _> = facet_toml_legacy::from_str(fixture);

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
                        println!("\n--- {name} ---");
                        if name.contains("implicit") {
                            println!("TOML:\n{fixture}");
                            println!("Actual: {actual:?}");
                            println!("Expected: {expected:?}");
                        }
                        println!("{diff}");
                    }
                    facet_assert::Sameness::Opaque { type_name } => {
                        failed += 1;
                        println!("\n--- {name} ---");
                        println!("Cannot compare opaque type: {type_name}");
                    }
                }
            }
            (Err(e), _) => {
                failed += 1;
                println!("\n--- {name} ---");
                println!("Failed to parse TOML: {e}");
            }
            (_, Err(e)) => {
                failed += 1;
                println!("\n--- {name} ---");
                println!("Failed to parse expected JSON: {e}");
            }
        }
    }

    println!("\n=== Compliance Test Results ===");
    println!("Passed:  {passed}");
    println!("Failed:  {failed}");
    println!("Skipped: {skipped}");

    if failed > 0 {
        panic!("{failed} tests failed");
    }
}
