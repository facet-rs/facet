//! Tests for JSON string escaping, particularly ASCII control characters.
//!
//! This file demonstrates comprehensive testing of the ASCII control character
//! escaping code in `facet-json/src/lib.rs` around line 129-142.
//!
//! The tests cover:
//! - All ASCII control characters (0x00-0x1F, 0x7F) are properly escaped as \u0000 format
//! - Special escape sequences (\n, \r, \t, \b, \f, \", \\) are not affected
//! - Mixed strings with control characters work correctly
//! - Roundtrip serialization/deserialization preserves control characters
//! - Hex digit generation is correct for edge cases
//!
//! We also found and fixed a critical bug in the original hex escaping code:
//! The original code incorrectly used raw bytes from `to_be_bytes()` instead of
//! extracting individual hex digits (nibbles) from the Unicode code point.

// Removed facet_testhelpers::test to avoid conflicts

/// Something `facet-json` has no trouble parsing.
const OK_JSON: &str = "\"This is fine.\"";
/// The result of the successful parse.
const OK_EXPECTED: &str = "This is fine.";
/// Something `facet-json` does not parse correctly.
const FAIL_JSON: &str = "\"This\\u0020is fine.\"";
/// The expected result of the failed parse.
const FAIL_EXPECTED: &str = "This is fine.";

#[test]
fn parse_ok() -> Result<(), Box<dyn std::error::Error>> {
    println!("Attempting to parse {OK_JSON}");
    let parsed_ok = facet_json::from_str::<String>(OK_JSON)?;
    assert_eq!(parsed_ok, OK_EXPECTED);
    Ok(())
}

#[test]
fn parse_fail() -> Result<(), Box<dyn std::error::Error>> {
    println!("Attempting to parse {FAIL_JSON}");
    let parsed_fail = facet_json::from_str::<String>(FAIL_JSON)?;
    assert_eq!(parsed_fail, FAIL_EXPECTED);
    Ok(())
}

/// Test cases for various Unicode escape sequences
const UNICODE_TEST_CASES: &[(&str, &str)] = &[
    // Space character (U+0020)
    ("\"\\u0020\"", " "),
    // Emoji (U+1F60A - smiling face with smiling eyes)
    // Note: This would need surrogate pair support for proper testing
    // Multiple escape sequences in one string
    ("\"\\u0048\\u0065\\u006C\\u006C\\u006F\"", "Hello"),
    // Unicode escape followed by normal characters
    ("\"\\u0057orld\"", "World"),
    // Normal characters followed by Unicode escape
    ("\"Hello\\u0021\"", "Hello!"),
    // Mixed normal escapes and Unicode escapes
    ("\"\\u0048\\tello\\u0021\"", "H\tello!"),
];

#[test]
fn test_unicode_escapes() -> Result<(), Box<dyn std::error::Error>> {
    for (input, expected) in UNICODE_TEST_CASES {
        println!("Attempting to parse {input}");
        let parsed = facet_json::from_str::<String>(input)?;
        assert_eq!(&parsed, expected, "Failed on input: {input}");
    }
    Ok(())
}

/// Test cases for ASCII control character serialization
/// These test the specific code path that generates \u0000 escape sequences
const CONTROL_CHAR_TEST_CASES: &[(char, &str)] = &[
    // Null character (U+0000)
    ('\u{00}', "\"\\u0000\""),
    // Start of Heading (U+0001)
    ('\u{01}', "\"\\u0001\""),
    // Start of Text (U+0002)
    ('\u{02}', "\"\\u0002\""),
    // End of Text (U+0003)
    ('\u{03}', "\"\\u0003\""),
    // End of Transmission (U+0004)
    ('\u{04}', "\"\\u0004\""),
    // Enquiry (U+0005)
    ('\u{05}', "\"\\u0005\""),
    // Acknowledge (U+0006)
    ('\u{06}', "\"\\u0006\""),
    // Bell (U+0007)
    ('\u{07}', "\"\\u0007\""),
    // Vertical Tab (U+000B) - backspace and form feed have special handling
    ('\u{0B}', "\"\\u000b\""),
    // Shift Out (U+000E)
    ('\u{0E}', "\"\\u000e\""),
    // Shift In (U+000F)
    ('\u{0F}', "\"\\u000f\""),
    // Data Link Escape (U+0010)
    ('\u{10}', "\"\\u0010\""),
    // Device Control 1 (U+0011)
    ('\u{11}', "\"\\u0011\""),
    // Device Control 2 (U+0012)
    ('\u{12}', "\"\\u0012\""),
    // Device Control 3 (U+0013)
    ('\u{13}', "\"\\u0013\""),
    // Device Control 4 (U+0014)
    ('\u{14}', "\"\\u0014\""),
    // Negative Acknowledge (U+0015)
    ('\u{15}', "\"\\u0015\""),
    // Synchronous Idle (U+0016)
    ('\u{16}', "\"\\u0016\""),
    // End of Transmission Block (U+0017)
    ('\u{17}', "\"\\u0017\""),
    // Cancel (U+0018)
    ('\u{18}', "\"\\u0018\""),
    // End of Medium (U+0019)
    ('\u{19}', "\"\\u0019\""),
    // Substitute (U+001A)
    ('\u{1A}', "\"\\u001a\""),
    // Escape (U+001B)
    ('\u{1B}', "\"\\u001b\""),
    // File Separator (U+001C)
    ('\u{1C}', "\"\\u001c\""),
    // Group Separator (U+001D)
    ('\u{1D}', "\"\\u001d\""),
    // Record Separator (U+001E)
    ('\u{1E}', "\"\\u001e\""),
    // Unit Separator (U+001F)
    ('\u{1F}', "\"\\u001f\""),
    // Delete (U+007F)
    ('\u{7F}', "\"\\u007f\""),
];

