//! Regression tests for escape sequence handling in JSON strings.
//!
//! See: https://github.com/bearcove/facet/issues/1891

/// Test that escape sequences after multi-byte UTF-8 characters are correctly decoded.
#[test]
fn test_escape_after_multibyte_utf8() {
    // The original bug: "中文\n" would deserialize with literal backslash-n
    // instead of a newline character.
    let original = "中文\n".to_string();
    let json = facet_json::to_string(&original).unwrap();
    let roundtrip: String = facet_json::from_str(&json).unwrap();
    assert_eq!(original, roundtrip);
    assert_eq!(roundtrip.as_bytes().last(), Some(&b'\n'));
}

/// Test various escape sequences following Unicode characters.
#[test]
fn test_various_escapes_after_unicode() {
    let test_cases = [
        ("中文\t", "tab after Chinese"),
        ("日本語\\", "backslash after Japanese"),
        ("한글\"", "quote after Korean"),
        ("中文\u{0041}", "unicode escape \\u0041 (A) after Chinese"),
        ("émoji\r\n", "CRLF after accented char"),
    ];

    for (original, desc) in test_cases {
        let json = facet_json::to_string(&original).unwrap();
        let roundtrip: String = facet_json::from_str(&json).unwrap();
        assert_eq!(original, roundtrip, "failed for: {}", desc);
    }
}

/// Test multiple escape sequences interspersed with Unicode.
#[test]
fn test_interspersed_unicode_and_escapes() {
    let original = "你好\n世界\t再见\n";
    let json = facet_json::to_string(&original).unwrap();
    let roundtrip: String = facet_json::from_str(&json).unwrap();
    assert_eq!(original, roundtrip);
}

/// Test emoji followed by escape sequences.
#[test]
fn test_emoji_followed_by_escapes() {
    let test_cases = [
        ("🎉\n", "party emoji then newline"),
        ("👨‍👩‍👧‍👦\t", "family emoji (ZWJ sequence) then tab"),
        ("🇺🇸\\", "flag emoji then backslash"),
        ("😀\r\n🎊", "emoji, CRLF, emoji"),
    ];

    for (original, desc) in test_cases {
        let json = facet_json::to_string(&original).unwrap();
        let roundtrip: String = facet_json::from_str(&json).unwrap();
        assert_eq!(original, roundtrip, "failed for: {}", desc);
    }
}

/// Test that pure ASCII strings with escapes still work.
#[test]
fn test_ascii_escapes_still_work() {
    let original = "hello\nworld\ttab\\backslash\"quote";
    let json = facet_json::to_string(&original).unwrap();
    let roundtrip: String = facet_json::from_str(&json).unwrap();
    assert_eq!(original, roundtrip);
}

/// Test escape sequences before Unicode (should have always worked, but verify).
#[test]
fn test_escapes_before_unicode() {
    let original = "\n中文";
    let json = facet_json::to_string(&original).unwrap();
    let roundtrip: String = facet_json::from_str(&json).unwrap();
    assert_eq!(original, roundtrip);
}
