//! Tests for TOML document parsing.

use facet_toml::TomlDeErrorKind;

#[test]
fn test_invalid_toml() {
    facet_testhelpers::setup();

    assert!(matches!(
        facet_toml::from_str::<()>("invalid toml").unwrap_err().kind,
        // We don't check on the error message here because it can change upstream
        TomlDeErrorKind::GenericTomlError(_)
    ));
}
