//! Regression test for issue #1404: facet-toml untagged enums fail to parse nested renamed enum variants

use facet::Facet;

#[test]
fn test_untagged_enum_with_renamed_nested_enum() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Edition {
        #[facet(rename = "2021")]
        E2021,
        #[facet(rename = "2024")]
        E2024,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct WorkspaceRef {
        workspace: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    pub enum EditionOrWorkspace {
        Edition(Edition),
        Workspace(WorkspaceRef),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        edition: EditionOrWorkspace,
    }

    // This should work - struct variant
    let toml1 = r#"edition = { workspace = true }"#;
    let config1: Config = facet_toml::from_str(toml1).unwrap();
    assert!(matches!(
        config1.edition,
        EditionOrWorkspace::Workspace(WorkspaceRef { workspace: true })
    ));

    // This should also work - enum variant with renamed value
    let toml2 = r#"edition = "2024""#;
    let config2: Config = facet_toml::from_str(toml2).unwrap();
    assert!(matches!(
        config2.edition,
        EditionOrWorkspace::Edition(Edition::E2024)
    ));

    // And this should work too
    let toml3 = r#"edition = "2021""#;
    let config3: Config = facet_toml::from_str(toml3).unwrap();
    assert!(matches!(
        config3.edition,
        EditionOrWorkspace::Edition(Edition::E2021)
    ));
}
