/// Test for issue #1434: Spanned<T> doesn't work in multi-variant untagged enums
///
/// This test reproduces the case where:
/// - An untagged enum has multiple scalar variants (all Spanned<T>)
/// - Deserializing should try each variant until one matches
/// - Currently fails because the deserializer doesn't fallthrough
use facet::Facet;
use facet_json_legacy as json;
use facet_reflect::Spanned;

/// An enum with multiple scalar types, like Cargo.toml's debug setting
#[derive(Facet, Debug)]
#[repr(u8)]
#[facet(untagged)]
pub enum DebugLevel {
    /// Boolean: debug = true
    Bool(Spanned<bool>),
    /// Number: debug = 2
    Number(Spanned<u8>),
    /// String: debug = "full"
    String(Spanned<String>),
}

#[test]
fn test_multi_scalar_spanned_bool() {
    let json = r#"true"#;
    let result: DebugLevel = json::from_str(json).expect("should deserialize as Bool variant");

    match result {
        DebugLevel::Bool(spanned_bool) => {
            assert!(*spanned_bool);
            assert!(spanned_bool.span.len > 0);
        }
        _ => panic!("Expected Bool variant"),
    }
}

#[test]
fn test_multi_scalar_spanned_number() {
    // This is the failing case from the issue
    let json = r#"2"#;
    let result: DebugLevel = json::from_str(json).expect("should deserialize as Number variant");

    match result {
        DebugLevel::Number(spanned_num) => {
            assert_eq!(*spanned_num, 2);
            assert!(spanned_num.span.len > 0);
        }
        _ => panic!("Expected Number variant, got a different variant"),
    }
}

#[test]
fn test_multi_scalar_spanned_string() {
    let json = r#""full""#;
    let result: DebugLevel = json::from_str(json).expect("should deserialize as String variant");

    match result {
        DebugLevel::String(spanned_str) => {
            assert_eq!(*spanned_str, "full");
            assert!(spanned_str.span.len > 0);
        }
        _ => panic!("Expected String variant"),
    }
}

/// Test without Spanned to verify the issue is Spanned-specific
#[derive(Facet, Debug)]
#[repr(u8)]
#[facet(untagged)]
pub enum DebugLevelUnwrapped {
    Bool(bool),
    Number(u8),
    String(String),
}

#[test]
fn test_multi_scalar_unwrapped_works() {
    // Verify that without Spanned, this works fine
    let json = r#"2"#;
    let result: DebugLevelUnwrapped = json::from_str(json).expect("unwrapped version should work");

    match result {
        DebugLevelUnwrapped::Number(num) => {
            assert_eq!(num, 2);
        }
        _ => panic!("Expected Number variant"),
    }
}
