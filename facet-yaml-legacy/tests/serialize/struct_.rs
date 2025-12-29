//! Tests for TOML table values.

use eyre::Result;
use facet::Facet;

use crate::assert_serialize;

#[test]
fn test_table_to_struct() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
        table: Table,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Table {
        value: i32,
    }

    assert_serialize!(
        Root,
        Root {
            value: 1,
            table: Table { value: 2 },
        },
    );

    Ok(())
}

#[test]
fn test_root_struct_multiple_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        a: i32,
        b: Option<bool>,
        c: String,
    }

    assert_serialize!(
        Root,
        Root {
            a: 1,
            b: Some(true),
            c: "'' \"test ".to_string()
        },
    );

    Ok(())
}

#[test]
fn test_nested_struct_multiple_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        nested: Nested,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Nested {
        a: i32,
        b: bool,
        c: String,
    }

    assert_serialize!(
        Root,
        Root {
            nested: Nested {
                a: 1,
                b: true,
                c: "test".to_string()
            }
        },
    );

    Ok(())
}

#[test]
fn test_rename_single_struct_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        #[facet(rename = "1")]
        a: i32,
        #[facet(rename = "with spaces")]
        b: bool,
        #[facet(rename = "'quoted'")]
        c: String,
        #[facet(rename = "")]
        d: usize,
    }

    assert_serialize!(
        Root,
        Root {
            a: 1,
            b: true,
            c: "quoted".parse().unwrap(),
            d: 2
        },
    );

    Ok(())
}

#[test]
fn test_rename_all_struct_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct Root {
        a_number: i32,
        another_bool: bool,
        #[facet(rename = "Overwrite")]
        shouldnt_matter: f32,
    }

    assert_serialize!(
        Root,
        Root {
            a_number: 1,
            another_bool: true,
            shouldnt_matter: 1.0
        },
    );

    Ok(())
}

#[test]
fn test_default_struct_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        #[facet(default)]
        a: i32,
        #[facet(default)]
        b: bool,
        #[facet(default)]
        c: String,
    }

    assert_serialize!(
        Root,
        Root {
            a: i32::default(),
            b: bool::default(),
            c: "hi".to_string()
        },
    );

    Ok(())
}

#[test]
#[ignore = "must be fixed in deserialize"]
fn test_optional_default_struct_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        #[facet(default)]
        a: Option<i32>,
        #[facet(default)]
        b: Option<bool>,
        #[facet(default = Some("hi".to_owned()))]
        c: Option<String>,
    }

    assert_serialize!(
        Root,
        Root {
            a: None,
            b: Some(bool::default()),
            c: Some("hi".to_string())
        },
    );

    Ok(())
}

#[test]
fn test_skip_serializing_if_none() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Step {
        name: String,
        #[facet(default, skip_serializing_if = Option::is_none)]
        run: Option<String>,
    }

    // When run is None, it should be omitted from the output
    let step = Step {
        name: "Checkout".to_string(),
        run: None,
    };
    let yaml = facet_yaml_legacy::to_string(&step)?;
    assert!(
        !yaml.contains("run"),
        "run field should be omitted when None"
    );
    assert!(
        yaml.contains("name: Checkout"),
        "name field should be present"
    );

    // When run is Some, it should be included
    let step_with_run = Step {
        name: "Build".to_string(),
        run: Some("cargo build".to_string()),
    };
    let yaml_with_run = facet_yaml_legacy::to_string(&step_with_run)?;
    assert!(
        yaml_with_run.contains("run: cargo build"),
        "run field should be present when Some"
    );

    Ok(())
}

#[test]
fn test_skip_serializing_if_custom_predicate() -> Result<()> {
    facet_testhelpers::setup();

    fn is_empty(s: &str) -> bool {
        s.is_empty()
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        #[facet(default, skip_serializing_if = is_empty)]
        description: String,
    }

    // When description is empty, it should be omitted
    let config = Config {
        name: "test".to_string(),
        description: String::new(),
    };
    let yaml = facet_yaml_legacy::to_string(&config)?;
    assert!(
        !yaml.contains("description"),
        "description should be omitted when empty"
    );

    // When description is not empty, it should be included
    let config_with_desc = Config {
        name: "test".to_string(),
        description: "A test config".to_string(),
    };
    let yaml_with_desc = facet_yaml_legacy::to_string(&config_with_desc)?;
    assert!(
        yaml_with_desc.contains("description: A test config"),
        "description should be present when not empty"
    );

    Ok(())
}