#[test]
fn test_ascii_control_character_serialization() -> Result<(), Box<dyn std::error::Error>> {
    for (input_char, expected_json) in CONTROL_CHAR_TEST_CASES {
        let input_string = input_char.to_string();
        let serialized = facet_json::to_string(&input_string);
        assert_eq!(
            &serialized,
            expected_json,
            "Failed to serialize control character U+{:04X} ('{}')",
            *input_char as u32,
            input_char.escape_debug()
        );
    }
    Ok(())
}

#[test]
fn test_special_escape_sequences_not_affected() -> Result<(), Box<dyn std::error::Error>> {
    // These characters have specific escape sequences and should NOT use the \u0000 format
    let special_cases = &[
        ('\n', "\"\\n\""),     // Line Feed (U+000A)
        ('\r', "\"\\r\""),     // Carriage Return (U+000D)
        ('\t', "\"\\t\""),     // Tab (U+0009)
        ('\u{08}', "\"\\b\""), // Backspace (U+0008)
        ('\u{0C}', "\"\\f\""), // Form Feed (U+000C)
        ('"', "\"\\\"\""),     // Quote
        ('\\', "\"\\\\\""),    // Backslash
    ];

    for (input_char, expected_json) in special_cases {
        let input_string = input_char.to_string();
        let serialized = facet_json::to_string(&input_string);
        assert_eq!(
            &serialized, expected_json,
            "Special escape character U+{:04X} should use specific escape sequence",
            *input_char as u32
        );
    }
    Ok(())
}

#[test]
fn test_control_characters_in_mixed_strings() -> Result<(), Box<dyn std::error::Error>> {
    // Test control characters mixed with regular text
    let test_cases = &[
        ("Hello\u{00}World", "\"Hello\\u0000World\""),
        ("Start\u{01}\u{02}End", "\"Start\\u0001\\u0002End\""),
        ("Tab\tand\u{0B}VTab", "\"Tab\\tand\\u000bVTab\""),
        (
            "\u{1F}before and after\u{7F}",
            "\"\\u001fbefore and after\\u007f\"",
        ),
    ];

    for (input, expected) in test_cases {
        let serialized = facet_json::to_string(input);
        assert_eq!(&serialized, expected, "Failed on mixed string: {:?}", input);
    }
    Ok(())
}

#[test]
fn test_control_character_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Test that we can serialize and deserialize control characters
    for (input_char, _) in CONTROL_CHAR_TEST_CASES {
        let input_string = input_char.to_string();
        let serialized = facet_json::to_string(&input_string);
        let deserialized: String = facet_json::from_str(&serialized).map_err(|e| e.into_owned())?;

        assert_eq!(
            deserialized, input_string,
            "Roundtrip failed for control character U+{:04X}",
            *input_char as u32
        );
    }
    Ok(())
}

#[test]
fn test_hex_digit_generation() -> Result<(), Box<dyn std::error::Error>> {
    // Test that the hex digit generation is correct for edge cases
    let edge_cases = &[
        ('\u{00}', "0000"), // All zeros
        ('\u{0F}', "000f"), // Single hex digit f
        ('\u{10}', "0010"), // Hex 10
        ('\u{1F}', "001f"), // Hex 1f
        ('\u{7F}', "007f"), // Hex 7f (DEL character)
    ];

    for (input_char, expected_hex) in edge_cases {
        let input_string = input_char.to_string();
        let serialized = facet_json::to_string(&input_string);
        let expected = format!("\"\\u{}\"", expected_hex);

        assert_eq!(
            serialized, expected,
            "Hex generation failed for U+{:04X}",
            *input_char as u32
        );
    }
    Ok(())
}
