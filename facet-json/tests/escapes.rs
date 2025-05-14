use facet_testhelpers::test;

/// Something `facet-json` has no trouble parsing.
const OK_JSON: &str = "\"This is fine.\"";
/// The result of the successful parse.
const OK_EXPECTED: &str = "This is fine.";
/// Something `facet-json` does not parse correctly.
const FAIL_JSON: &str = "\"This\\u0020is fine.\"";
/// The expected result of the failed parse.
const FAIL_EXPECTED: &str = "This is fine.";

#[test]
fn parse_ok() {
    println!("Attempting to parse {OK_JSON}");
    let parsed_ok = facet_json::from_str::<String>(OK_JSON)
        .map_err(|err| eyre::eyre!("Could not parse {OK_JSON:?}: {err}"))?;
    assert_eq!(parsed_ok, OK_EXPECTED);
}

#[test]
fn parse_fail() {
    println!("Attempting to parse {FAIL_JSON}");
    let parsed_fail = facet_json::from_str::<String>(FAIL_JSON)
        .map_err(|err| eyre::eyre!("Could not parse {FAIL_JSON:?}: {err}"))?;
    assert_eq!(parsed_fail, FAIL_EXPECTED);
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
fn test_unicode_escapes() {
    for (input, expected) in UNICODE_TEST_CASES {
        println!("Attempting to parse {input}");
        let parsed = facet_json::from_str::<String>(input)
            .map_err(|err| eyre::eyre!("Could not parse {input:?}: {err}"))?;
        assert_eq!(&parsed, expected, "Failed on input: {input}");
    }
}
