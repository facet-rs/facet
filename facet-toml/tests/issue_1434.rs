/// Test for issue #1434: Spanned<T> in multi-variant untagged enums (facet-toml)
///
/// This test verifies that the fix in facet-solver also works for facet-toml
use facet::Facet;
use facet_toml as toml;
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
fn test_format_toml_multi_scalar_spanned_bool() {
    let toml_str = r#"value = true"#;

    #[derive(Facet, Debug)]
    struct Config {
        value: DebugLevel,
    }

    let result: Config = toml::from_str(toml_str).expect("should deserialize as Bool variant");

    match result.value {
        DebugLevel::Bool(spanned_bool) => {
            assert!(*spanned_bool);
        }
        _ => panic!("Expected Bool variant"),
    }
}

#[test]
fn test_format_toml_multi_scalar_spanned_number() {
    let toml_str = r#"value = 2"#;

    #[derive(Facet, Debug)]
    struct Config {
        value: DebugLevel,
    }

    let result: Config = toml::from_str(toml_str).expect("should deserialize as Number variant");

    match result.value {
        DebugLevel::Number(spanned_num) => {
            assert_eq!(*spanned_num, 2);
        }
        _ => panic!("Expected Number variant"),
    }
}

#[test]
fn test_format_toml_multi_scalar_spanned_string() {
    let toml_str = r#"value = "full""#;

    #[derive(Facet, Debug)]
    struct Config {
        value: DebugLevel,
    }

    let result: Config = toml::from_str(toml_str).expect("should deserialize as String variant");

    match result.value {
        DebugLevel::String(spanned_str) => {
            assert_eq!(*spanned_str, "full");
        }
        _ => panic!("Expected String variant"),
    }
}
